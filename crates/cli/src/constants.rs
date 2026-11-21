//! Constantes compartilhadas da CLI (cabeçalhos de segurança HTTP, CSP e modos TLS).

/// Caminho padrão do documento HTML principal no bundle embutido.
pub const INDEX_HTML_PATH: &str = "index.html";
/// Content-Type do documento HTML da SPA.
pub const CONTENT_TYPE_HTML: &str = "text/html; charset=utf-8";
/// Tag de abertura `<head>` onde o shim de token é inserido.
pub const HEAD_OPEN_TAG: &str = "<head>";

/// Valor do cabeçalho `X-Content-Type-Options`.
pub const HEADER_NOSNIFF: &str = "nosniff";
/// Valor do cabeçalho `X-Frame-Options`.
pub const HEADER_FRAME_DENY: &str = "DENY";
/// Valor do cabeçalho `Referrer-Policy`.
pub const HEADER_REFERRER_NO_REFERRER: &str = "no-referrer";

/// Prefixo da diretiva `Content-Security-Policy` antes do nonce.
pub const CSP_PREFIX: &str = "default-src 'self'; script-src 'self' 'nonce-";
/// Sufixo da diretiva `Content-Security-Policy` após o nonce.
pub const CSP_SUFFIX: &str = "' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' ws: wss:; frame-ancestors 'none'; base-uri 'self'; object-src 'none'";

/// Valores aceitos para o campo `tls` em `/api/introspect`.
pub const TLS_MODE_AUTO: &str = "auto";
pub const TLS_MODE_VERIFY: &str = "verify";
pub const TLS_MODE_REQUIRE: &str = "require";
pub const TLS_MODE_DISABLE: &str = "disable";
