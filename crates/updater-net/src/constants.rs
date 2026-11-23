//! Todas as strings e números da camada de rede do updater (zero magic strings/numbers fora
//! deste arquivo). As constantes do núcleo puro ficam em `updater::constants`.

use std::time::Duration;

// --- Tamanhos ----------------------------------------------------------------------------------

/// Bytes em um KiB.
pub const BYTES_PER_KIB: u64 = 1024;
/// Bytes em um MiB.
pub const BYTES_PER_MIB: u64 = 1024 * BYTES_PER_KIB;
/// Tamanho máximo aceito do manifesto `latest.json` (64 KiB).
pub const MAX_MANIFEST_BYTES: u64 = 64 * BYTES_PER_KIB;
/// Tamanho máximo aceito da assinatura `.sig` de 4 linhas (8 KiB).
pub const MAX_SIGNATURE_BYTES: u64 = 8 * BYTES_PER_KIB;
/// Tamanho máximo aceito de um artefato de atualização (512 MiB).
pub const MAX_DOWNLOAD_BYTES: u64 = 512 * BYTES_PER_MIB;
/// Espaço livre mínimo exigido no volume do cache antes de baixar (1 GiB: o artefato, a
/// extração e o respaldo da instalação).
pub const MIN_FREE_SPACE_BYTES: u64 = 1024 * BYTES_PER_MIB;
/// Tamanho do buffer de cópia do download (64 KiB).
pub const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;

// --- Timeouts e redirecionamentos --------------------------------------------------------------

/// Timeout de conexão (resolução de nome + TCP + TLS é limitado por `CONNECT_TIMEOUT` e
/// `RESOLVE_TIMEOUT`).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Timeout de resolução de DNS.
pub const RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);
/// Timeout de cada leitura do corpo (um download lento mas vivo não estoura).
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Timeout de cada escrita da requisição.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
/// Timeout global de uma requisição pequena (manifesto, assinatura): 30 s.
pub const SMALL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Máximo de redirecionamentos seguidos (`latest/download` → `releases/download` → CDN = 2).
pub const MAX_REDIRECTS: u32 = 5;

// --- Política de URL ---------------------------------------------------------------------------

/// Único esquema aceito em produção.
pub const SCHEME_HTTPS: &str = "https";
/// Hosts exatos permitidos (o próprio GitHub).
pub const ALLOWED_HOSTS: [&str; 1] = ["github.com"];
/// Sufixos de host permitidos para o CDN de ativos (`*.githubusercontent.com`); o ponto inicial
/// impede `evilgithubusercontent.com`.
pub const ALLOWED_HOST_SUFFIXES: [&str; 1] = [".githubusercontent.com"];
/// Separador de segmentos de caminho.
pub const PATH_SEPARATOR: char = '/';
/// Segmento "ponto" (diretório atual).
pub const DOT_SEGMENT: &str = ".";
/// Segmento "ponto-ponto" (travessia).
pub const DOT_DOT_SEGMENT: &str = "..";
/// Percent-encoding do ponto, em minúsculas (comparado após `to_ascii_lowercase`).
pub const ENCODED_DOT: &str = "%2e";
/// Percent-encodings proibidos dentro de um segmento (`/` e `\`), em minúsculas.
pub const ENCODED_SEPARATORS: [&str; 2] = ["%2f", "%5c"];
/// Separador de diretórios do Windows, proibido em qualquer caminho de URL.
pub const BACKSLASH: char = '\\';
/// Separador entre esquema e autoridade em uma URL absoluta.
pub const SCHEME_SEPARATOR: &str = "://";
/// Início do query string.
pub const QUERY_START: char = '?';
/// Início do fragmento.
pub const FRAGMENT_START: char = '#';
/// Caracteres permitidos (além de ASCII alfanumérico) no nome do arquivo baixado.
pub const FILE_NAME_EXTRA_CHARS: [char; 3] = ['.', '-', '_'];

// --- HTTP --------------------------------------------------------------------------------------

/// Prefixo do `User-Agent`; o sufixo é `/<versão do app>`. Apenas o nome do cliente e a versão
/// saem no `User-Agent` (sem SO, arquitetura nem identificador de máquina).
pub const USER_AGENT_PREFIX: &str = "db-modeling-updater";
/// Versão que acompanha o `User-Agent` (a mesma do app: versão única do workspace).
pub const UPDATER_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Cabeçalho de requisição condicional.
pub const HEADER_IF_NONE_MATCH: &str = "If-None-Match";
/// Cabeçalho de retomada.
pub const HEADER_RANGE: &str = "Range";
/// Cabeçalho do `ETag`.
pub const HEADER_ETAG: &str = "ETag";
/// Cabeçalho de destino do redirecionamento.
pub const HEADER_LOCATION: &str = "Location";
/// Cabeçalho do intervalo devolvido em `206`.
pub const HEADER_CONTENT_RANGE: &str = "Content-Range";
/// Cabeçalho do tamanho do corpo.
pub const HEADER_CONTENT_LENGTH: &str = "Content-Length";
/// Prefixo do valor de `Range` (`bytes=<início>-`).
pub const RANGE_UNIT_PREFIX: &str = "bytes=";
/// Prefixo do valor de `Content-Range` (`bytes <início>-<fim>/<total>`).
pub const CONTENT_RANGE_UNIT_PREFIX: &str = "bytes ";
/// Separador do início e fim em `Range` e `Content-Range`.
pub const RANGE_DASH: char = '-';
/// `200 OK`.
pub const STATUS_OK: u16 = 200;
/// `206 Partial Content`.
pub const STATUS_PARTIAL_CONTENT: u16 = 206;
/// `304 Not Modified`.
pub const STATUS_NOT_MODIFIED: u16 = 304;
/// Início da faixa de status de redirecionamento.
pub const STATUS_REDIRECT_MIN: u16 = 300;
/// Fim (exclusivo) da faixa de status de redirecionamento.
pub const STATUS_REDIRECT_MAX: u16 = 400;
/// Status de redirecionamento aceitos (`301`, `302`, `303`, `307`, `308`).
pub const REDIRECT_STATUSES: [u16; 5] = [301, 302, 303, 307, 308];

// --- Proxy -------------------------------------------------------------------------------------

/// Variáveis de ambiente de proxy honradas, em ordem de precedência.
pub const ENV_PROXY_VARS: [&str; 2] = ["HTTPS_PROXY", "https_proxy"];

// --- Download ----------------------------------------------------------------------------------

/// Extensão do arquivo de download em andamento (nunca é usado como pronto).
pub const PART_EXTENSION: &str = "part";
/// Modo do arquivo de download em Unix (somente o dono lê e escreve).
#[cfg(unix)]
pub const PART_FILE_UNIX_MODE: u32 = 0o600;

// --- Windows -----------------------------------------------------------------------------------

/// SDDL do diretório de downloads: DACL protegida (`P`, não herda) com controle total para o dono
/// do objeto (`OW`, Owner Rights) e para `SY` (`SYSTEM`), herdado por subpastas e arquivos
/// (`OICI`). Não verificado em Windows (ver `private_dir`).
#[cfg(windows)]
pub const DIR_ACL_SDDL: &str = "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)";

// --- Gancho de teste (só debug ou feature `harness`) -------------------------------------------

/// Variável com a origem (esquema + host + porta) que substitui a do GitHub; só em debug ou
/// com a feature `harness`.
#[cfg(any(debug_assertions, feature = "harness"))]
pub const ENV_UPDATE_URL: &str = "DBM_UPDATE_URL";
/// Variável com a chave pública minisign (base64) de teste que substitui as embutidas; só em
/// debug ou com a feature `harness`.
#[cfg(any(debug_assertions, feature = "harness"))]
pub const ENV_UPDATE_TEST_PUBLIC_KEY: &str = "DBM_UPDATE_TEST_PUBLIC_KEY";
