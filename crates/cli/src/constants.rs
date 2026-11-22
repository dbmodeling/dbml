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

/// Rota pública de verificação de integridade (Docker `HEALTHCHECK` / probes).
pub const HEALTH_ROUTE: &str = "/health";
/// Content-Type para respostas JSON simples.
pub const CONTENT_TYPE_JSON: &str = "application/json";
/// Corpo JSON retornado por `GET /health`.
pub const HEALTH_BODY_OK: &str = r#"{"status":"ok"}"#;

/// Executável chamado pelo modo Comparar para ler a versão do arquivo no `HEAD`.
pub const GIT_PROGRAM: &str = "git";
/// Variáveis herdadas (ex.: de hooks do git) que redirecionariam o `git` para outro repositório.
pub const GIT_REPO_ENV_VARS: [&str; 5] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_COMMON_DIR",
];

/// Resposta do servidor ao pedido `git_base` do cliente (versão base no git `HEAD`): `source` (sucesso) ou `error` + `error_code`.
pub const WS_MSG_GIT_BASE_RESULT: &str = "git_base_result";

/// Códigos de erro estáveis de `git_base_result` (o cliente traduz a mensagem a partir deles).
pub const GIT_BASE_ERR_GIT_UNAVAILABLE: &str = "git_unavailable";
pub const GIT_BASE_ERR_NOT_A_REPO: &str = "not_a_repo";
pub const GIT_BASE_ERR_NO_COMMITS: &str = "no_commits";
pub const GIT_BASE_ERR_UNTRACKED: &str = "untracked";
pub const GIT_BASE_ERR_NOT_UTF8: &str = "not_utf8";
/// Qualquer outra falha (arquivo ativo mudou, tarefa interrompida).
pub const GIT_BASE_ERR_OTHER: &str = "other";
