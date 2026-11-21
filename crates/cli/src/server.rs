use crate::assets::static_handler;
use crate::state::AppState;
use axum::{
    Router,
    extract::{
        DefaultBodyLimit, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
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

/// Tamanho máximo de uma mensagem/frame WebSocket (aplicado pelo protocolo, antes da alocação).
pub const MAX_WS_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;

/// Aviso exibido no terminal quando o servidor escuta fora do loopback: qualquer máquina da rede
/// que conheça o token de sessão consegue editar os arquivos do workspace.
#[must_use]
pub fn non_loopback_warning(ip: std::net::IpAddr) -> Option<String> {
    if ip.is_loopback() {
        return None;
    }
    Some(format!(
        "O servidor escuta em {ip}, acessível pela rede. Quem tiver a URL com o token de sessão \
         pode ler e editar os arquivos do workspace. Use apenas em redes confiáveis."
    ))
}

/// Compara em tempo constante (para o comprimento do menor) para não vazar o token por timing.
fn token_matches(expected: &str, given: &str) -> bool {
    let (a, b) = (expected.as_bytes(), given.as_bytes());
    let mut diff = a.len() ^ b.len();
    for (x, y) in a.iter().zip(b) {
        diff |= usize::from(x ^ y);
    }
    diff == 0
}

/// Extrai o token de `X-DBML-Token`, `Authorization: Bearer` ou `?token=` (o WebSocket do
/// navegador não permite cabeçalhos próprios, então usa a query string).
fn request_token(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    if let Some(v) = headers.get("x-dbml-token").and_then(|v| v.to_str().ok()) {
        return Some(v.to_owned());
    }
    if let Some(v) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        return Some(v.to_owned());
    }
    query?
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="))
        .map(str::to_owned)
}

async fn require_token(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let given = request_token(req.headers(), req.uri().query());
    if given.is_some_and(|g| token_matches(state.token(), &g)) {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "missing or invalid session token").into_response()
    }
}

/// Corpo máximo das rotas `/api/*` (formulários de conexão; o `DefaultBodyLimit` do Axum é
/// aplicado antes de ler o corpo inteiro).
pub const MAX_API_BODY_BYTES: usize = 1024 * 1024;

pub fn create_router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/ws", get(ws_handler))
        .route(
            "/api/introspect",
            post(crate::introspect_api::handler).layer(DefaultBodyLimit::max(MAX_API_BODY_BYTES)),
        )
        .route(
            "/api/introspect/test",
            post(crate::introspect_api::test_handler)
                .layer(DefaultBodyLimit::max(MAX_API_BODY_BYTES)),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), require_token));
    Router::new()
        .merge(protected)
        .fallback(static_handler)
        .with_state(state)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.max_message_size(MAX_WS_PAYLOAD_BYTES)
        .max_frame_size(MAX_WS_PAYLOAD_BYTES)
        .on_upgrade(move |socket| handle_socket(socket, state))
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
    use super::{apply_patch, request_token, token_matches};
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn token_comparison_and_extraction() {
        assert!(token_matches("abc", "abc"));
        assert!(!token_matches("abc", "abd"));
        assert!(!token_matches("abc", "ab"));
        assert!(!token_matches("abc", ""));
        let mut h = HeaderMap::new();
        assert_eq!(
            request_token(&h, Some("a=1&token=xyz")).as_deref(),
            Some("xyz")
        );
        assert_eq!(request_token(&h, None), None);
        h.insert("authorization", HeaderValue::from_static("Bearer tk"));
        assert_eq!(request_token(&h, None).as_deref(), Some("tk"));
        h.insert("x-dbml-token", HeaderValue::from_static("hh"));
        assert_eq!(request_token(&h, None).as_deref(), Some("hh"));
    }

    #[test]
    fn rejects_stale_and_invalid_patches() {
        assert!(apply_patch("new".into(), "old", 0, 3, "x").is_err());
        assert!(apply_patch("é".into(), "é", 1, 2, "x").is_err());
        assert_eq!(apply_patch("é".into(), "é", 0, 2, "x").unwrap(), "x");
    }
}
