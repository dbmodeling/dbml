//! Teste fim a fim (#83): sobe o servidor HTTP + WebSocket real da CLI, carrega o SPA (`/`),
//! aplica uma edição via WebSocket (`apply_patch`), confere o arquivo gravado em disco e encerra
//! o servidor verificando o fechamento limpo do socket.

use dbml_cli::{server, state::AppState};
use futures_util::{SinkExt, StreamExt};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    connect_async, tungstenite::Message, tungstenite::client::IntoClientRequest,
};

const E2E_DIR_PREFIX: &str = "dbml_e2e_web_";
const E2E_FILE_NAME: &str = "schema.dbml";
const INITIAL_DBML: &str = "Table users {\n  id int [pk]\n}\n";
const UPDATED_DBML: &str = "Table accounts {\n  id int [pk]\n}\n";
const PATCH_START_BYTE: usize = 6;
const PATCH_END_BYTE: usize = 11;
const PATCH_NEW_TEXT: &str = "accounts";
const LOOPBACK_HOST: &str = "127.0.0.1";
const PATCH_REQ_ID: u64 = 1;
const SAVE_REQ_ID: u64 = 2;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn e2e_server_http_websocket_edit_persist_and_shutdown() {
    let dir = std::env::temp_dir().join(format!("{E2E_DIR_PREFIX}{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let file = dir.join(E2E_FILE_NAME);
    tokio::fs::write(&file, INITIAL_DBML).await.unwrap();

    let state = AppState::new(file.clone()).await.unwrap();
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let app = server::create_router(
        state.clone(),
        server::allowed_hosts(LOOPBACK_HOST, addr.port()),
    );

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let srv = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = stop_rx.await;
            })
            .await
            .unwrap();
    });

    // 1) HTTP GET / entrega o SPA com CSP e cabeçalhos de segurança.
    let mut http = TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "GET / HTTP/1.1\r\nHost: {LOOPBACK_HOST}:{}\r\nConnection: close\r\n\r\n",
        addr.port()
    );
    http.write_all(req.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(IO_TIMEOUT, http.read_to_end(&mut raw))
        .await
        .unwrap()
        .unwrap();
    let response = String::from_utf8_lossy(&raw);
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        response
            .to_ascii_lowercase()
            .contains("content-security-policy:"),
        "{response}"
    );
    assert!(
        response
            .to_ascii_lowercase()
            .contains("x-content-type-options: nosniff"),
        "{response}"
    );

    // 2) WebSocket conecta com o token da sessão e recebe `init`.
    let ws_url = format!(
        "ws://{LOOPBACK_HOST}:{}/ws?token={}",
        addr.port(),
        state.token()
    );
    let mut ws_req = ws_url.into_client_request().unwrap();
    ws_req.headers_mut().insert(
        "Origin",
        format!("http://{LOOPBACK_HOST}:{}", addr.port())
            .parse()
            .unwrap(),
    );
    let (mut ws, _) = tokio::time::timeout(IO_TIMEOUT, connect_async(ws_req))
        .await
        .unwrap()
        .unwrap();

    let init_frame = tokio::time::timeout(IO_TIMEOUT, ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let init: serde_json::Value = serde_json::from_str(init_frame.to_text().unwrap()).unwrap();
    assert_eq!(init["type"], "init");
    assert_eq!(init["source"], INITIAL_DBML);
    let version = init["version"].as_u64().unwrap();
    let doc_id = init["documentId"].as_str().unwrap().to_owned();

    // 3) Envia `apply_patch` e `save_document` e confere a gravação em disco.
    let patch_msg = serde_json::json!({
        "type": "apply_patch",
        "id": PATCH_REQ_ID,
        "payload": {
            "base_version": version,
            "byte_start": PATCH_START_BYTE,
            "byte_end": PATCH_END_BYTE,
            "new_text": PATCH_NEW_TEXT,
            "expected_source": INITIAL_DBML,
            "expected_file": doc_id,
        }
    });
    ws.send(Message::Text(patch_msg.to_string().into()))
        .await
        .unwrap();
    let patch_res: serde_json::Value = serde_json::from_str(
        tokio::time::timeout(IO_TIMEOUT, ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .to_text()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(patch_res["type"], "result");
    assert_eq!(patch_res["id"], PATCH_REQ_ID);
    assert!(patch_res["error"].is_null(), "{patch_res}");
    let next_version = patch_res["version"].as_u64().unwrap();

    let save_msg = serde_json::json!({
        "type": "save_document",
        "id": SAVE_REQ_ID,
        "payload": {
            "base_version": next_version,
            "expected_source": UPDATED_DBML,
            "expected_file": doc_id,
        }
    });
    ws.send(Message::Text(save_msg.to_string().into()))
        .await
        .unwrap();
    let save_res: serde_json::Value = serde_json::from_str(
        tokio::time::timeout(IO_TIMEOUT, ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .to_text()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(save_res["type"], "result");
    assert_eq!(save_res["id"], SAVE_REQ_ID);
    assert!(save_res["error"].is_null(), "{save_res}");
    assert_eq!(
        tokio::fs::read_to_string(&file).await.unwrap(),
        UPDATED_DBML
    );

    // 4) Encerra o servidor e confirma que o WebSocket desconecta sem erro.
    state.lifecycle.begin_shutdown();
    let _ = stop_tx.send(());
    state.shutdown(SHUTDOWN_GRACE).await;
    let _ = srv.await;
    let closed = tokio::time::timeout(IO_TIMEOUT, ws.next()).await.unwrap();
    assert!(
        closed.is_none() || matches!(closed, Some(Ok(Message::Close(_)) | Err(_))),
        "expected socket close after server shutdown, got {closed:?}"
    );

    let _ = tokio::fs::remove_dir_all(&dir).await;
}
