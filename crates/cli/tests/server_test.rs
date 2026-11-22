#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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
    let app = create_router(
        state.clone(),
        dbml_cli::server::allowed_hosts("127.0.0.1", addr.port()),
    );
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
async fn shutdown_closes_sockets_and_waits_for_their_tasks() {
    let h = spawn("shutdown").await;
    let url = format!("ws://{}/ws?token={}", h.addr, h.state.token());
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws.next().await.unwrap().unwrap(); // init
    // O upgrade já passou, mas a task da conexão pode ainda estar subindo.
    while h.state.lifecycle.running() == 0 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    h.state.shutdown(std::time::Duration::from_secs(3)).await;
    assert_eq!(h.state.lifecycle.running(), 0);
    let end = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            match ws.next().await {
                None | Some(Err(_) | Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            }
        }
    })
    .await;
    assert!(end.is_ok(), "o cliente devia ver o fechamento");
}

#[tokio::test]
async fn lagging_client_gets_the_current_state_and_keeps_receiving() {
    let h = spawn("lagged").await;
    let url = format!("ws://{}/ws?token={}", h.addr, h.state.token());
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws.next().await.unwrap().unwrap(); // init
    while h.state.lifecycle.running() == 0 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    // Sem ceder o runtime, estoura a capacidade do canal: o receptor do socket fica atrasado.
    for i in 0..(dbml_cli::state::BROADCAST_CAPACITY * 4) {
        let _ = h
            .state
            .tx
            .send(format!(r#"{{"type":"noise","n":{i}}}"#).into());
    }
    let _ = h.state.tx.send(r#"{"type":"marker"}"#.into());

    let mut saw_update = false;
    let seen_marker = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let Some(Ok(Message::Text(text))) = ws.next().await else {
                return false;
            };
            if text.contains(r#""type":"update""#) {
                saw_update = true;
                assert!(
                    text.contains("Table a"),
                    "o update deve trazer o documento: {text}"
                );
            }
            if text.contains(r#""type":"marker""#) {
                return true;
            }
        }
    })
    .await;
    assert_eq!(seen_marker, Ok(true), "o cliente parou de receber");
    assert!(
        saw_update,
        "ao atrasar, o cliente deve receber o estado atual"
    );

    // Depois do atraso continua recebendo normalmente.
    let _ = h.state.tx.send(r#"{"type":"after"}"#.into());
    let next = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
        .await
        .unwrap();
    let Some(Ok(Message::Text(text))) = next else {
        panic!("esperava texto, veio {next:?}")
    };
    assert!(text.contains("after"), "{text}");
}

#[tokio::test]
async fn websocket_connections_are_capped_with_a_clear_refusal() {
    let h = spawn("cap").await;
    let url = format!("ws://{}/ws?token={}", h.addr, h.state.token());
    let mut open = Vec::new();
    for _ in 0..dbml_cli::server::MAX_WS_CONNECTIONS {
        let (ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        open.push(ws);
    }
    let err = tokio_tungstenite::connect_async(&url).await.unwrap_err();
    assert!(
        matches!(&err, tokio_tungstenite::tungstenite::Error::Http(r) if r.status() == 503),
        "{err:?}"
    );

    // Fechar uma conexão libera a vaga.
    let mut first = open.remove(0);
    first.close(None).await.ok();
    drop(first);
    let reconnected = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if tokio_tungstenite::connect_async(&url).await.is_ok() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reconnected.is_ok(), "a vaga não foi liberada");
}

#[tokio::test]
async fn shutdown_waits_for_pending_writes() {
    let h = spawn("shutdown_write").await;
    let writer = {
        let state = h.state.clone();
        tokio::spawn(async move {
            // Segura o lock do documento como uma gravação em andamento faria.
            let mut doc = state.doc.write().await;
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            doc.content = "// gravado\n".into();
        })
    };
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    h.state.shutdown(std::time::Duration::from_secs(1)).await;
    assert_eq!(h.state.doc.read().await.content, "// gravado\n");
    writer.await.unwrap();
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

// ───────────────────────── versão do documento (#51) ─────────────────────────

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(h: &Harness) -> Ws {
    let url = format!("ws://{}/ws?token={}", h.addr, h.state.token());
    tokio_tungstenite::connect_async(&url).await.unwrap().0
}

/// Próxima mensagem JSON do tipo `kind` (ignora as demais), com timeout.
async fn next_of(ws: &mut Ws, kind: &str) -> serde_json::Value {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let Some(Ok(Message::Text(text))) = ws.next().await else {
                panic!("conexão encerrada esperando {kind}");
            };
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            if value["type"] == kind {
                return value;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timeout esperando {kind}"))
}

async fn active_file(h: &Harness) -> String {
    h.state
        .doc
        .read()
        .await
        .file_path
        .to_string_lossy()
        .into_owned()
}

async fn send_patch(
    ws: &mut Ws,
    h: &Harness,
    id: u32,
    base_version: Option<u64>,
    expected_source: &str,
    text: &str,
) {
    use futures_util::SinkExt;
    let mut payload = serde_json::json!({
        "byte_start": 0, "byte_end": 0, "new_text": text,
        "expected_source": expected_source, "expected_file": active_file(h).await,
    });
    if let Some(v) = base_version {
        payload["base_version"] = v.into();
    }
    let msg = serde_json::json!({"type": "apply_patch", "payload": payload, "id": id});
    ws.send(Message::Text(msg.to_string().into()))
        .await
        .unwrap();
}

#[tokio::test]
async fn init_carries_the_document_version() {
    let h = spawn("ver_init").await;
    let mut ws = connect(&h).await;
    let init = next_of(&mut ws, "init").await;
    assert_eq!(init["version"], h.state.doc.read().await.version);
    assert!(init["version"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn patch_with_current_version_applies_and_bumps_the_version() {
    let h = spawn("ver_ok").await;
    let mut ws = connect(&h).await;
    let init = next_of(&mut ws, "init").await;
    let v = init["version"].as_u64().unwrap();
    let source = init["source"].as_str().unwrap().to_owned();

    send_patch(&mut ws, &h, 1, Some(v), &source, "// ok\n").await;
    let result = next_of(&mut ws, "result").await;
    assert!(result["error"].is_null(), "{result}");
    assert_eq!(result["version"], v + 1);
    let doc = h.state.doc.read().await;
    assert!(doc.content.starts_with("// ok\n"));
    assert_eq!(doc.version, v + 1);
}

#[tokio::test]
async fn patch_without_base_version_is_refused_and_changes_nothing() {
    let h = spawn("ver_missing").await;
    let mut ws = connect(&h).await;
    let init = next_of(&mut ws, "init").await;
    let source = init["source"].as_str().unwrap().to_owned();

    send_patch(&mut ws, &h, 1, None, &source, "// x\n").await;
    let result = next_of(&mut ws, "result").await;
    assert!(result["error"].is_string(), "{result}");
    assert_eq!(h.state.doc.read().await.content, source);
}

#[tokio::test]
async fn stale_patch_is_rejected_with_current_state_and_nothing_changes() {
    let h = spawn("ver_stale").await;
    let mut a = connect(&h).await;
    let mut b = connect(&h).await;
    let init_a = next_of(&mut a, "init").await;
    let init_b = next_of(&mut b, "init").await;
    let v = init_a["version"].as_u64().unwrap();
    assert_eq!(init_b["version"], v);
    let original = init_a["source"].as_str().unwrap().to_owned();

    // A vence; B ainda tem a versão antiga.
    send_patch(&mut a, &h, 1, Some(v), &original, "// de A\n").await;
    assert!(next_of(&mut a, "result").await["error"].is_null());
    let after_a = h.state.doc.read().await.content.clone();

    send_patch(&mut b, &h, 1, Some(v), &original, "// de B\n").await;
    let rejected = next_of(&mut b, "patch_rejected").await;
    assert_eq!(rejected["source"], after_a);
    assert_eq!(rejected["version"], v + 1);
    assert!(rejected.get("layout").is_some());
    assert!(rejected["reason"].is_string());
    let result = next_of(&mut b, "result").await;
    assert!(
        result["error"].as_str().unwrap().contains("version"),
        "{result}"
    );

    // Nada mudou por causa do patch recusado.
    {
        let doc = h.state.doc.read().await;
        assert_eq!(doc.content, after_a);
        assert_eq!(doc.version, v + 1);
    }

    // Com o estado novo, B consegue editar.
    send_patch(&mut b, &h, 2, Some(v + 1), &after_a, "// de B\n").await;
    let ok = next_of(&mut b, "result").await;
    assert!(ok["error"].is_null(), "{ok}");
    assert_eq!(ok["version"], v + 2);
}

#[tokio::test]
async fn same_version_wrong_source_is_still_refused_by_expected_source() {
    let h = spawn("ver_source").await;
    let mut ws = connect(&h).await;
    let init = next_of(&mut ws, "init").await;
    let v = init["version"].as_u64().unwrap();
    send_patch(
        &mut ws,
        &h,
        1,
        Some(v),
        "texto que o cliente inventou",
        "// x\n",
    )
    .await;
    let result = next_of(&mut ws, "result").await;
    assert!(result["error"].is_string(), "{result}");
    assert_eq!(h.state.doc.read().await.version, v);
}

#[tokio::test]
async fn stale_save_document_is_rejected_with_current_state() {
    use futures_util::SinkExt;
    let h = spawn("ver_save").await;
    let mut ws = connect(&h).await;
    let init = next_of(&mut ws, "init").await;
    let v = init["version"].as_u64().unwrap();
    let source = init["source"].as_str().unwrap().to_owned();
    let file = active_file(&h).await;

    // Sem base_version: erro.
    let msg = serde_json::json!({"type": "save_document", "id": 1,
        "payload": {"expected_source": source, "expected_file": file}});
    ws.send(Message::Text(msg.to_string().into()))
        .await
        .unwrap();
    assert!(next_of(&mut ws, "result").await["error"].is_string());

    // Versão velha: rejeitado com estado atual.
    h.state
        .apply_patch(&file, v, &source, 0, 0, "// externo\n")
        .await
        .unwrap();
    let msg = serde_json::json!({"type": "save_document", "id": 2,
        "payload": {"expected_source": source, "expected_file": file, "base_version": v}});
    ws.send(Message::Text(msg.to_string().into()))
        .await
        .unwrap();
    let rejected = next_of(&mut ws, "patch_rejected").await;
    assert_eq!(rejected["version"], v + 1);
    assert!(
        rejected["source"]
            .as_str()
            .unwrap()
            .starts_with("// externo\n")
    );
    assert!(next_of(&mut ws, "result").await["error"].is_string());

    // Versão certa: salva.
    let current = rejected["source"].as_str().unwrap().to_owned();
    let msg = serde_json::json!({"type": "save_document", "id": 3,
        "payload": {"expected_source": current, "expected_file": file, "base_version": v + 1}});
    ws.send(Message::Text(msg.to_string().into()))
        .await
        .unwrap();
    let ok = next_of(&mut ws, "result").await;
    assert!(ok["error"].is_null(), "{ok}");
}

#[tokio::test]
async fn version_grows_on_external_change_and_file_switch() {
    let h = spawn("ver_ext").await;
    dbml_cli::watcher::start_file_watcher(h.state.clone()).unwrap();
    let mut ws = connect(&h).await;
    let init = next_of(&mut ws, "init").await;
    let v = init["version"].as_u64().unwrap();

    let file = h.state.doc.read().await.file_path.clone();
    std::fs::write(&file, "// de fora\n").unwrap();
    let ext = next_of(&mut ws, "external_change").await;
    assert!(ext["version"].as_u64().unwrap() > v, "{ext}");
    assert_eq!(h.state.doc.read().await.version, ext["version"]);

    // Troca de arquivo (mesmo arquivo alvo) também conta como mudança de texto.
    let before = h.state.doc.read().await.version;
    h.state.switch_file(file).await.unwrap();
    let update = next_of(&mut ws, "update").await;
    assert!(update["version"].as_u64().unwrap() > before, "{update}");
}

// ───────────────────────── modo Comparar: versão base no git HEAD ─────────────────────────

async fn ask_git_base_for(ws: &mut Ws, expected_file: &str, id: u32) -> serde_json::Value {
    use futures_util::SinkExt;
    let msg = serde_json::json!({
        "type": "git_base",
        "payload": { "expected_file": expected_file },
        "id": id,
    });
    ws.send(Message::Text(msg.to_string().into()))
        .await
        .unwrap();
    next_of(ws, "git_base_result").await
}

fn git_in(dir: &std::path::Path, args: &[&str]) {
    let mut cmd = std::process::Command::new(dbml_cli::constants::GIT_PROGRAM);
    for var in dbml_cli::constants::GIT_REPO_ENV_VARS {
        cmd.env_remove(var);
    }
    let status = cmd
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[tokio::test]
async fn git_base_returns_head_content_of_the_active_file() {
    let h = spawn("git_ok").await;
    git_in(&h.dir, &["init", "-q"]);
    git_in(&h.dir, &["add", "schema.dbml"]);
    git_in(&h.dir, &["commit", "-q", "-m", "init"]);
    let file = h.dir.join("schema.dbml");
    std::fs::write(&file, "Table a { id int [pk]\n  extra text }\n").unwrap();

    let mut ws = connect(&h).await;
    next_of(&mut ws, "init").await;
    let reply = ask_git_base_for(&mut ws, &active_file(&h).await, 7).await;
    assert_eq!(reply["id"], 7);
    assert_eq!(reply["source"], "Table a { id int [pk] }\n", "{reply}");
    assert!(reply["error"].is_null(), "{reply}");
    // Somente leitura: o documento servido não mudou.
    assert_eq!(h.state.doc.read().await.version, 1);
}

#[tokio::test]
async fn git_base_reports_untracked_file_with_a_code() {
    let h = spawn("git_untracked").await;
    git_in(&h.dir, &["init", "-q"]);
    std::fs::write(h.dir.join("other.dbml"), "Table o { id int }\n").unwrap();
    git_in(&h.dir, &["add", "other.dbml"]);
    git_in(&h.dir, &["commit", "-q", "-m", "init"]);

    let mut ws = connect(&h).await;
    next_of(&mut ws, "init").await;
    let reply = ask_git_base_for(&mut ws, &active_file(&h).await, 1).await;
    assert!(reply["source"].is_null(), "{reply}");
    assert_eq!(reply["error_code"], "untracked", "{reply}");
    assert!(reply["error"].is_string(), "{reply}");
}

#[tokio::test]
async fn git_base_refuses_a_file_that_is_not_the_active_one() {
    let h = spawn("git_other").await;
    let mut ws = connect(&h).await;
    next_of(&mut ws, "init").await;
    let reply = ask_git_base_for(&mut ws, "/etc/passwd", 3).await;
    assert!(reply["source"].is_null(), "{reply}");
    assert_eq!(reply["error_code"], "other", "{reply}");
}

#[tokio::test]
async fn git_base_requires_the_session_token() {
    let h = spawn("git_token").await;
    let url = format!("ws://{}/ws", h.addr);
    assert!(tokio_tungstenite::connect_async(&url).await.is_err());
}
