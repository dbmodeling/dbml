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

/// Capacidade em bytes de um cabeçalho `ETag` SHA-256 com aspas (`"` + 64 hex + `"`).
pub const ETAG_HEADER_CAPACITY: usize = 66;

/// Prefixo de ativos com hash de conteúdo no nome (imutáveis em cache).
pub const IMMUTABLE_ASSETS_PREFIX: &str = "assets/";
/// Política de cache para ativos versionados por hash.
pub const CACHE_CONTROL_IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// Política de cache para `index.html` e ativos não versionados.
pub const CACHE_CONTROL_NO_CACHE: &str = "no-cache";

/// Valores aceitos para o campo `tls` em `/api/introspect`.
pub const TLS_MODE_AUTO: &str = "auto";
pub const TLS_MODE_VERIFY: &str = "verify";
pub const TLS_MODE_REQUIRE: &str = "require";
pub const TLS_MODE_DISABLE: &str = "disable";
