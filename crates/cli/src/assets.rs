use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/"]
pub struct WebAssets;

/// Script injetado no `index.html`: lê `?token=` da URL aberta pelo terminal, guarda em
/// `sessionStorage` e anexa o token ao `WebSocket` (`/ws?token=`) e ao `fetch` (`X-DBML-Token`)
/// dirigidos a este servidor. O token nunca é embutido no HTML: quem não recebeu a URL do
/// terminal não o obtém. Pode ser removido quando o front passar a enviar o token por conta
/// própria (ver `docs/09-cli-token.md`).
const TOKEN_SHIM: &str = r#"<script>(function(){try{var k="dbml-token",q=new URLSearchParams(location.search),t=q.get("token");if(t){sessionStorage.setItem(k,t);q.delete("token");var s=q.toString();history.replaceState(null,"",location.pathname+(s?"?"+s:"")+location.hash)}t=sessionStorage.getItem(k);if(!t)return;var W=window.WebSocket;window.WebSocket=function(u,p){var a=new URL(u,location.href);if(a.host===location.host){a.searchParams.set("token",t)}return p===undefined?new W(a.href):new W(a.href,p)};window.WebSocket.prototype=W.prototype;["CONNECTING","OPEN","CLOSING","CLOSED"].forEach(function(n){window.WebSocket[n]=W[n]});var F=window.fetch;window.fetch=function(i,o){var u=new URL(typeof i==="string"?i:i.url||String(i),location.href);if(u.host===location.host&&u.pathname.indexOf("/api/")===0){o=Object.assign({},o);var h=new Headers(o.headers||(typeof i==="object"&&i.headers)||undefined);h.set("X-DBML-Token",t);o.headers=h}return F.call(this,i,o)}}catch(e){}})();</script>"#;

/// Insere o [`TOKEN_SHIM`] logo após `<head>`; devolve o HTML intacto se não houver `<head>`.
fn inject_token_shim(html: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(html);
    match text.find("<head>") {
        Some(at) => {
            let at = at + "<head>".len();
            format!("{}{TOKEN_SHIM}{}", &text[..at], &text[at..]).into_bytes()
        }
        None => html.to_vec(),
    }
}

pub async fn static_handler(uri: axum::http::Uri) -> Response {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() {
        path = "index.html".to_string();
    }

    match WebAssets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            let body = if path == "index.html" {
                Body::from(inject_token_shim(&content.data))
            } else {
                Body::from(content.data)
            };
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(body)
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        None => {
            // Fallback para index.html (SPA routing)
            if let Some(content) = WebAssets::get("index.html") {
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(Body::from(inject_token_shim(&content.data)))
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
    }
}
