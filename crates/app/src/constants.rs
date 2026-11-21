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

/// Chaves de persistência e limites do tour guiado / modal de novidades em `settings.txt`.
pub const SETTING_ONBOARDING_DONE: &str = "onboarding_done";
pub const SETTING_LAST_SEEN_VERSION: &str = "last_seen_version";
pub const MAX_VERSION_LEN: usize = 32;

/// Variáveis de harness que suprimem o disparo automático do tour/novidades.
pub const ENV_NO_GUIDE: &str = "DBM_NO_GUIDE";
pub const ENV_PRESELECT: &str = "DBM_SELECT";

/// Nomes canônicos das ações rápidas referenciadas em `assets/product-guide.json`.
pub const GUIDE_ACTION_TOGGLE_MINIMAP: &str = "ToggleMinimap";
pub const GUIDE_ACTION_QUICK_SEARCH: &str = "QuickSearch";

/// Dimensões da geometria do Spotlight Tour e do Modal de Novidades (em pixels).
pub const TOUR_HOLE_PAD_PX: f32 = 6.0;
pub const TOUR_CARD_GAP_PX: f32 = 14.0;
pub const TOUR_CARD_W_PX: f32 = 340.0;
pub const TOUR_CARD_H_PX: f32 = 190.0;
pub const TOUR_VIEWPORT_MARGIN_PX: f32 = 12.0;
pub const TOUR_ARROW_MIN_OFFSET_PX: f32 = 24.0;
pub const TOUR_ARROW_BADGE_SIZE_PX: f32 = 18.0;
pub const TOUR_ARROW_HALF_PX: f32 = 8.0;
pub const TOUR_ARROW_OVERHANG_PX: f32 = 12.0;
pub const TOUR_ARROW_INSET_PX: f32 = 4.0;
pub const TOUR_MIN_VIEWPORT_W_PX: f32 = 320.0;
pub const TOUR_MIN_VIEWPORT_H_PX: f32 = 240.0;
pub const TOUR_BOUNDS_EPSILON_PX: f32 = 0.5;
pub const WHATS_NEW_DIALOG_W_PX: f32 = 560.0;
pub const WHATS_NEW_DIALOG_MAX_H_PX: f32 = 560.0;
