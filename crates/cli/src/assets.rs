use crate::constants::{
    CONTENT_TYPE_HTML, CSP_PREFIX, CSP_SUFFIX, HEAD_OPEN_TAG, HEADER_FRAME_DENY, HEADER_NOSNIFF,
    HEADER_REFERRER_NO_REFERRER, INDEX_HTML_PATH,
};
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
const TOKEN_SHIM_BODY: &str = r#"(function(){try{var k="dbml-token",q=new URLSearchParams(location.search),t=q.get("token");if(t){sessionStorage.setItem(k,t);q.delete("token");var s=q.toString();history.replaceState(null,"",location.pathname+(s?"?"+s:"")+location.hash)}t=sessionStorage.getItem(k);if(!t)return;var W=window.WebSocket;window.WebSocket=function(u,p){var a=new URL(u,location.href);if(a.host===location.host){a.searchParams.set("token",t)}return p===undefined?new W(a.href):new W(a.href,p)};window.WebSocket.prototype=W.prototype;["CONNECTING","OPEN","CLOSING","CLOSED"].forEach(function(n){window.WebSocket[n]=W[n]});var F=window.fetch;window.fetch=function(i,o){var u=new URL(typeof i==="string"?i:i.url||String(i),location.href);if(u.host===location.host&&u.pathname.indexOf("/api/")===0){o=Object.assign({},o);var h=new Headers(o.headers||(typeof i==="object"&&i.headers)||undefined);h.set("X-DBML-Token",t);o.headers=h}return F.call(this,i,o)}}catch(e){}})();"#;

/// Insere o shim com `nonce` logo após `<head>`; devolve o HTML intacto se não houver `<head>`.
fn inject_token_shim(html: &[u8], nonce: &str) -> Vec<u8> {
    let text = String::from_utf8_lossy(html);
    match text.find(HEAD_OPEN_TAG) {
        Some(at) => {
            let at = at + HEAD_OPEN_TAG.len();
            format!(
                "{}<script nonce=\"{nonce}\">{TOKEN_SHIM_BODY}</script>{}",
                &text[..at],
                &text[at..]
            )
            .into_bytes()
        }
        None => html.to_vec(),
    }
}

fn csp_header(nonce: &str) -> String {
    format!("{CSP_PREFIX}{nonce}{CSP_SUFFIX}")
}

fn html_response(html: &[u8]) -> Response {
    let nonce = crate::state::generate_token();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, CONTENT_TYPE_HTML)
        .header(header::CONTENT_SECURITY_POLICY, csp_header(&nonce))
        .header(header::X_CONTENT_TYPE_OPTIONS, HEADER_NOSNIFF)
        .header(header::X_FRAME_OPTIONS, HEADER_FRAME_DENY)
        .header(header::REFERRER_POLICY, HEADER_REFERRER_NO_REFERRER)
        .body(Body::from(inject_token_shim(html, &nonce)))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub async fn static_handler(uri: axum::http::Uri) -> Response {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() {
        path = INDEX_HTML_PATH.to_string();
    }

    match WebAssets::get(&path) {
        Some(content) if path == INDEX_HTML_PATH => html_response(&content.data),
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::X_CONTENT_TYPE_OPTIONS, HEADER_NOSNIFF)
                .header(header::X_FRAME_OPTIONS, HEADER_FRAME_DENY)
                .header(header::REFERRER_POLICY, HEADER_REFERRER_NO_REFERRER)
                .body(Body::from(content.data))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        None => {
            // Fallback para index.html (SPA routing)
            if let Some(content) = WebAssets::get(INDEX_HTML_PATH) {
                html_response(&content.data)
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_response_sets_csp_nonce_and_security_headers() {
        let resp = html_response(b"<!doctype html><html><head></head><body></body></html>");
        assert_eq!(resp.status(), StatusCode::OK);
        let headers = resp.headers();
        assert_eq!(
            headers
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .and_then(|v| v.to_str().ok()),
            Some("nosniff")
        );
        assert_eq!(
            headers
                .get(header::X_FRAME_OPTIONS)
                .and_then(|v| v.to_str().ok()),
            Some("DENY")
        );
        let csp = headers
            .get(header::CONTENT_SECURITY_POLICY)
            .and_then(|v| v.to_str().ok())
            .expect("CSP header must be present");
        assert!(csp.contains("script-src 'self' 'nonce-"), "{csp}");
        assert!(csp.contains("'wasm-unsafe-eval'"), "{csp}");
        assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
    }
}
