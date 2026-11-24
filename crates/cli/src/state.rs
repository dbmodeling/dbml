use crate::constants::{GIT_BASE_ERR_OTHER, WS_MSG_GIT_BASE_RESULT};
use crate::git_base::read_head_blob;
use anyhow::Context;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{OwnedRwLockWriteGuard, RwLock, broadcast};

#[must_use]
pub fn compute_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Realiza gravação atômica em disco preservando symlinks e permissões (via `atomic-write`).
///
/// # Errors
///
/// Returns an error if writing the temporary file or renaming it to `path` fails.
pub async fn atomic_write(path: &Path, content: &str) -> anyhow::Result<()> {
    let path = path.to_path_buf();
    let bytes = content.as_bytes().to_vec();
    tokio::task::spawn_blocking(move || {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.is_dir()
        {
            anyhow::bail!("Parent directory '{}' does not exist", parent.display());
        }
        atomic_write::atomic_write(&path, &bytes)
            .with_context(|| format!("Failed to write file '{}'", path.display()))
    })
    .await
    .map_err(|e| anyhow::anyhow!("atomic write task failed: {e}"))?
}

/// Idade mínima para um arquivo temporário de gravação atômica ser considerado abandonado
/// (uma gravação em andamento dura milissegundos; outro processo pode estar escrevendo agora).
pub const STALE_TEMP_AGE: std::time::Duration = std::time::Duration::from_secs(60);

/// Reconhece `.<nome>.tmp-<pid>-<seq>` com `<nome>` terminando em `.dbml` ou `.dbml.layout.json`
/// (o padrão criado por [`atomic_write`] para os arquivos que este servidor grava; a comparação
/// é exata de propósito, pois o nome é gerado por nós).
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub(crate) fn is_atomic_write_leftover(file_name: &str) -> bool {
    let Some(rest) = file_name.strip_prefix('.') else {
        return false;
    };
    let Some((name, suffix)) = rest.rsplit_once(".tmp-") else {
        return false;
    };
    let Some((pid, seq)) = suffix.split_once('-') else {
        return false;
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    digits(pid) && digits(seq) && (name.ends_with(".dbml") || name.ends_with(".dbml.layout.json"))
}

/// Remove de `dir` os temporários de gravações atômicas interrompidas (processo morto entre a
/// escrita e o rename) com mais de `min_age`. Devolve quantos foram removidos.
pub async fn clean_stale_temp_files(dir: &Path, min_age: std::time::Duration) -> usize {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return 0;
    };
    let mut removed = 0;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        if !name.to_str().is_some_and(is_atomic_write_leftover) {
            continue;
        }
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        let old_enough = meta
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age >= min_age);
        if meta.is_file() && old_enough && tokio::fs::remove_file(entry.path()).await.is_ok() {
            removed += 1;
        }
    }
    removed
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
    #[expect(
        clippy::expect_used,
        reason = "server refuses to start without OS cryptographic entropy"
    )]
    getrandom::fill(&mut bytes).expect("o sistema operacional não forneceu entropia");
    bytes.iter().fold(String::with_capacity(32), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Aplica um patch textual a `current` no lugar, recusando versão desatualizada ou intervalo
/// inválido. Em caso de erro `current` não é alterado.
///
/// # Errors
///
/// Retorna erro se `current` difere de `expected` ou se `start..end` não é um intervalo válido
/// em fronteiras de caractere.
pub fn patch_in_place(
    current: &mut String,
    expected: &str,
    start: usize,
    end: usize,
    text: &str,
) -> anyhow::Result<()> {
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
    Ok(())
}

/// Executa `work` em uma task própria e espera o resultado. Se quem chamou for cancelado (por
/// exemplo, o `abort` da task do socket ao fechar a conexão), a gravação em andamento termina
/// por inteiro em vez de parar no meio entre o arquivo temporário e o rename.
async fn detach<T, F>(work: F) -> anyhow::Result<T>
where
    T: Send + 'static,
    F: std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
{
    tokio::spawn(work)
        .await
        .map_err(|e| anyhow::anyhow!("write task failed: {e}"))?
}

/// Capacidade do canal de difusão; ao estourar, o cliente atrasado recebe o estado atual.
pub const BROADCAST_CAPACITY: usize = 16;

/// Tempo que as edições (`apply_patch`) esperam antes de ir para o disco; rajadas de patches
/// viram uma única gravação atômica. Salvar, trocar de arquivo e encerrar gravam na hora.
pub const WRITE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(150);

/// Estado do documento ativo. Todos os campos mudam juntos sob um único lock
/// ([`AppState::doc`]), então arquivo, sidecar, conteúdo e hash nunca ficam dessincronizados.
#[derive(Debug, Clone)]
pub struct DocState {
    pub file_path: PathBuf,
    pub layout_path: PathBuf,
    pub content: String,
    pub layout: Option<String>,
    /// Hash do que está gravado em disco (usado pelo watcher para ignorar as próprias gravações).
    pub last_saved_hash: u64,
    /// `content` tem edições ainda não gravadas (gravação adiada, ver [`WRITE_DEBOUNCE`]).
    pub dirty: bool,
    /// Já existe uma gravação adiada agendada.
    pub flush_scheduled: bool,
    /// Versão do texto do documento: começa em [`INITIAL_VERSION`] e cresce de 1 em 1 a cada
    /// mudança de texto (patch, alteração externa vista pelo watcher, troca de arquivo,
    /// [`AppState::save_content`]). Patches e salvamentos citam a versão em que foram calculados.
    pub version: u64,
}

/// Versão do documento logo depois de aberto.
pub const INITIAL_VERSION: u64 = 1;

/// Patch ou salvamento calculado sobre uma versão que não é mais a atual. Carrega o estado atual
/// (mensagem `patch_rejected` já serializada) para o servidor devolver ao cliente que errou.
#[derive(Debug)]
pub struct StaleVersion {
    pub base_version: u64,
    pub current_version: u64,
    /// Mensagem `patch_rejected` com o documento atual (montada sob o lock do documento).
    pub rejection: String,
}

impl std::fmt::Display for StaleVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Document version changed (client has {}, current is {}); state resynced, reapply your change",
            self.base_version, self.current_version
        )
    }
}

impl std::error::Error for StaleVersion {}

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
    version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

#[derive(Clone)]
pub struct AppState {
    /// Token de sessão exigido em `/ws` e `/api/*`.
    pub token: Arc<str>,
    /// Modo demo pública: `/ws` e `/api/*` dispensam o token (ver `--public-demo`).
    pub public_demo: bool,
    /// Máximo de conexões WebSocket simultâneas (`--max-connections`).
    pub max_ws_connections: usize,
    pub target_path: PathBuf,
    pub project_dir: Option<PathBuf>,
    /// Documento ativo (arquivo, sidecar, conteúdo, layout e hash) sob um único lock.
    pub doc: Arc<RwLock<DocState>>,
    /// Mensagens para todos os clientes. `Arc<str>` evita copiar o documento por receptor e a
    /// capacidade é pequena: quem atrasa recebe o estado atual em vez de um histórico.
    pub tx: broadcast::Sender<Arc<str>>,
    /// Tasks em segundo plano (sockets, watcher) e sinal de encerramento.
    pub lifecycle: Arc<crate::lifecycle::Lifecycle>,
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

async fn is_file(path: &Path) -> bool {
    tokio::fs::metadata(path).await.is_ok_and(|m| m.is_file())
}

async fn path_exists(path: &Path) -> bool {
    tokio::fs::try_exists(path).await.unwrap_or(false)
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
        let target_abs = tokio::fs::canonicalize(&target).await.unwrap_or(target);
        let target_is_dir = tokio::fs::metadata(&target_abs)
            .await
            .is_ok_and(|m| m.is_dir());
        let (project_dir, file_path) = if target_is_dir {
            let files = scan_dbml_dir(&target_abs).await;
            let session = crate::session::load_async().await;
            let remembered = match session.active_file {
                Some(p) if p.starts_with(&target_abs) && is_file(&p).await => Some(p),
                _ => None,
            };
            let chosen = remembered
                .or_else(|| files.into_iter().next())
                .unwrap_or_else(|| target_abs.join("schema.dbml"));
            (Some(target_abs.clone()), chosen)
        } else {
            let parent = target_abs.parent().map(Path::to_path_buf);
            (parent, target_abs.clone())
        };

        let layout_path = layout_path_for(&file_path);

        // Sobras de gravações atômicas interrompidas numa execução anterior.
        if let Some(dir) = file_path.parent() {
            clean_stale_temp_files(dir, STALE_TEMP_AGE).await;
        }

        // Cria arquivo com template inicial se não existir
        if !path_exists(&file_path).await {
            let initial_dbml = "// DBML Document\n\nTable users {\n  id int [pk]\n  name varchar\n  email varchar [unique]\n}\n\nTable posts {\n  id int [pk]\n  user_id int\n  title varchar\n}\n\nRef: posts.user_id > users.id\n";
            atomic_write(&file_path, initial_dbml).await?;
        }

        let content = tokio::fs::read_to_string(&file_path)
            .await
            .with_context(|| format!("Could not read file '{}'", file_path.display()))?;

        let hash = compute_hash(&content);

        let layout = tokio::fs::read_to_string(&layout_path).await.ok();

        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);

        crate::session::save_async(crate::session::CliSession {
            last_target: Some(target_abs.clone()),
            active_file: Some(file_path.clone()),
        })
        .await;

        Ok(Self {
            token: generate_token().into(),
            public_demo: false,
            max_ws_connections: crate::server::MAX_WS_CONNECTIONS,
            target_path: target_abs,
            project_dir,
            doc: Arc::new(RwLock::new(DocState {
                file_path,
                layout_path,
                content,
                layout,
                last_saved_hash: hash,
                dirty: false,
                flush_scheduled: false,
                version: INITIAL_VERSION,
            })),
            tx,
            lifecycle: Arc::default(),
        })
    }

    /// Encerra o servidor: sinaliza sockets e watcher, espera as tasks rastreadas (até `grace`) e
    /// grava as edições ainda pendentes (o que também aguarda gravações em andamento, pois elas
    /// seguram o lock do documento).
    pub async fn shutdown(&self, grace: std::time::Duration) {
        self.lifecycle.shutdown(grace).await;
        if let Err(err) = self.flush().await {
            tracing::error!("Failed to save pending changes on shutdown: {err:#}");
        }
    }

    /// Grava agora as edições pendentes do documento ativo (no-op se não há nenhuma).
    ///
    /// # Errors
    ///
    /// Retorna erro se a gravação atômica falha; as edições continuam pendentes em memória.
    pub async fn flush(&self) -> anyhow::Result<()> {
        let doc = self.write_doc().await;
        detach(async move {
            let mut doc = doc;
            Self::flush_locked(&mut doc).await
        })
        .await
    }

    async fn flush_locked(doc: &mut DocState) -> anyhow::Result<()> {
        if !doc.dirty {
            return Ok(());
        }
        atomic_write(&doc.file_path, &doc.content).await?;
        doc.last_saved_hash = compute_hash(&doc.content);
        doc.dirty = false;
        Ok(())
    }

    /// Agenda a gravação adiada: espera [`WRITE_DEBOUNCE`] (ou o início do encerramento) e grava.
    fn schedule_flush(&self) {
        let state = self.clone();
        let mut shutdown = self.lifecycle.subscribe();
        self.lifecycle.spawn(async move {
            tokio::select! {
                () = tokio::time::sleep(WRITE_DEBOUNCE) => {}
                _ = shutdown.changed() => {}
            }
            let doc = state.write_doc().await;
            let result = detach(async move {
                let mut doc = doc;
                doc.flush_scheduled = false;
                Self::flush_locked(&mut doc).await
            })
            .await;
            if let Err(err) = result {
                tracing::error!("Failed to save changes: {err:#}");
            }
        });
    }

    /// Troca o token aleatório por um fixo (`--token`/`DBML_TOKEN`).
    #[must_use]
    pub fn with_token(mut self, token: &str) -> Self {
        self.token = token.into();
        self
    }

    /// Troca o limite padrão de conexões WebSocket simultâneas.
    #[must_use]
    pub const fn with_max_ws_connections(mut self, max: usize) -> Self {
        self.max_ws_connections = max;
        self
    }

    /// Liga o modo demo pública: nenhuma rota exige token. O `Host`/`Origin` continua validado.
    #[must_use]
    pub const fn with_public_demo(mut self, public_demo: bool) -> Self {
        self.public_demo = public_demo;
        self
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
        self.document_message_with(kind, doc, None).await
    }

    /// Como [`Self::document_message`], com o motivo de uma recusa (`patch_rejected`).
    async fn document_message_with(
        &self,
        kind: &str,
        doc: &DocState,
        reason: Option<&str>,
    ) -> String {
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
            version: doc.version,
            reason,
        };
        #[expect(
            clippy::expect_used,
            reason = "in-memory DocumentMessage serialization is infallible"
        )]
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
        // Only the target file or a `.dbml` file sitting directly in the workspace folder. A
        // symlink resolves outside the folder, so it fails the parent check.
        let allowed = canonical == self.target_path
            || self.project_dir.as_ref().is_some_and(|dir| {
                canonical.parent() == Some(dir.as_path())
                    && canonical.extension().and_then(|e| e.to_str()) == Some("dbml")
            });
        if !allowed {
            anyhow::bail!(
                "Access denied: '{}' is outside the active workspace",
                canonical.display()
            );
        }
        if !is_file(&canonical).await {
            anyhow::bail!("File not found: {}", canonical.display());
        }
        let doc = self.write_doc().await;
        let this = self.clone();
        detach(async move {
            let mut doc = doc;
            // Edições pendentes do documento atual vão para o disco antes de trocá-lo.
            Self::flush_locked(&mut doc).await?;
            let new_layout_path = layout_path_for(&canonical);
            let new_content = tokio::fs::read_to_string(&canonical).await?;
            let new_layout = tokio::fs::read_to_string(&new_layout_path).await.ok();

            doc.last_saved_hash = compute_hash(&new_content);
            doc.content = new_content;
            doc.layout = new_layout;
            doc.layout_path = new_layout_path;
            doc.file_path.clone_from(&canonical);
            doc.version += 1;

            crate::session::save_async(crate::session::CliSession {
                last_target: Some(this.target_path.clone()),
                active_file: Some(canonical),
            })
            .await;

            let update = this.document_message("update", &doc).await;
            let _ = this.tx.send(update.into());
            Ok(())
        })
        .await
    }

    fn ensure_active(doc: &DocState, expected_file: &str) -> anyhow::Result<()> {
        if doc.file_path.to_string_lossy() == expected_file {
            Ok(())
        } else {
            anyhow::bail!("Active file changed; reload or reconcile changes")
        }
    }

    /// Resposta `git_base_result` ao pedido do modo Comparar: o conteúdo do arquivo ativo no `HEAD`
    /// do git (`source`) ou, em falha, `error` (texto) e `error_code` (ver `GIT_BASE_ERR_*`).
    /// Somente leitura: não toca o documento, a versão nem o disco. Só serve o arquivo ativo
    /// (`expected_file`), nunca um caminho arbitrário.
    ///
    /// # Panics
    ///
    /// Nunca: a serialização de valores JSON em memória não falha.
    pub async fn git_base_message(&self, id: &serde_json::Value, expected_file: &str) -> String {
        let active = {
            let doc = self.doc.read().await;
            Self::ensure_active(&doc, expected_file).map(|()| doc.file_path.clone())
        };
        let outcome: Result<String, (&'static str, String)> = match active {
            Err(err) => Err((GIT_BASE_ERR_OTHER, format!("{err:#}"))),
            Ok(path) => match tokio::task::spawn_blocking(move || read_head_blob(&path)).await {
                Ok(Ok(source)) => Ok(source),
                Ok(Err(err)) => Err((err.code(), err.to_string())),
                Err(err) => Err((GIT_BASE_ERR_OTHER, format!("git task failed: {err}"))),
            },
        };
        let (source, error, error_code) = match outcome {
            Ok(source) => (Some(source), None, None),
            Err((code, error)) => (None, Some(error), Some(code)),
        };
        serde_json::json!({
            "type": WS_MSG_GIT_BASE_RESULT,
            "id": id,
            "source": source,
            "error": error,
            "error_code": error_code,
        })
        .to_string()
    }

    /// Recusa quando `base_version` não é a versão atual do documento; o erro leva o estado atual.
    async fn ensure_version(&self, doc: &DocState, base_version: u64) -> anyhow::Result<()> {
        if doc.version == base_version {
            return Ok(());
        }
        Err(StaleVersion {
            base_version,
            current_version: doc.version,
            rejection: self
                .document_message_with("patch_rejected", doc, Some("stale base_version"))
                .await,
        }
        .into())
    }

    /// Lê o arquivo ativo, valida e aplica o patch e persiste o resultado tudo sob um único
    /// lock de escrita, de modo que dois clientes não percam a edição um do outro.
    ///
    /// # Errors
    ///
    /// Retorna [`StaleVersion`] se `base_version` não é a versão atual (nada é alterado); também
    /// retorna erro se o arquivo ativo mudou, se o documento diverge de `expected_source`, se o
    /// intervalo é inválido ou se a gravação falha. Em caso de sucesso devolve a nova versão.
    pub async fn apply_patch(
        &self,
        expected_file: &str,
        base_version: u64,
        expected_source: &str,
        start: usize,
        end: usize,
        new_text: &str,
    ) -> anyhow::Result<u64> {
        let doc = self.write_doc().await;
        let (expected_file, expected_source, new_text) = (
            expected_file.to_owned(),
            expected_source.to_owned(),
            new_text.to_owned(),
        );
        let this = self.clone();
        detach(async move {
            let mut doc = doc;
            Self::ensure_active(&doc, &expected_file)?;
            this.ensure_version(&doc, base_version).await?;
            // Com edições pendentes a memória é a fonte da verdade; sem elas, o disco (detecta
            // alterações externas mesmo com o watcher desligado).
            if doc.dirty {
                patch_in_place(&mut doc.content, &expected_source, start, end, &new_text)?;
            } else {
                let mut buffer = tokio::fs::read_to_string(&doc.file_path).await?;
                patch_in_place(&mut buffer, &expected_source, start, end, &new_text)?;
                doc.content = buffer;
            }
            doc.dirty = true;
            doc.version += 1;
            if !doc.flush_scheduled {
                doc.flush_scheduled = true;
                this.schedule_flush();
            }
            Ok(doc.version)
        })
        .await
    }

    /// Grava as edições pendentes e confere que o arquivo em disco é `expected_source` (o botão
    /// "salvar" do front).
    ///
    /// # Errors
    ///
    /// Retorna [`StaleVersion`] se `base_version` não é a atual; erro se o arquivo ativo mudou ou o
    /// conteúdo em disco diverge. Em caso de sucesso devolve a versão do documento.
    pub async fn verify_document(
        &self,
        expected_file: &str,
        base_version: u64,
        expected_source: &str,
    ) -> anyhow::Result<u64> {
        let doc = self.write_doc().await;
        let (expected_file, expected_source) =
            (expected_file.to_owned(), expected_source.to_owned());
        let this = self.clone();
        detach(async move {
            let mut doc = doc;
            Self::ensure_active(&doc, &expected_file)?;
            this.ensure_version(&doc, base_version).await?;
            // Salvar = gravar as edições pendentes agora e conferir o que ficou no disco.
            Self::flush_locked(&mut doc).await?;
            if tokio::fs::read_to_string(&doc.file_path).await? != expected_source {
                anyhow::bail!("Document changed outside this editor; reload or reconcile changes");
            }
            Ok(doc.version)
        })
        .await
    }

    /// Atomically persists `new_content` to the active `.dbml` file and updates in-memory state.
    ///
    /// # Errors
    ///
    /// Returns an error if the atomic write fails.
    pub async fn save_content(&self, new_content: String) -> anyhow::Result<()> {
        let doc = self.write_doc().await;
        detach(async move {
            let mut doc = doc;
            Self::persist(&mut doc, new_content).await
        })
        .await
    }

    async fn write_doc(&self) -> OwnedRwLockWriteGuard<DocState> {
        Arc::clone(&self.doc).write_owned().await
    }

    async fn persist(doc: &mut DocState, new_content: String) -> anyhow::Result<()> {
        atomic_write(&doc.file_path, &new_content).await?;
        doc.last_saved_hash = compute_hash(&new_content);
        doc.content = new_content;
        doc.dirty = false;
        doc.version += 1;
        Ok(())
    }

    /// Atomically persists `layout_json` to the `.layout.json` sidecar file.
    ///
    /// # Errors
    ///
    /// Returns an error if the atomic write fails.
    pub async fn save_layout(&self, layout_json: String) -> anyhow::Result<()> {
        let doc = self.write_doc().await;
        detach(async move {
            let mut doc = doc;
            Self::persist_layout(&mut doc, layout_json).await
        })
        .await
    }

    async fn persist_layout(doc: &mut DocState, layout_json: String) -> anyhow::Result<()> {
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
        let doc = self.write_doc().await;
        let expected_file = expected_file.to_owned();
        detach(async move {
            let mut doc = doc;
            Self::ensure_active(&doc, &expected_file)?;
            Self::persist_layout(&mut doc, layout_json).await
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::patch_in_place;

    #[test]
    fn rejects_stale_and_invalid_patches() {
        let mut s = String::from("new");
        assert!(patch_in_place(&mut s, "old", 0, 3, "x").is_err());
        assert_eq!(s, "new");
        let mut s = String::from("é");
        assert!(patch_in_place(&mut s, "é", 1, 2, "x").is_err());
        assert_eq!(s, "é");
        patch_in_place(&mut s, "é", 0, 2, "x").unwrap();
        assert_eq!(s, "x");
    }
}
