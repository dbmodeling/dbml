//! Testes de ponta a ponta do servidor Axum/WebSocket (sobem um listener real em porta efêmera).

use dbml_cli::server::create_router;
use dbml_cli::state::AppState;
use futures_util::StreamExt;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

struct Harness {
    addr: SocketAddr,
    state: AppState,
    dir: PathBuf,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn spawn(name: &str) -> Harness {
    let dir = std::env::temp_dir().join(format!("dbml_srv_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("schema.dbml");
    std::fs::write(&file, "Table a { id int [pk] }\n").unwrap();
    let state = AppState::new(file).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = create_router(state.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Harness { addr, state, dir }
}

/// Requisição HTTP/1.1 crua; devolve a linha de status e o corpo.
async fn http(addr: SocketAddr, request: &str) -> (u16, String) {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8_lossy(&buf).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
    (status, body)
}

fn post_introspect(addr: SocketAddr, extra_header: &str) -> String {
    let body = r#"{"dialect":"db2"}"#;
    format!(
        "POST /api/introspect/test HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_header}\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn ws_requires_session_token() {
    let h = spawn("ws_token").await;
    let url = format!("ws://{}/ws", h.addr);
    let err = tokio_tungstenite::connect_async(&url).await.unwrap_err();
    assert!(
        matches!(&err, tokio_tungstenite::tungstenite::Error::Http(r) if r.status() == 401),
        "{err:?}"
    );
    let bad = format!("ws://{}/ws?token=errado", h.addr);
    assert!(tokio_tungstenite::connect_async(&bad).await.is_err());

    let good = format!("ws://{}/ws?token={}", h.addr, h.state.token());
    let (mut ws, _) = tokio_tungstenite::connect_async(&good).await.unwrap();
    let first = ws.next().await.unwrap().unwrap();
    let Message::Text(text) = first else {
        panic!("esperava texto, veio {first:?}")
    };
    assert!(text.contains("\"type\":\"init\""), "{text}");
    ws.close(None).await.ok();
}

#[tokio::test]
async fn api_requires_session_token_but_static_assets_do_not() {
    let h = spawn("api_token").await;
    let (status, _) = http(h.addr, &post_introspect(h.addr, "")).await;
    assert_eq!(status, 401);

    let header = format!("X-DBML-Token: {}\r\n", h.state.token());
    let (status, body) = http(h.addr, &post_introspect(h.addr, &header)).await;
    assert_eq!(status, 400, "{body}"); // passou na autenticação, falhou no dialeto

    let header = format!("Authorization: Bearer {}\r\n", h.state.token());
    let (status, _) = http(h.addr, &post_introspect(h.addr, &header)).await;
    assert_eq!(status, 400);

    let get = format!(
        "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        h.addr
    );
    let (status, body) = http(h.addr, &get).await;
    assert_eq!(status, 200);
    assert!(
        !body.contains(h.state.token()),
        "o token não pode vazar no HTML"
    );
}

#[test]
fn tokens_are_random_per_run() {
    let a = dbml_cli::state::generate_token();
    let b = dbml_cli::state::generate_token();
    assert_eq!(a.len(), 32);
    assert_ne!(a, b);
    assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn warns_only_for_non_loopback_hosts() {
    use dbml_cli::server::non_loopback_warning;
    assert!(non_loopback_warning("127.0.0.1".parse().unwrap()).is_none());
    assert!(non_loopback_warning("::1".parse().unwrap()).is_none());
    let w = non_loopback_warning("0.0.0.0".parse().unwrap()).unwrap();
    assert!(w.contains("0.0.0.0") && w.contains("token"), "{w}");
}

#[tokio::test]
async fn oversized_websocket_message_closes_the_connection() {
    use futures_util::SinkExt;
    let h = spawn("ws_big").await;
    let url = format!("ws://{}/ws?token={}", h.addr, h.state.token());
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws.next().await.unwrap().unwrap(); // init
    let big = "x".repeat(dbml_cli::server::MAX_WS_PAYLOAD_BYTES + 1024);
    let _ = ws.send(Message::Text(big.into())).await;
    let end = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match ws.next().await {
                None | Some(Err(_) | Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            }
        }
    })
    .await;
    assert!(end.is_ok(), "o servidor devia encerrar a conexão");
}

#[tokio::test]
async fn oversized_api_body_is_rejected_with_413() {
    let h = spawn("api_big").await;
    let body = "x".repeat(dbml_cli::server::MAX_API_BODY_BYTES + 1024);
    let request = format!(
        "POST /api/introspect/test HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nX-DBML-Token: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        h.addr,
        h.state.token(),
        body.len()
    );
    let (status, _) = http(h.addr, &request).await;
    assert_eq!(status, 413);
}
