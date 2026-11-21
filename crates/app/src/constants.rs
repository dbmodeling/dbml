//! Constantes compartilhadas do aplicativo Desktop (limites de sessão, geometria padrão e IDs de UI).

/// Limite máximo de viewports por arquivo mantidos em `session.json`.
pub const MAX_VIEWPORTS: usize = 64;

/// Posição padrão de fallback quando uma tabela não está no mapa de posições do canvas.
pub const DEFAULT_CARD_POS: (f32, f32) = (0.0, 0.0);

/// Rótulo e IDs dos controles de TLS no modal "Conectar ao banco".
pub const TLS_FIELD_LABEL: &str = "TLS";
pub const ID_TLS_AUTO: &str = "db-tls-auto";
pub const ID_TLS_VERIFY: &str = "db-tls-verify";
pub const ID_TLS_REQUIRE: &str = "db-tls-require";
pub const ID_TLS_DISABLE: &str = "db-tls-disable";

/// Dimensões do seletor de TLS no formulário de conexão (em pixels).
pub const TLS_ROW_GAP_PX: f32 = 12.0;
pub const TLS_BTN_GAP_PX: f32 = 6.0;
pub const TLS_BTN_HEIGHT_PX: f32 = 28.0;
pub const TLS_LABEL_WIDTH_PX: f32 = 96.0;
pub const TLS_LABEL_FONT_PX: f32 = 12.5;

/// Configuração do runtime tokio reutilizado pela introspecção de banco.
pub const INTROSPECT_WORKER_THREADS: usize = 1;
pub const INTROSPECT_TIMEOUT_SECS: u64 = 10;

/// Configuração do log rotativo estruturado do app Desktop (#98).
pub const LOGS_DIR_NAME: &str = "logs";
pub const LOG_FILE_NAME: &str = "db-modeling.log";
pub const LOG_ROTATED_FILE_NAME: &str = "db-modeling.log.1";
pub const MAX_LOG_FILE_BYTES: u64 = 1024 * 1024;
pub const MAX_LOG_LINE_BYTES: usize = 8 * 1024;
pub const LOG_TRUNCATED_SUFFIX: &str = "...[truncated]";
