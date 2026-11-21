use crate::assets::static_handler;
use crate::state::AppState;
use axum::{
    Router,
    body::Body,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(tag = "type", content = "payload")]
enum ClientMessage {
    #[serde(rename = "apply_patch")]
    ApplyPatch {
        byte_start: usize,
        byte_end: usize,
        new_text: String,
        expected_source: String,
        expected_file: String,
    },
    #[serde(rename = "save_layout")]
    SaveLayout {
        layout: serde_json::Value,
        expected_file: String,
    },
    #[serde(rename = "save_document")]
    SaveDocument {
        expected_source: String,
        expected_file: String,
    },
    #[serde(rename = "open_file")]
    OpenFile(String),
}

fn apply_patch(
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

async fn active_file(state: &AppState, expected_file: &str) -> anyhow::Result<std::path::PathBuf> {
    let path = state.file_path.read().await;
    if path.to_string_lossy() != expected_file {
        anyhow::bail!("Active file changed; reload or reconcile changes");
    }
    Ok(path.clone())
}

const MAX_WS_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;

/// `Host` values (lowercase `name:port`) this server answers to. Anything else is refused, which
/// defeats DNS rebinding: the attacker's name never matches the allowlist.
#[must_use]
pub fn allowed_hosts(bind_host: &str, port: u16) -> Vec<String> {
    let mut hosts: Vec<String> = ["127.0.0.1", "localhost", "[::1]"]
        .iter()
        .map(|h| format!("{h}:{port}"))
        .collect();
    let bind = bind_host.trim().to_ascii_lowercase();
    if !matches!(bind.as_str(), "" | "0.0.0.0" | "::" | "[::]") {
        let bind = if bind.contains(':') && !bind.starts_with('[') {
            format!("[{bind}]")
        } else {
            bind
        };
        let entry = format!("{bind}:{port}");
        if !hosts.contains(&entry) {
            hosts.push(entry);
        }
    }
    hosts
}

/// `Host` must be allowlisted; when `Origin` is present it must name the same allowlisted
/// authority. Requests without `Origin` (curl, native clients) pass the Origin check.
#[must_use]
pub fn request_allowed(headers: &HeaderMap, allowed: &[String]) -> bool {
    let Some(Ok(host)) = headers.get(header::HOST).map(|h| h.to_str()) else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    if !allowed.contains(&host) {
        return false;
    }
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    origin
        .to_str()
        .ok()
        .and_then(|o| {
            o.strip_prefix("http://")
                .or_else(|| o.strip_prefix("https://"))
        })
        .is_some_and(|authority| authority.eq_ignore_ascii_case(&host))
}

async fn guard(
    State(allowed): State<std::sync::Arc<Vec<String>>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if request_allowed(req.headers(), &allowed) {
        next.run(req).await
    } else {
        (StatusCode::FORBIDDEN, "forbidden host or origin").into_response()
    }
}

pub fn create_router(state: AppState, allowed: Vec<String>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/introspect", post(crate::introspect_api::handler))
        .route(
            "/api/introspect/test",
            post(crate::introspect_api::test_handler),
        )
        .fallback(static_handler)
        .layer(middleware::from_fn_with_state(
            std::sync::Arc::new(allowed),
            guard,
        ))
        .with_state(state)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe before reading the initial snapshot so file changes are not missed.
    let mut rx = state.tx.subscribe();
    let init_msg = {
        let _guard = state.write_lock.lock().await;
        let content = state.content.read().await.clone();
        let layout: Option<serde_json::Value> = state
            .layout
            .read()
            .await
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok());

        let title = state.active_title().await;
        let document_id = state.file_path.read().await.to_string_lossy().into_owned();
        let files = state.workspace_files().await;

        serde_json::json!({
            "type": "init",
            "source": content,
            "layout": layout,
            "title": title,
            "documentId": document_id,
            "saved": true,
            "files": files
        })
        .to_string()
    };
    if sender.send(Message::Text(init_msg.into())).await.is_err() {
        return;
    }

    let (reply_tx, mut reply_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let mut send_task = tokio::spawn(async move {
        loop {
            let msg = tokio::select! {
                Ok(msg) = rx.recv() => msg,
                Some(msg) = reply_rx.recv() => msg,
                else => break,
            };
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // 3. Recebe mensagens enviadas pelo navegador (patches e layout)
    let state_clone = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(frame)) = receiver.next().await {
            let text = match frame {
                Message::Text(text) => text,
                Message::Close(_) => break,
                _ => continue,
            };
            if text.len() > MAX_WS_PAYLOAD_BYTES {
                eprintln!(
                    "Ignoring oversized WebSocket payload ({} bytes)",
                    text.len()
                );
                break;
            }
            let envelope: serde_json::Value = match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let id = envelope
                .get("id")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let client_msg = match serde_json::from_value::<ClientMessage>(envelope) {
                Ok(msg) => msg,
                Err(err) => {
                    let _ = reply_tx.send(serde_json::json!({"type": "result", "id": id, "error": format!("Invalid request: {err}")}).to_string());
                    continue;
                }
            };
            let _guard = state_clone.write_lock.lock().await;
            let result: anyhow::Result<()> = match client_msg {
                ClientMessage::ApplyPatch {
                    byte_start,
                    byte_end,
                    new_text,
                    expected_file,
                    expected_source,
                } => {
                    async {
                        let path = active_file(&state_clone, &expected_file).await?;
                        let current = tokio::fs::read_to_string(path).await?;
                        let updated = apply_patch(
                            current,
                            &expected_source,
                            byte_start,
                            byte_end,
                            &new_text,
                        )?;
                        state_clone.save_content(updated).await
                    }
                    .await
                }
                ClientMessage::SaveLayout {
                    layout,
                    expected_file,
                } => {
                    async {
                        active_file(&state_clone, &expected_file).await?;
                        let layout = serde_json::to_string_pretty(&layout)?;
                        state_clone.save_layout(layout).await
                    }
                    .await
                }
                ClientMessage::SaveDocument {
                    expected_source,
                    expected_file,
                } => {
                    async {
                        let path = active_file(&state_clone, &expected_file).await?;
                        if tokio::fs::read_to_string(path).await? != expected_source {
                            anyhow::bail!(
                                "Document changed outside this editor; reload or reconcile changes"
                            );
                        }
                        Ok(())
                    }
                    .await
                }
                ClientMessage::OpenFile(path) => async {
                    state_clone.switch_file(std::path::PathBuf::from(path)).await?;
                    let content = state_clone.content.read().await.clone();
                    let layout: Option<serde_json::Value> = state_clone
                        .layout
                        .read()
                        .await
                        .as_ref()
                        .and_then(|s| serde_json::from_str(s).ok());
                    let document_id = state_clone
                        .file_path
                        .read()
                        .await
                        .to_string_lossy()
                        .into_owned();
                    let update_msg = serde_json::json!({
                        "type": "update", "source": content, "layout": layout,
                        "title": state_clone.active_title().await, "files": state_clone.workspace_files().await,
                        "documentId": document_id, "saved": true
                    }).to_string();
                    let _ = state_clone.tx.send(update_msg);
                    Ok(())
                }.await,
            };
            let _ = reply_tx.send(
                serde_json::json!({
                    "type": "result", "id": id, "error": result.err().map(|e| format!("{e:#}"))
                })
                .to_string(),
            );
        }
    });

    // Se qualquer uma das tarefas encerrar, aborta a outra
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}

#[cfg(test)]
mod tests {
    use super::apply_patch;

    #[test]
    fn rejects_stale_and_invalid_patches() {
        assert!(apply_patch("new".into(), "old", 0, 3, "x").is_err());
        assert!(apply_patch("é".into(), "é", 1, 2, "x").is_err());
        assert_eq!(apply_patch("é".into(), "é", 0, 2, "x").unwrap(), "x");
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    use axum::http::HeaderValue;

    fn hdr(origin: Option<&str>, host: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        if let Some(o) = origin {
            h.insert(header::ORIGIN, HeaderValue::from_str(o).unwrap());
        }
        h
    }

    #[test]
    fn foreign_origin_is_refused() {
        let a = allowed_hosts("127.0.0.1", 3000);
        assert!(request_allowed(&hdr(None, "127.0.0.1:3000"), &a));
        assert!(request_allowed(
            &hdr(Some("http://localhost:3000"), "localhost:3000"),
            &a
        ));
        assert!(!request_allowed(
            &hdr(Some("https://evil.example"), "127.0.0.1:3000"),
            &a
        ));
        assert!(!request_allowed(
            &hdr(Some("http://127.0.0.1:3000"), "localhost:3000"),
            &a
        ));
    }

    #[test]
    fn rebound_host_is_refused_even_with_matching_origin() {
        let a = allowed_hosts("127.0.0.1", 3000);
        assert!(!request_allowed(
            &hdr(Some("http://evil.example:3000"), "evil.example:3000"),
            &a
        ));
        assert!(!request_allowed(&hdr(None, "evil.example:3000"), &a));
    }

    #[test]
    fn custom_bind_host_is_allowed_and_wildcard_is_not() {
        let a = allowed_hosts("192.168.0.5", 3000);
        assert!(request_allowed(&hdr(None, "192.168.0.5:3000"), &a));
        assert!(!allowed_hosts("0.0.0.0", 3000).contains(&"0.0.0.0:3000".to_string()));
    }
}
