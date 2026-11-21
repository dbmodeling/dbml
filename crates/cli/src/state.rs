use anyhow::Context;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{RwLock, broadcast};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

#[must_use]
pub fn compute_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Realiza gravação atômica em disco utilizando arquivo temporário e rename do sistema de arquivos.
///
/// # Errors
///
/// Returns an error if writing the temporary file or renaming it to `path` fails.
pub async fn atomic_write(path: &Path, content: &str) -> anyhow::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_path = parent.join(format!(".{file_name}.tmp-{}-{seq}", std::process::id()));

    tokio::fs::write(&tmp_path, content)
        .await
        .with_context(|| format!("Failed to write temporary file '{}'", tmp_path.display()))?;

    if let Err(err) = tokio::fs::rename(&tmp_path, path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(err).with_context(|| format!("Failed to rename file to '{}'", path.display()));
    }

    Ok(())
}

/// Gera o token de sessão desta execução: 128 bits do gerador do sistema operacional em hex.
///
/// # Panics
///
/// Entra em pânico se o sistema operacional não fornecer entropia (o servidor não deve subir
/// sem um token imprevisível).
#[must_use]
pub fn generate_token() -> String {
    use std::fmt::Write as _;
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("o sistema operacional não forneceu entropia");
    bytes.iter().fold(String::with_capacity(32), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Aplica um patch textual a `current`, recusando versão desatualizada ou intervalo inválido.
///
/// # Errors
///
/// Retorna erro se `current` difere de `expected` ou se `start..end` não é um intervalo válido
/// em fronteiras de caractere.
pub fn patch_text(
    mut current: String,
    expected: &str,
    start: usize,
    end: usize,
    text: &str,
) -> anyhow::Result<String> {
    if current != expected {
        anyhow::bail!("Document changed outside this editor; reload or reconcile changes");
    }
    if start > end
        || end > current.len()
        || !current.is_char_boundary(start)
        || !current.is_char_boundary(end)
    {
        anyhow::bail!("Invalid patch range");
    }
    current.replace_range(start..end, text);
    Ok(current)
}

/// Estado do documento ativo. Todos os campos mudam juntos sob um único lock
/// ([`AppState::doc`]), então arquivo, sidecar, conteúdo e hash nunca ficam dessincronizados.
#[derive(Debug, Clone)]
pub struct DocState {
    pub file_path: PathBuf,
    pub layout_path: PathBuf,
    pub content: String,
    pub layout: Option<String>,
    pub last_saved_hash: u64,
}

impl DocState {
    /// Nome do arquivo ativo para exibição.
    #[must_use]
    pub fn title(&self) -> &str {
        self.file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("schema.dbml")
    }
}

/// Mensagem `init`/`update` enviada aos clientes com o documento completo.
#[derive(Serialize)]
struct DocumentMessage<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    source: &'a str,
    layout: Option<serde_json::Value>,
    title: &'a str,
    #[serde(rename = "documentId")]
    document_id: std::borrow::Cow<'a, str>,
    saved: bool,
    files: Vec<serde_json::Value>,
}

#[derive(Clone)]
pub struct AppState {
    /// Token de sessão exigido em `/ws` e `/api/*`.
    pub token: Arc<str>,
    pub target_path: PathBuf,
    pub project_dir: Option<PathBuf>,
    /// Documento ativo (arquivo, sidecar, conteúdo, layout e hash) sob um único lock.
    pub doc: Arc<RwLock<DocState>>,
    pub tx: broadcast::Sender<String>,
}

async fn scan_dbml_dir(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return out;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("dbml")
            && entry.file_type().await.is_ok_and(|t| t.is_file())
        {
            out.push(p);
        }
    }
    out.sort();
    out
}

fn layout_path_for(file: &Path) -> PathBuf {
    PathBuf::from(format!("{}.layout.json", file.display()))
}

impl AppState {
    /// Initializes [`AppState`] for a `.dbml` file or project directory.
    ///
    /// # Errors
    ///
    /// Returns an error if creating or reading the active `.dbml` file fails.
    pub async fn new(target: PathBuf) -> anyhow::Result<Self> {
        let target_abs = std::fs::canonicalize(&target).unwrap_or(target);
        let (project_dir, file_path) = if target_abs.is_dir() {
            let files = scan_dbml_dir(&target_abs).await;
            let session = crate::session::load();
            let chosen = session
                .active_file
                .filter(|p| p.is_file() && p.starts_with(&target_abs))
                .or_else(|| files.into_iter().next())
                .unwrap_or_else(|| target_abs.join("schema.dbml"));
            (Some(target_abs.clone()), chosen)
        } else {
            let parent = target_abs.parent().map(Path::to_path_buf);
            (parent, target_abs.clone())
        };

        let layout_path = layout_path_for(&file_path);

        // Cria arquivo com template inicial se não existir
        if !file_path.exists() {
            let initial_dbml = "// DBML Document\n\nTable users {\n  id int [pk]\n  name varchar\n  email varchar [unique]\n}\n\nTable posts {\n  id int [pk]\n  user_id int\n  title varchar\n}\n\nRef: posts.user_id > users.id\n";
            atomic_write(&file_path, initial_dbml).await?;
        }

        let content = tokio::fs::read_to_string(&file_path)
            .await
            .with_context(|| format!("Could not read file '{}'", file_path.display()))?;

        let hash = compute_hash(&content);

        let layout = if layout_path.exists() {
            tokio::fs::read_to_string(&layout_path).await.ok()
        } else {
            None
        };

        let (tx, _) = broadcast::channel(100);

        crate::session::save(&crate::session::CliSession {
            last_target: Some(target_abs.clone()),
            active_file: Some(file_path.clone()),
        });

        Ok(Self {
            token: generate_token().into(),
            target_path: target_abs,
            project_dir,
            doc: Arc::new(RwLock::new(DocState {
                file_path,
                layout_path,
                content,
                layout,
                last_saved_hash: hash,
            })),
            tx,
        })
    }

    /// Token de sessão desta execução (exigido em `/ws` e `/api/*`).
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Lista os `.dbml` do workspace marcando `active`.
    pub async fn workspace_files(&self, active: &Path) -> Vec<serde_json::Value> {
        let Some(dir) = &self.project_dir else {
            return Vec::new();
        };
        scan_dbml_dir(dir)
            .await
            .into_iter()
            .map(|p| {
                let name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("schema.dbml")
                    .to_string();
                let is_active = p == active;
                serde_json::json!({
                    "name": name,
                    "path": p.to_string_lossy(),
                    "active": is_active,
                })
            })
            .collect()
    }

    /// Monta a mensagem (`init` ou `update`) com o documento completo a partir de `doc`.
    ///
    /// # Panics
    ///
    /// Nunca: a serialização de valores JSON em memória não falha.
    pub async fn document_message(&self, kind: &str, doc: &DocState) -> String {
        let msg = DocumentMessage {
            kind,
            source: &doc.content,
            layout: doc
                .layout
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok()),
            title: doc.title(),
            document_id: doc.file_path.to_string_lossy(),
            saved: true,
            files: self.workspace_files(&doc.file_path).await,
        };
        serde_json::to_string(&msg).expect("serialização de DocumentMessage não falha")
    }

    /// Mensagem `init` do estado atual (para um cliente recém-conectado).
    pub async fn init_message(&self) -> String {
        let doc = self.doc.read().await;
        self.document_message("init", &doc).await
    }

    /// Switches the active document to `new_path` within the active workspace and broadcasts an
    /// `update` message describing the new document (sent while the document lock is held, so
    /// clients see switches in order).
    ///
    /// # Errors
    ///
    /// Returns an error if `new_path` is outside the workspace, is not a file, or cannot be read.
    pub async fn switch_file(&self, new_path: PathBuf) -> anyhow::Result<()> {
        let canonical = tokio::fs::canonicalize(&new_path).await.unwrap_or(new_path);
        let allowed = canonical == self.target_path
            || self
                .project_dir
                .as_ref()
                .is_some_and(|dir| canonical.starts_with(dir));
        if !allowed {
            anyhow::bail!(
                "Access denied: '{}' is outside the active workspace",
                canonical.display()
            );
        }
        if !canonical.is_file() {
            anyhow::bail!("File not found: {}", canonical.display());
        }
        let mut doc = self.doc.write().await;
        let new_layout_path = layout_path_for(&canonical);
        let new_content = tokio::fs::read_to_string(&canonical).await?;
        let new_layout = if new_layout_path.exists() {
            tokio::fs::read_to_string(&new_layout_path).await.ok()
        } else {
            None
        };

        doc.last_saved_hash = compute_hash(&new_content);
        doc.content = new_content;
        doc.layout = new_layout;
        doc.layout_path = new_layout_path;
        doc.file_path.clone_from(&canonical);

        crate::session::save(&crate::session::CliSession {
            last_target: Some(self.target_path.clone()),
            active_file: Some(canonical),
        });

        let update = self.document_message("update", &doc).await;
        let _ = self.tx.send(update);
        Ok(())
    }

    fn ensure_active(doc: &DocState, expected_file: &str) -> anyhow::Result<()> {
        if doc.file_path.to_string_lossy() == expected_file {
            Ok(())
        } else {
            anyhow::bail!("Active file changed; reload or reconcile changes")
        }
    }

    /// Lê o arquivo ativo, valida e aplica o patch e persiste o resultado tudo sob um único
    /// lock de escrita, de modo que dois clientes não percam a edição um do outro.
    ///
    /// # Errors
    ///
    /// Retorna erro se o arquivo ativo mudou, se o documento diverge de `expected_source`, se o
    /// intervalo é inválido ou se a gravação falha.
    pub async fn apply_patch(
        &self,
        expected_file: &str,
        expected_source: &str,
        start: usize,
        end: usize,
        new_text: &str,
    ) -> anyhow::Result<()> {
        let mut doc = self.doc.write().await;
        Self::ensure_active(&doc, expected_file)?;
        let current = tokio::fs::read_to_string(&doc.file_path).await?;
        let updated = patch_text(current, expected_source, start, end, new_text)?;
        Self::persist(&mut doc, updated).await
    }

    /// Confere que o arquivo em disco ainda é `expected_source` (o botão "salvar" do front).
    ///
    /// # Errors
    ///
    /// Retorna erro se o arquivo ativo mudou ou o conteúdo em disco diverge.
    pub async fn verify_document(
        &self,
        expected_file: &str,
        expected_source: &str,
    ) -> anyhow::Result<()> {
        let doc = self.doc.read().await;
        Self::ensure_active(&doc, expected_file)?;
        if tokio::fs::read_to_string(&doc.file_path).await? != expected_source {
            anyhow::bail!("Document changed outside this editor; reload or reconcile changes");
        }
        Ok(())
    }

    /// Atomically persists `new_content` to the active `.dbml` file and updates in-memory state.
    ///
    /// # Errors
    ///
    /// Returns an error if the atomic write fails.
    pub async fn save_content(&self, new_content: String) -> anyhow::Result<()> {
        let mut doc = self.doc.write().await;
        Self::persist(&mut doc, new_content).await
    }

    async fn persist(doc: &mut DocState, new_content: String) -> anyhow::Result<()> {
        atomic_write(&doc.file_path, &new_content).await?;
        doc.last_saved_hash = compute_hash(&new_content);
        doc.content = new_content;
        Ok(())
    }

    /// Atomically persists `layout_json` to the `.layout.json` sidecar file.
    ///
    /// # Errors
    ///
    /// Returns an error if the atomic write fails.
    pub async fn save_layout(&self, layout_json: String) -> anyhow::Result<()> {
        let mut doc = self.doc.write().await;
        atomic_write(&doc.layout_path, &layout_json).await?;
        doc.layout = Some(layout_json);
        Ok(())
    }

    /// Como [`Self::save_layout`], mas só grava se `expected_file` ainda é o arquivo ativo.
    ///
    /// # Errors
    ///
    /// Retorna erro se o arquivo ativo mudou ou a gravação falha.
    pub async fn save_layout_for(
        &self,
        expected_file: &str,
        layout_json: String,
    ) -> anyhow::Result<()> {
        let mut doc = self.doc.write().await;
        Self::ensure_active(&doc, expected_file)?;
        atomic_write(&doc.layout_path, &layout_json).await?;
        doc.layout = Some(layout_json);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::patch_text;

    #[test]
    fn rejects_stale_and_invalid_patches() {
        assert!(patch_text("new".into(), "old", 0, 3, "x").is_err());
        assert!(patch_text("é".into(), "é", 1, 2, "x").is_err());
        assert_eq!(patch_text("é".into(), "é", 0, 2, "x").unwrap(), "x");
    }
}
