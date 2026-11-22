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

/// Connection string do modal "Conectar ao banco": separador do esquema e usuário/senha
/// provisórios usados só para montar a string sem usuário ou sem expor a senha (nunca enviados).
pub const CONN_SCHEME_SEP: &str = "://";
pub const CONN_USER_STUB: &str = "u";
pub const CONN_PASSWORD_STUB: &str = "p";
/// Espera após a última tecla na connection string antes de lê-la (ms).
pub const CONN_STRING_DEBOUNCE_MS: u64 = 150;

/// IDs dos controles do modal "Conectar ao banco".
pub const ID_DB_ENGINE: &str = "db-engine";
pub const ID_DB_ENGINE_MENU: &str = "db-engine-menu";
pub const ID_DB_CONN_COPY: &str = "db-conn-copy";
pub const ID_DB_PASSWORD_REVEAL: &str = "db-password-reveal";
pub const ID_DB_PICK_FILE: &str = "db-pick-file";
pub const ID_DB_STATUS: &str = "db-status";
pub const ID_DB_WARNINGS: &str = "db-warn";
pub const ID_DB_TRUST: &str = "db-trust";
pub const ID_DB_TEST: &str = "db-test";
pub const ID_DB_CONNECT: &str = "db-connect";
pub const ID_DB_CANCEL: &str = "db-cancel";
pub const ID_DB_REPLACE: &str = "db-replace";
pub const ID_DB_INSERT: &str = "db-insert";

/// Tecla que também abre o dropdown de banco (além de Enter/Espaço do botão).
pub const KEY_ARROW_DOWN: &str = "down";

/// Geometria do modal "Conectar ao banco" (em pixels).
pub const DB_DIALOG_W_PX: f32 = 600.0;
pub const DB_DIALOG_TOP_PX: f32 = 64.0;
pub const DB_FORM_PAD_X_PX: f32 = 18.0;
pub const DB_FORM_PAD_Y_PX: f32 = 10.0;
pub const DB_FORM_GAP_PX: f32 = 8.0;
pub const DB_FIELD_GAP_PX: f32 = 12.0;
pub const DB_FIELD_LABEL_W_PX: f32 = 96.0;
/// Rótulo da segunda coluna (Porta, Senha) nas linhas duplas.
pub const DB_FIELD_LABEL2_W_PX: f32 = 48.0;
pub const DB_FIELD_LABEL_FONT_PX: f32 = 12.5;
pub const DB_FIELD_H_PX: f32 = 32.0;
pub const DB_FIELD_PAD_X_PX: f32 = 10.0;
pub const DB_FIELD_RADIUS_PX: f32 = 7.0;
pub const DB_PORT_FIELD_W_PX: f32 = 88.0;
pub const DB_CONN_FONT_PX: f32 = 12.0;
pub const DB_ERROR_FONT_PX: f32 = 12.0;
pub const DB_ENGINE_TRIGGER_W_PX: f32 = 200.0;
pub const DB_ENGINE_MENU_W_PX: f32 = 200.0;
pub const DB_STATUS_MAX_H_PX: f32 = 72.0;
pub const DB_STATUS_MARGIN_TOP_PX: f32 = 4.0;
pub const DB_STATUS_PAD_X_PX: f32 = 12.0;
pub const DB_STATUS_PAD_Y_PX: f32 = 10.0;
pub const DB_STATUS_RADIUS_PX: f32 = 8.0;
pub const DB_STATUS_ICON_PX: f32 = 14.0;
pub const DB_WARNINGS_MAX_H_PX: f32 = 64.0;
pub const DB_FOOTER_PAD_Y_PX: f32 = 16.0;

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

/// IDs de UI para criação de tabela associativa (relacionamentos N:N).
pub const ID_CREATE_JUNCTION_BTN: &str = "insp-create-junction";
pub const ID_JUNCTION_CONFIRM_BTN: &str = "insp-junction-confirm";
pub const ID_JUNCTION_CANCEL_BTN: &str = "insp-junction-cancel";
pub const JUNCTION_BTN_HEIGHT_PX: f32 = 30.0;

/// Base do modo Comparar (#18): `git show HEAD:./<arquivo>`.
pub const GIT_BIN: &str = "git";
pub const GIT_HEAD_SPEC_PREFIX: &str = "HEAD:./";
/// Variáveis herdadas (ex.: de hooks do git) que redirecionariam o `git` para outro repositório.
pub const GIT_REPO_ENV_VARS: [&str; 5] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_COMMON_DIR",
];
/// `CREATE_NO_WINDOW`: o `git` não abre console no Windows.
#[cfg(windows)]
pub const WIN_CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Geometria do modo Comparar (unidades de mundo do canvas, salvo indicação `_PX`).
/// Espaço entre a linha do cabeçalho e a primeira coluna do cartão (`pt` das linhas).
pub const CARD_ROWS_TOP_PAD: f32 = 4.0;
/// Folga entre cartões fantasma (removidos) e o diagrama atual.
pub const DIFF_GHOST_GAP: f32 = 40.0;
/// Quantos cartões fantasma por fileira, no máximo, abaixo do diagrama.
pub const DIFF_GHOST_PER_ROW: f32 = 4.0;
/// Opacidade dos cartões/linhas fantasma (contexto do que foi removido).
pub const DIFF_GHOST_OPACITY: f32 = 0.55;
/// Opacidade das tabelas, enums e relações inalteradas (o que mudou fica em evidência).
pub const DIFF_UNCHANGED_OPACITY: f32 = 0.5;
/// Fundo das linhas alteradas (alfa sobre a cor do status).
pub const DIFF_ROW_TINT: f32 = 0.14;
/// Halo do cartão alterado (alfa e espessura em px de tela).
pub const DIFF_HALO_ALPHA: f32 = 0.22;
pub const DIFF_HALO_SPREAD_PX: f32 = 3.0;
/// Traço tracejado das relações removidas (px de tela).
pub const DIFF_DASH_PX: f32 = 5.0;
pub const DIFF_DASH_GAP_PX: f32 = 4.0;
pub const DIFF_EDGE_W_PX: f32 = 1.6;
/// Painel de resumo do modo Comparar (px de tela).
pub const DIFF_PANEL_W_PX: f32 = 320.0;
pub const DIFF_PANEL_MAX_H_PX: f32 = 420.0;
pub const DIFF_PANEL_MARGIN_PX: f32 = 12.0;
pub const DIFF_PANEL_PAD_PX: f32 = 10.0;
pub const DIFF_ITEM_H_PX: f32 = 26.0;
pub const DIFF_MARK_W_PX: f32 = 14.0;
/// Zoom mínimo ao centralizar um item do resumo (abaixo disso, vai a 100%).
pub const DIFF_FOCUS_MIN_ZOOM: f32 = 0.85;
/// IDs de UI do modo Comparar.
pub const ID_DIFF_PANEL: &str = "canvas-diff-panel";
pub const ID_DIFF_LIST: &str = "canvas-diff-list";
pub const ID_DIFF_EXIT: &str = "canvas-diff-exit";
pub const ID_DIFF_ITEM_PREFIX: &str = "canvas-diff-item";
pub const ID_DIFF_GHOST_PREFIX: &str = "canvas-diff-ghost";
/// Botão do modo Comparar na ilha de ferramentas do canvas.
pub const ID_TOOL_DIFF: &str = "tool-diff";
/// Deslocamento vertical da ilha de ferramentas a partir do meio do canvas (px de tela; ≈ metade
/// da altura da ilha, para centralizá-la).
pub const TOOL_ISLAND_OFFSET_Y_PX: f32 = -146.0;
/// Opacidade dos cartões fantasma no minimapa.
pub const DIFF_MINIMAP_GHOST_ALPHA: f32 = 0.5;
