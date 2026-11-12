use anyhow::Context;
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

#[derive(Clone)]
pub struct AppState {
    pub target_path: PathBuf,
    pub project_dir: Option<PathBuf>,
    pub file_path: Arc<RwLock<PathBuf>>,
    pub layout_path: Arc<RwLock<PathBuf>>,
    pub content: Arc<RwLock<String>>,
    pub layout: Arc<RwLock<Option<String>>>,
    pub last_saved_hash: Arc<RwLock<u64>>,
    pub tx: broadcast::Sender<String>,
}

fn scan_dbml_dir(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("dbml") {
            out.push(p);
        }
    }
    out.sort();
    out
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
            let files = scan_dbml_dir(&target_abs);
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

        let layout_path = PathBuf::from(format!("{}.layout.json", file_path.display()));

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
            target_path: target_abs,
            project_dir,
            file_path: Arc::new(RwLock::new(file_path)),
            layout_path: Arc::new(RwLock::new(layout_path)),
            content: Arc::new(RwLock::new(content)),
            layout: Arc::new(RwLock::new(layout)),
            last_saved_hash: Arc::new(RwLock::new(hash)),
            tx,
        })
    }

    pub async fn active_title(&self) -> String {
        let fp = self.file_path.read().await;
        fp.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("schema.dbml")
            .to_string()
    }

    pub async fn workspace_files(&self) -> Vec<serde_json::Value> {
        let Some(dir) = &self.project_dir else {
            return Vec::new();
        };
        let active = self.file_path.read().await.clone();
        scan_dbml_dir(dir)
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

    /// Switches the active document to `new_path` within the active workspace.
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
        let new_layout_path = PathBuf::from(format!("{}.layout.json", canonical.display()));
        let new_content = tokio::fs::read_to_string(&canonical).await?;
        let new_hash = compute_hash(&new_content);
        let new_layout = if new_layout_path.exists() {
            tokio::fs::read_to_string(&new_layout_path).await.ok()
        } else {
            None
        };

        *self.file_path.write().await = canonical.clone();
        *self.layout_path.write().await = new_layout_path;
        *self.last_saved_hash.write().await = new_hash;
        *self.content.write().await = new_content;
        *self.layout.write().await = new_layout;

        crate::session::save(&crate::session::CliSession {
            last_target: Some(self.target_path.clone()),
            active_file: Some(canonical),
        });

        Ok(())
    }

    /// Atomically persists `new_content` to the active `.dbml` file and updates in-memory state.
    ///
    /// # Errors
    ///
    /// Returns an error if the atomic write fails.
    pub async fn save_content(&self, new_content: String) -> anyhow::Result<()> {
        let new_hash = compute_hash(&new_content);

        // Atualiza a hash em memória antes de gravar para que o watcher ignore o próprio eco
        {
            let mut hash_lock = self.last_saved_hash.write().await;
            *hash_lock = new_hash;
        }

        let fp = self.file_path.read().await.clone();
        atomic_write(&fp, &new_content).await?;

        {
            let mut content_lock = self.content.write().await;
            *content_lock = new_content;
        }

        Ok(())
    }

    /// Atomically persists `layout_json` to the `.layout.json` sidecar file.
    ///
    /// # Errors
    ///
    /// Returns an error if the atomic write fails.
    pub async fn save_layout(&self, layout_json: String) -> anyhow::Result<()> {
        let lp = self.layout_path.read().await.clone();
        atomic_write(&lp, &layout_json).await?;
        let mut layout_lock = self.layout.write().await;
        *layout_lock = Some(layout_json);
        Ok(())
    }
}
