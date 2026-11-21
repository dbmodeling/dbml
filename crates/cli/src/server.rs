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
        .on_upgrade(move |socket| async move {
            // A conexão roda numa task rastreada: o encerramento do servidor a espera terminar.
            let lifecycle = std::sync::Arc::clone(&state.lifecycle);
            lifecycle.spawn(handle_socket(socket, state));
        })
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut shutdown = state.lifecycle.subscribe();

    // Subscribe before reading the initial snapshot so file changes are not missed.
    let mut rx = state.tx.subscribe();
    let init_msg = state.init_message().await;
    if sender.send(Message::Text(init_msg.into())).await.is_err() {
        return;
    }

    let (reply_tx, mut reply_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let send_loop = async {
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
    };

    // Recebe mensagens enviadas pelo navegador (patches e layout). As gravações rodam em tasks
    // próprias (ver `AppState::apply_patch`), então abandonar este loop não as interrompe.
    let recv_loop = async {
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
            let result = handle_client_message(&state, client_msg).await;
            let _ = reply_tx.send(
                serde_json::json!({
                    "type": "result", "id": id, "error": result.err().map(|e| format!("{e:#}"))
                })
                .to_string(),
            );
        }
    };

    // O que terminar primeiro (cliente saiu, envio falhou ou servidor encerrando) derruba o resto.
    let closing_gracefully = tokio::select! {
        () = send_loop => false,
        () = recv_loop => false,
        _ = shutdown.changed() => true,
    };
    if closing_gracefully {
        let _ = sender.send(Message::Close(None)).await;
    }
}

async fn handle_client_message(state: &AppState, msg: ClientMessage) -> anyhow::Result<()> {
    match msg {
        ClientMessage::ApplyPatch {
            byte_start,
            byte_end,
            new_text,
            expected_file,
            expected_source,
        } => {
            state
                .apply_patch(
                    &expected_file,
                    &expected_source,
                    byte_start,
                    byte_end,
                    &new_text,
                )
                .await
        }
        ClientMessage::SaveLayout {
            layout,
            expected_file,
        } => {
            let layout = serde_json::to_string_pretty(&layout)?;
            state.save_layout_for(&expected_file, layout).await
        }
        ClientMessage::SaveDocument {
            expected_source,
            expected_file,
        } => {
            state
                .verify_document(&expected_file, &expected_source)
                .await
        }
        ClientMessage::OpenFile(path) => state.switch_file(std::path::PathBuf::from(path)).await,
    }
}

#[cfg(test)]
mod tests {
    use super::{request_token, token_matches};
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
}
