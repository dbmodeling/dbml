use crate::assets::static_handler;
use crate::state::AppState;
use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
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
    },
    #[serde(rename = "save_source")]
    SaveSource(String),
    #[serde(rename = "save_layout")]
    SaveLayout(serde_json::Value),
    #[serde(rename = "open_file")]
    OpenFile(String),
}

const MAX_WS_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .fallback(static_handler)
        .with_state(state)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // 1. Envia payload inicial ao conectar
    {
        let content = state.content.read().await.clone();
        let layout: Option<serde_json::Value> = state
            .layout
            .read()
            .await
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok());

        let title = state.active_title().await;
        let files = state.workspace_files().await;

        let init_msg = serde_json::json!({
            "type": "init",
            "source": content,
            "layout": layout,
            "title": title,
            "files": files
        })
        .to_string();

        if sender.send(Message::Text(init_msg.into())).await.is_err() {
            return;
        }
    }

    // 2. Canal de broadcast para enviar atualizações externas a este cliente
    let mut rx = state.tx.subscribe();
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
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
                continue;
            }
            let client_msg = match serde_json::from_str::<ClientMessage>(&text) {
                Ok(msg) => msg,
                Err(err) => {
                    eprintln!("Ignoring invalid WebSocket message: {err}");
                    continue;
                }
            };
            match client_msg {
                ClientMessage::ApplyPatch {
                    byte_start,
                    byte_end,
                    new_text,
                } => {
                    let current = state_clone.content.read().await.clone();
                    if byte_start <= byte_end
                        && byte_end <= current.len()
                        && current.is_char_boundary(byte_start)
                        && current.is_char_boundary(byte_end)
                    {
                        let mut updated = current;
                        updated.replace_range(byte_start..byte_end, &new_text);
                        if let Err(err) = state_clone.save_content(updated).await {
                            eprintln!("Failed to save patched content: {err}");
                        }
                    }
                }
                ClientMessage::SaveSource(new_source) => {
                    if let Err(err) = state_clone.save_content(new_source).await {
                        eprintln!("Failed to save source: {err}");
                    }
                }
                ClientMessage::SaveLayout(layout_val) => {
                    if let Ok(layout_str) = serde_json::to_string_pretty(&layout_val)
                        && let Err(err) = state_clone.save_layout(layout_str).await
                    {
                        eprintln!("Failed to save layout: {err}");
                    }
                }
                ClientMessage::OpenFile(path) => {
                    if let Err(err) = state_clone
                        .switch_file(std::path::PathBuf::from(path))
                        .await
                    {
                        eprintln!("Rejected open_file request: {err}");
                        continue;
                    }
                    let content = state_clone.content.read().await.clone();
                    let layout: Option<serde_json::Value> = state_clone
                        .layout
                        .read()
                        .await
                        .as_ref()
                        .and_then(|s| serde_json::from_str(s).ok());
                    let title = state_clone.active_title().await;
                    let files = state_clone.workspace_files().await;
                    let update_msg = serde_json::json!({
                        "type": "update",
                        "source": content,
                        "layout": layout,
                        "title": title,
                        "files": files
                    })
                    .to_string();
                    let _ = state_clone.tx.send(update_msg);
                }
            }
        }
    });

    // Se qualquer uma das tarefas encerrar, aborta a outra
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}
