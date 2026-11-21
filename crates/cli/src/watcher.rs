use crate::state::{AppState, compute_hash};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::time::Duration;

/// Starts a background filesystem watcher for external edits to the active `.dbml` file.
///
/// # Errors
///
/// Returns an error if initializing the OS file watcher or watching the directory fails.
pub fn start_file_watcher(state: AppState) -> anyhow::Result<()> {
    let watch_dir = state
        .project_dir
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher = RecommendedWatcher::new(
        move |event| {
            let _ = tx.send(event);
        },
        Config::default(),
    )?;
    watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;

    tokio::spawn(async move {
        let _watcher = watcher;
        while let Some(event) = rx.recv().await {
            if let Err(error) = event {
                eprintln!("Watcher error: {error}");
                continue;
            }
            tokio::time::sleep(Duration::from_millis(60)).await;
            while rx.try_recv().is_ok() {}

            let _guard = state.write_lock.lock().await;
            let path = state.file_path.read().await.clone();
            let Ok(source) = tokio::fs::read_to_string(&path).await else {
                continue;
            };
            let hash = compute_hash(&source);
            let mut saved_hash = state.last_saved_hash.write().await;
            if hash == *saved_hash {
                continue;
            }
            *saved_hash = hash;
            state.content.write().await.clone_from(&source);
            let message = serde_json::json!({
                "type": "external_change",
                "source": source,
                "documentId": path.to_string_lossy(),
            });
            let _ = state.tx.send(message.to_string());
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::start_file_watcher;
    use crate::state::AppState;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    use tokio::sync::{Mutex, RwLock, broadcast};

    #[test]
    fn watcher_does_not_block_runtime_shutdown() {
        let (done, result) = mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let dir = std::env::temp_dir();
            let (tx, _) = broadcast::channel(1);
            let state = AppState {
                target_path: dir.clone(),
                project_dir: Some(dir.clone()),
                file_path: Arc::new(RwLock::new(dir.clone())),
                layout_path: Arc::new(RwLock::new(dir)),
                content: Arc::new(RwLock::new(String::new())),
                layout: Arc::new(RwLock::new(None)),
                last_saved_hash: Arc::new(RwLock::new(0)),
                tx,
                write_lock: Arc::new(Mutex::new(())),
            };
            runtime.block_on(async { start_file_watcher(state).unwrap() });
            drop(runtime);
            done.send(()).unwrap();
        });
        result
            .recv_timeout(Duration::from_secs(3))
            .expect("watcher blocked runtime shutdown");
    }
}
