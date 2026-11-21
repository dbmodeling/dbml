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

    let lifecycle = std::sync::Arc::clone(&state.lifecycle);
    let mut shutdown = lifecycle.subscribe();
    lifecycle.spawn(async move {
        let _watcher = watcher;
        loop {
            let event = tokio::select! {
                event = rx.recv() => event,
                _ = shutdown.changed() => break,
            };
            let Some(event) = event else { break };
            if let Err(error) = event {
                eprintln!("Watcher error: {error}");
                continue;
            }
            tokio::time::sleep(Duration::from_millis(60)).await;
            while rx.try_recv().is_ok() {}

            let mut doc = state.doc.write().await;
            let path = doc.file_path.clone();
            let Ok(source) = tokio::fs::read_to_string(&path).await else {
                continue;
            };
            let hash = compute_hash(&source);
            if hash == doc.last_saved_hash {
                continue;
            }
            // Alteração externa: vence sobre edições ainda não gravadas (o cliente reconcilia).
            doc.dirty = false;
            doc.last_saved_hash = hash;
            doc.content.clone_from(&source);
            let message = serde_json::json!({
                "type": "external_change",
                "source": source,
                "documentId": path.to_string_lossy(),
            });
            let _ = state.tx.send(message.to_string().into());
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::start_file_watcher;
    use crate::state::{AppState, DocState};
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    use tokio::sync::{RwLock, broadcast};

    #[test]
    fn watcher_does_not_block_runtime_shutdown() {
        let (done, result) = mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let dir = std::env::temp_dir();
            let (tx, _) = broadcast::channel(1);
            let state = AppState {
                token: "t".into(),
                target_path: dir.clone(),
                project_dir: Some(dir.clone()),
                doc: Arc::new(RwLock::new(DocState {
                    file_path: dir.clone(),
                    layout_path: dir,
                    content: String::new(),
                    layout: None,
                    last_saved_hash: 0,
                    dirty: false,
                    flush_scheduled: false,
                })),
                tx,
                lifecycle: Arc::default(),
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
