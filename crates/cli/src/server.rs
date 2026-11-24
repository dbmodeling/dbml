use crate::assets::static_handler;
use crate::constants::{CONTENT_TYPE_JSON, HEALTH_BODY_OK, HEALTH_ROUTE};
use crate::state::{AppState, StaleVersion};
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
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

#[derive(Deserialize, Debug)]
#[serde(tag = "type", content = "payload")]
enum ClientMessage {
    #[serde(rename = "apply_patch")]
    ApplyPatch {
        /// Versão do documento em que o patch foi calculado (obrigatória).
        base_version: u64,
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
        base_version: u64,
        expected_source: String,
        expected_file: String,
    },
    #[serde(rename = "open_file")]
    OpenFile(String),
    /// Modo Comparar: pede o conteúdo do arquivo ativo no `HEAD` do git. Somente leitura.
    #[serde(rename = "git_base")]
    GitBase { expected_file: String },
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

/// Aviso do `--public-demo`: sem token, o acesso é aberto a quem alcançar o servidor.
pub const PUBLIC_DEMO_WARNING: &str = "token de sessão desativado. Qualquer pessoa que alcance \
    este servidor lê e grava o workspace e pode usar a conexão com banco de dados. Use apenas \
    com pasta sem dados reais.";

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
    if state.public_demo {
        return next.run(req).await;
    }
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

/// `Host` values (lowercase `name:port`) this server answers to. Anything else is refused, which
/// defeats DNS rebinding: the attacker's name never matches the allowlist.
#[must_use]
pub fn allowed_hosts(bind_host: &str, port: u16) -> Vec<String> {
    allowed_hosts_with_extra(bind_host, port, &[])
}

/// Como [`allowed_hosts`], mais `extra` (ex.: domínio público atrás de proxy). Cada item aceita
/// com ou sem porta: sem porta vale exatamente aquele `Host` (proxy HTTPS não envia a porta);
/// com porta, só aquela. Curingas e itens vazios são ignorados, então os demais hosts seguem
/// recusados (proteção contra DNS rebinding intacta).
#[must_use]
pub fn allowed_hosts_with_extra(bind_host: &str, port: u16, extra: &[String]) -> Vec<String> {
    let mut hosts = base_allowed_hosts(bind_host, port);
    for raw in extra {
        let entry = raw.trim().to_ascii_lowercase();
        let entry = entry
            .strip_prefix("https://")
            .or_else(|| entry.strip_prefix("http://"))
            .unwrap_or(&entry)
            .trim_end_matches('/')
            .to_owned();
        if entry.is_empty() || entry.contains('*') || entry.contains('/') {
            continue;
        }
        if !hosts.contains(&entry) {
            hosts.push(entry);
        }
    }
    hosts
}

fn base_allowed_hosts(bind_host: &str, port: u16) -> Vec<String> {
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
    req: Request,
    next: Next,
) -> Response {
    if request_allowed(req.headers(), &allowed) {
        next.run(req).await
    } else {
        (StatusCode::FORBIDDEN, "forbidden host or origin").into_response()
    }
}

async fn health_handler() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, CONTENT_TYPE_JSON)],
        HEALTH_BODY_OK,
    )
        .into_response()
}

pub fn create_router(state: AppState, allowed: Vec<String>) -> Router {
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
    let app = Router::new()
        .merge(protected)
        .fallback(static_handler)
        .layer(middleware::from_fn_with_state(
            std::sync::Arc::new(allowed),
            guard,
        ))
        .with_state(state);
    Router::new()
        .route(HEALTH_ROUTE, get(health_handler))
        .merge(app)
}

/// Máximo padrão de conexões WebSocket simultâneas (configurável); além disso o upgrade é recusado com `503`.
pub const MAX_WS_CONNECTIONS: usize = 256;

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    let Some(slot) = state
        .lifecycle
        .try_acquire_connection(state.max_ws_connections)
    else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "too many open connections (limit {}); close another tab",
                state.max_ws_connections
            ),
        )
            .into_response();
    };
    ws.max_message_size(MAX_WS_PAYLOAD_BYTES)
        .max_frame_size(MAX_WS_PAYLOAD_BYTES)
        .on_upgrade(move |socket| async move {
            // A conexão roda numa task rastreada: o encerramento do servidor a espera terminar.
            // A vaga só volta quando a task termina.
            let lifecycle = std::sync::Arc::clone(&state.lifecycle);
            lifecycle.spawn(async move {
                handle_socket(socket, state).await;
                drop(slot);
            });
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
            let msg: Arc<str> = tokio::select! {
                outcome = rx.recv() => match outcome {
                    Ok(msg) => msg,
                    // Cliente atrasado: perdeu mensagens, então reenvia o documento atual e segue.
                    Err(RecvError::Lagged(_)) => {
                        let doc = state.doc.read().await;
                        state.document_message("update", &doc).await.into()
                    }
                    Err(RecvError::Closed) => break,
                },
                Some(msg) = reply_rx.recv() => msg.into(),
                else => break,
            };
            if sender
                .send(Message::Text(msg.as_ref().into()))
                .await
                .is_err()
            {
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
            // Pedido de leitura com resposta própria (`git_base_result`), sem `result`/versão.
            if let ClientMessage::GitBase { expected_file } = &client_msg {
                let _ = reply_tx.send(state.git_base_message(&id, expected_file).await);
                continue;
            }
            let result = handle_client_message(&state, client_msg).await;
            // Versão velha: primeiro o estado atual (só para quem errou), depois o erro do pedido.
            if let Some(stale) = result
                .as_ref()
                .err()
                .and_then(|e| e.downcast_ref::<StaleVersion>())
            {
                let _ = reply_tx.send(stale.rejection.clone());
            }
            let (version, error) = match result {
                Ok(version) => (version, None),
                Err(err) => (None, Some(format!("{err:#}"))),
            };
            let _ = reply_tx.send(
                serde_json::json!({"type": "result", "id": id, "error": error, "version": version})
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

/// Trata um pedido do cliente; devolve a versão do documento quando o pedido a muda ou confirma.
async fn handle_client_message(
    state: &AppState,
    msg: ClientMessage,
) -> anyhow::Result<Option<u64>> {
    match msg {
        ClientMessage::ApplyPatch {
            base_version,
            byte_start,
            byte_end,
            new_text,
            expected_file,
            expected_source,
        } => state
            .apply_patch(
                &expected_file,
                base_version,
                &expected_source,
                byte_start,
                byte_end,
                &new_text,
            )
            .await
            .map(Some),
        ClientMessage::SaveLayout {
            layout,
            expected_file,
        } => {
            let layout = serde_json::to_string_pretty(&layout)?;
            state.save_layout_for(&expected_file, layout).await?;
            Ok(None)
        }
        ClientMessage::SaveDocument {
            base_version,
            expected_source,
            expected_file,
        } => state
            .verify_document(&expected_file, base_version, &expected_source)
            .await
            .map(Some),
        ClientMessage::OpenFile(path) => {
            state.switch_file(std::path::PathBuf::from(path)).await?;
            Ok(None)
        }
        // Tratado antes, em `handle_socket` (resposta própria).
        ClientMessage::GitBase { .. } => Ok(None),
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

#[cfg(test)]
mod exposure_tests {
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

    fn extra(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn default_stays_loopback_only() {
        let a = allowed_hosts_with_extra("0.0.0.0", 3000, &[]);
        assert_eq!(a, allowed_hosts("0.0.0.0", 3000));
        assert!(!request_allowed(&hdr(None, "meu-app.onrender.com"), &a));
    }

    #[test]
    fn extra_host_without_port_is_accepted_behind_https_proxy() {
        let a = allowed_hosts_with_extra("0.0.0.0", 3000, &extra(&["Meu-App.onrender.com"]));
        assert!(request_allowed(&hdr(None, "meu-app.onrender.com"), &a));
        assert!(request_allowed(
            &hdr(Some("https://meu-app.onrender.com"), "meu-app.onrender.com"),
            &a
        ));
        assert!(!request_allowed(
            &hdr(Some("https://evil.example"), "meu-app.onrender.com"),
            &a
        ));
    }

    #[test]
    fn extra_host_only_matches_declared_form_and_others_stay_refused() {
        let a = allowed_hosts_with_extra("0.0.0.0", 3000, &extra(&["demo.example.com:8080"]));
        assert!(request_allowed(&hdr(None, "demo.example.com:8080"), &a));
        assert!(!request_allowed(&hdr(None, "demo.example.com"), &a));
        assert!(!request_allowed(&hdr(None, "evil.example"), &a));
    }

    #[test]
    fn extra_host_ignores_wildcards_empty_and_url_forms() {
        let a = allowed_hosts_with_extra(
            "0.0.0.0",
            3000,
            &extra(&["*", "", "  ", "*.example.com", "https://ok.example.com/"]),
        );
        assert!(!request_allowed(&hdr(None, "anything.example.com"), &a));
        assert!(request_allowed(&hdr(None, "ok.example.com"), &a));
    }

    #[test]
    fn fixed_token_is_compared_like_any_other() {
        assert!(token_matches("fixed-token-1234567", "fixed-token-1234567"));
        assert!(!token_matches("fixed-token-1234567", "random"));
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
