use crate::state::{AppState, compute_hash, is_atomic_write_leftover};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::time::Duration;

/// Se o evento pode ter alterado o arquivo ativo: algum caminho é o próprio arquivo (temporários
/// de gravação atômica e outros arquivos da pasta não contam). Evento sem caminhos (reescaneamento)
/// não pode ser descartado.
pub(crate) fn is_relevant(event: &notify::Event, active: &std::path::Path) -> bool {
    event.paths.is_empty()
        || event.paths.iter().any(|p| {
            p == active
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(is_atomic_write_leftover)
        })
}

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
            let event = match event {
                Ok(event) => event,
                Err(error) => {
                    eprintln!("Watcher error: {error}");
                    continue;
                }
            };
            let active = state.doc.read().await.file_path.clone();
            if !is_relevant(&event, &active) {
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
            doc.version += 1;
            let message = serde_json::json!({
                "type": "external_change",
                "source": source,
                "documentId": path.to_string_lossy(),
                "version": doc.version,
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
    fn only_events_for_the_active_file_are_relevant() {
        use super::is_relevant;
        use std::path::{Path, PathBuf};
        let active = Path::new("/w/schema.dbml");
        let ev = |paths: &[&str]| notify::Event {
            kind: notify::EventKind::Any,
            paths: paths.iter().map(PathBuf::from).collect(),
            attrs: notify::event::EventAttributes::new(),
        };
        assert!(is_relevant(&ev(&["/w/schema.dbml"]), active));
        // rename do temporário sobre o arquivo ativo (a própria gravação atômica) também passa;
        // o hash descarta depois, mas o temporário sozinho não.
        assert!(is_relevant(
            &ev(&["/w/.schema.dbml.tmp-1-2", "/w/schema.dbml"]),
            active
        ));
        assert!(!is_relevant(&ev(&["/w/.schema.dbml.tmp-1-2"]), active));
        assert!(!is_relevant(&ev(&["/w/other.dbml"]), active));
        assert!(!is_relevant(&ev(&["/w/schema.dbml.layout.json"]), active));
        // Sem caminhos (ex.: reescaneamento) não dá para descartar.
        assert!(is_relevant(&ev(&[]), active));
    }

    #[test]
    fn watcher_does_not_block_runtime_shutdown() {
        let (done, result) = mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let dir = std::env::temp_dir();
            let (tx, _) = broadcast::channel(1);
            let state = AppState {
                token: "t".into(),
                public_demo: false,
                max_ws_connections: 1,
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
                    version: crate::state::INITIAL_VERSION,
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
