use crate::state::{AppState, compute_hash};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::mpsc::channel;
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

    tokio::task::spawn_blocking(move || {
        let (tx, rx) = channel();

        let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Warning: Could not initialize file watcher: {e}");
                return;
            }
        };

        if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
            eprintln!(
                "Warning: Failed to watch directory '{}': {e}",
                watch_dir.display()
            );
            return;
        }

        // Loop de eventos com debounce simples
        while let Ok(res) = rx.recv() {
            match res {
                Ok(Event { .. }) => {
                    // Debounce para aguardar conclusão de escritas em lote de editores externos
                    std::thread::sleep(Duration::from_millis(60));

                    // Limpa eventos pendentes na fila
                    while rx.try_recv().is_ok() {}

                    let state = state.clone();
                    tokio::spawn(async move {
                        let file_path = state.file_path.read().await.clone();
                        if let Ok(new_content) = std::fs::read_to_string(&file_path) {
                            let current_hash = compute_hash(&new_content);
                            let last_hash = *state.last_saved_hash.read().await;

                            // Se a hash for diferente, é uma alteração externa legítima!
                            if current_hash != last_hash {
                                {
                                    let mut hash_lock = state.last_saved_hash.write().await;
                                    *hash_lock = current_hash;
                                }
                                {
                                    let mut content_lock = state.content.write().await;
                                    content_lock.clone_from(&new_content);
                                }

                                let msg = serde_json::json!({
                                    "type": "external_change",
                                    "source": new_content
                                })
                                .to_string();

                                let _ = state.tx.send(msg);
                                println!(
                                    "  [sync] External change detected and broadcast to browser."
                                );
                            }
                        }
                    });
                }
                Err(e) => {
                    eprintln!("Watcher error: {e}");
                }
            }
        }
    });

    Ok(())
}
