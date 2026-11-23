//! Constantes compartilhadas do aplicativo Desktop (limites de sessão, geometria padrão e IDs de UI).

/// Limite máximo de viewports por arquivo mantidos em `session.json`.
pub const MAX_VIEWPORTS: usize = 64;

/// Posição padrão de fallback quando uma tabela não está no mapa de posições do canvas.
pub const DEFAULT_CARD_POS: (f32, f32) = (0.0, 0.0);

/// Rótulo e IDs dos controles de SSL / TLS no modal "Conectar ao banco".
pub const TLS_FIELD_LABEL: &str = "SSL / TLS";
pub const ID_TLS_TOGGLE: &str = "db-tls-toggle";
pub const ID_TLS_VERIFY_CERT: &str = "db-tls-verify-cert";

/// Dimensões do controle de SSL / TLS no formulário de conexão (em pixels).
pub const TLS_ROW_GAP_PX: f32 = 12.0;
pub const TLS_CTRL_GAP_PX: f32 = 14.0;
pub const TLS_BTN_HEIGHT_PX: f32 = 28.0;
pub const TLS_LABEL_WIDTH_PX: f32 = 96.0;
pub const TLS_LABEL_FONT_PX: f32 = 12.5;
pub const TLS_SWITCH_W_PX: f32 = 34.0;
pub const TLS_SWITCH_H_PX: f32 = 18.0;
pub const TLS_SWITCH_THUMB_PX: f32 = 14.0;
pub const TLS_SWITCH_PAD_PX: f32 = 2.0;
pub const TLS_SWITCH_RADIUS_PX: f32 = 9.0;

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
pub const ID_DB_ERROR_DETAILS_TOGGLE: &str = "db-error-details-toggle";
pub const ID_DB_ERROR_DETAILS: &str = "db-error-details";
pub const ID_DB_USE_TLS_REQUIRE: &str = "db-use-tls-require";
pub const ID_DB_WARNINGS: &str = "db-warn";
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
pub const DB_FIELD_LABEL2_W_PX: f32 = 62.0;
pub const DB_FIELD_LABEL_FONT_PX: f32 = 12.5;
pub const DB_FIELD_H_PX: f32 = 32.0;
pub const DB_FIELD_PAD_X_PX: f32 = 10.0;
pub const DB_FIELD_RADIUS_PX: f32 = 7.0;
pub const DB_PORT_FIELD_W_PX: f32 = 88.0;
pub const DB_CONN_FONT_PX: f32 = 12.0;
pub const DB_ERROR_FONT_PX: f32 = 12.0;
pub const DB_ENGINE_TRIGGER_W_PX: f32 = 200.0;
pub const DB_ENGINE_MENU_W_PX: f32 = 200.0;
pub const DB_STATUS_MAX_H_PX: f32 = 96.0;
pub const DB_STATUS_MARGIN_TOP_PX: f32 = 4.0;
pub const DB_STATUS_PAD_X_PX: f32 = 12.0;
pub const DB_STATUS_PAD_Y_PX: f32 = 10.0;
pub const DB_STATUS_RADIUS_PX: f32 = 8.0;
pub const DB_STATUS_ICON_PX: f32 = 14.0;
pub const DB_ERROR_DETAILS_MAX_H_PX: f32 = 88.0;
pub const DB_ERROR_DETAILS_PAD_X_PX: f32 = 10.0;
pub const DB_ERROR_DETAILS_PAD_Y_PX: f32 = 8.0;
pub const DB_ERROR_DETAILS_FONT_PX: f32 = 11.5;
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

// ---------------------------------------------------------------------------------------------
// Auto-update (ticket 04 do spec `.scratch/auto-update-desktop`; só o App Desktop).
// ---------------------------------------------------------------------------------------------

/// Chave da preferência `auto_update` em `settings.txt` (booleana, padrão ligada).
pub const SETTING_AUTO_UPDATE: &str = "auto_update";

/// Página das Releases (link mostrado quando o auto-update está desativado ou falhou).
pub const RELEASES_URL: &str = "https://github.com/dbmodeling/dbml/releases/latest";

/// Argumento interno: `db-modeling --relaunch-after <pid>` espera o PID terminar e reabre o app.
pub const ARG_RELAUNCH_AFTER: &str = "--relaunch-after";
/// Intervalo entre as conferências de "o PID antigo ainda existe?" no processo de relançamento.
#[cfg(unix)]
pub const RELAUNCH_POLL_INTERVAL_MS: u64 = 100;
/// Quanto o processo de relançamento espera o app antigo sair antes de desistir (o app antigo
/// travado não deve ganhar uma segunda janela).
#[cfg(unix)]
pub const RELAUNCH_WAIT_TIMEOUT_SECS: u64 = 30;
/// Sufixo que o Linux acrescenta a `current_exe()` quando o binário foi substituído (rsync).
#[cfg(any(unix, test))]
pub const DELETED_EXE_SUFFIX: &str = " (deleted)";

/// Linux (ticket 07): pasta do app dentro do tarball e do prefixo do usuário (`db-modeling.app`).
#[cfg(target_os = "linux")]
pub const LINUX_APP_DIR_NAME: &str = "db-modeling.app";
/// Linux: subpasta do executável dentro da pasta do app.
#[cfg(target_os = "linux")]
pub const LINUX_BIN_DIR_NAME: &str = "bin";
/// Linux: nome do executável dentro de `bin/`.
#[cfg(target_os = "linux")]
pub const LINUX_EXE_NAME: &str = "db-modeling";
/// Linux: subpasta (dentro do diretório privado de updates) onde o tarball é extraído.
#[cfg(target_os = "linux")]
pub const LINUX_EXTRACT_DIR_NAME: &str = "linux-extract";
/// Linux: programas externos (resolvidos pelo `PATH`; nunca root, `apt`, `dnf` ou `pkexec`).
#[cfg(target_os = "linux")]
pub const LINUX_TAR_BIN: &str = "tar";
#[cfg(target_os = "linux")]
pub const LINUX_RSYNC_BIN: &str = "rsync";
/// Linux: argumentos do `tar` (`-xzf <tarball> -C <destino> --no-same-owner`).
#[cfg(target_os = "linux")]
pub const LINUX_TAR_EXTRACT_FLAGS: &str = "-xzf";
#[cfg(target_os = "linux")]
pub const LINUX_TAR_DEST_FLAG: &str = "-C";
#[cfg(target_os = "linux")]
pub const LINUX_TAR_NO_SAME_OWNER_FLAG: &str = "--no-same-owner";
/// Linux: argumentos do `rsync` (`-av --no-links --delete <app> <prefixo>/`).
#[cfg(target_os = "linux")]
pub const LINUX_RSYNC_ARCHIVE_FLAGS: &str = "-av";
#[cfg(target_os = "linux")]
pub const LINUX_RSYNC_NO_LINKS_FLAG: &str = "--no-links";
#[cfg(target_os = "linux")]
pub const LINUX_RSYNC_DELETE_FLAG: &str = "--delete";
/// Linux: prefixos de sistema onde o updater nunca instala (só o gerenciador de pacotes).
#[cfg(target_os = "linux")]
pub const LINUX_SYSTEM_PREFIXES: &[&str] = &[
    "/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/var", "/boot", "/snap",
];
/// Linux: máximo de caracteres do `stderr` do `tar`/`rsync` copiados para o log.
#[cfg(target_os = "linux")]
pub const LINUX_STDERR_MAX_CHARS: usize = 400;
/// Linux: dica quando o `rsync`/`tar` não existe (só texto; o app nunca instala nada).
#[cfg(target_os = "linux")]
pub const LINUX_MISSING_TOOL_HINT: &str =
    "instale com o gerenciador de pacotes (ex.: sudo apt install rsync, sudo dnf install rsync)";
/// Linux: chaves `i18n` do motivo de não atualizar quando o executável não está no layout do
/// tarball ou está num prefixo de sistema.
#[cfg(any(target_os = "linux", test))]
pub const KEY_LINUX_NOT_TARBALL: &str =
    "Só a instalação pelo install.sh se atualiza sozinha; atualize manualmente";
#[cfg(target_os = "linux")]
pub const KEY_LINUX_SYSTEM_PREFIX: &str =
    "Instalado numa pasta do sistema; atualize pelo gerenciador de pacotes ou pelo install.sh";
/// Diretório de processos do Linux (existência de `/proc/<pid>` = processo vivo).
#[cfg(target_os = "linux")]
pub const PROC_DIR: &str = "/proc";
/// Arquivo de estado de um processo em `/proc/<pid>/stat`.
#[cfg(target_os = "linux")]
pub const PROC_STAT_FILE: &str = "stat";
/// Marcador de processo zumbi em `/proc/<pid>/stat`.
#[cfg(any(target_os = "linux", test))]
pub const PROC_STATE_ZOMBIE: &str = "Z";
/// Comando que testa se um PID existe (`kill -0 <pid>`), usado no macOS (sem `/proc`).
#[cfg(target_os = "macos")]
pub const KILL_BIN: &str = "kill";
/// Argumento de `kill` que só testa a existência do processo.
#[cfg(target_os = "macos")]
pub const KILL_PROBE_SIGNAL: &str = "-0";
/// Instalador do macOS (`install_macos.rs`, ticket 06): comandos, argumentos e marcadores da saída
/// de `hdiutil`/`codesign`. Compilado no macOS e nos testes (a lógica é pura e roda no Linux).
#[cfg(any(target_os = "macos", test))]
pub mod macos_update {
    /// `hdiutil attach -nobrowse -readonly -plist -mountroot <tmp> <dmg>`.
    pub const HDIUTIL_BIN: &str = "hdiutil";
    pub const HDIUTIL_ATTACH: &str = "attach";
    pub const HDIUTIL_DETACH: &str = "detach";
    pub const ARG_NOBROWSE: &str = "-nobrowse";
    pub const ARG_READONLY: &str = "-readonly";
    pub const ARG_PLIST: &str = "-plist";
    pub const ARG_MOUNTROOT: &str = "-mountroot";
    pub const ARG_FORCE: &str = "-force";
    /// `codesign --verify --deep --strict <app>` e `codesign -d --verbose=4 <app>`.
    pub const CODESIGN_BIN: &str = "codesign";
    pub const CODESIGN_VERIFY_ARGS: [&str; 3] = ["--verify", "--deep", "--strict"];
    pub const CODESIGN_IDENTITY_ARGS: [&str; 2] = ["-d", "--verbose=4"];
    /// Linha da saída de `codesign -d` com o Team ID, e o valor de um build sem Team ID.
    pub const TEAM_ID_PREFIX: &str = "TeamIdentifier=";
    pub const TEAM_ID_NOT_SET: &str = "not set";
    /// `rsync -av --delete --exclude "Icon?" <origem>/ <destino>/`.
    pub const RSYNC_BIN: &str = "rsync";
    pub const RSYNC_ARGS: [&str; 3] = ["-av", "--delete", "--exclude"];
    pub const RSYNC_EXCLUDE_ICON: &str = "Icon?";
    /// A barra final faz o `rsync` copiar o conteúdo do bundle, não o bundle dentro do bundle.
    pub const DIR_TRAILING_SLASH: &str = "/";
    /// Nome do bundle dentro do `.dmg` (`scripts/build-macos.sh`) e volume padrão do `.dmg`.
    pub const BUNDLE_NAME: &str = "DB Modeling.app";
    pub const VOLUME_NAME: &str = "DB Modeling";
    /// Pasta privada de montagem em `std::env::temp_dir()` (sufixo: PID do app).
    pub const MOUNT_ROOT_PREFIX: &str = "db-modeling-update-";
    /// Chaves e marcadores do plist XML devolvido por `hdiutil attach -plist`.
    pub const PLIST_KEY_MOUNT_POINT: &str = "mount-point";
    pub const PLIST_KEY_DEV_ENTRY: &str = "dev-entry";
    pub const PLIST_STRING_OPEN: &str = "<string>";
    pub const PLIST_STRING_CLOSE: &str = "</string>";
    /// Tamanho máximo do texto de `stderr` copiado para a mensagem de erro (caracteres).
    pub const STDERR_MAX_CHARS: usize = 400;
}
/// Uso correto do argumento interno (mensagem de `--relaunch-after` sem PID válido).
pub const RELAUNCH_USAGE: &str = "Uso interno: db-modeling --relaunch-after <pid>";
/// Códigos de saída do processo curto de relançamento.
pub const EXIT_CODE_RELAUNCH_FAILED: i32 = 1;
pub const EXIT_CODE_USAGE: i32 = 2;
/// Caminho manual quando o relançamento falha: a versão nova já está instalada (log e doc).
pub const RELAUNCH_MANUAL_HINT: &str =
    "A versão nova já está instalada. Abra o DB Modeling novamente pelo menu de aplicativos.";

/// Cache do usuário onde o updater baixa (`<cache>/db-modeling/updates/`).
pub const ENV_LOCALAPPDATA: &str = "LOCALAPPDATA";
pub const ENV_XDG_CACHE_HOME: &str = "XDG_CACHE_HOME";
pub const ENV_HOME: &str = "HOME";
pub const MACOS_CACHES_SUBDIR: &str = "Library/Caches";
pub const LINUX_CACHE_SUBDIR: &str = ".cache";

/// Arquivo (na pasta de configuração) que liga a instalação ao aviso "Atualizado para X" do
/// primeiro start depois do reinício: `versão\nurl-das-notas\n`.
pub const UPDATE_NOTICE_FILE: &str = "update-notice.txt";
/// Limite de tamanho do arquivo de aviso (arquivo editado à mão/corrompido é ignorado).
pub const UPDATE_NOTICE_MAX_BYTES: u64 = 2048;

/// Nome da thread do updater (aparece em depuradores e em `ps -L`).
pub const UPDATER_THREAD_NAME: &str = "dbm-updater";
/// Cadência do ticker que avisa a UI das mudanças de estado durante uma rodada (ms). Só roda
/// enquanto há rodada em andamento; fora dela a UI só acorda por evento.
pub const UPDATE_TICK_MS: u64 = 200;
/// Salto entre relógio de parede e relógio monotônico que indica suspensão do sistema (s).
pub const SUSPEND_GAP_SECS: u64 = 30;

/// IDs de UI do controle de atualização (title bar) e da seção em Aparência.
pub const ID_UPDATE_CONTROL: &str = "update-control";
pub const ID_PREF_AUTO_UPDATE: &str = "pref-auto-update";
pub const ID_PREF_UPDATE_RELEASES: &str = "pref-update-releases";
/// Geometria do controle de atualização na title bar (px).
pub const UPDATE_CONTROL_H_PX: f32 = 28.0;
pub const UPDATE_CONTROL_PAD_X_PX: f32 = 10.0;
pub const UPDATE_CONTROL_GAP_PX: f32 = 6.0;
pub const UPDATE_CONTROL_RADIUS_PX: f32 = 8.0;
pub const UPDATE_CONTROL_MAX_W_PX: f32 = 340.0;
pub const UPDATE_CONTROL_FONT_PX: f32 = 12.0;
pub const UPDATE_CONTROL_ICON_PX: f32 = 13.0;
/// Texto de desativação / link da seção "Atualizações" do popover Aparência (px).
pub const UPDATE_PREF_SECTION_GAP_PX: f32 = 2.0;
pub const UPDATE_PREF_SECTION_PT_PX: f32 = 10.0;
pub const UPDATE_PREF_SECTION_PB_PX: f32 = 4.0;
pub const UPDATE_PREF_HEADER_PAD_X_PX: f32 = 8.0;
pub const UPDATE_PREF_HEADER_PB_PX: f32 = 4.0;
pub const UPDATE_PREF_NOTE_FONT_PX: f32 = 11.5;
pub const UPDATE_PREF_NOTE_PAD_X_PX: f32 = 8.0;
pub const UPDATE_PREF_NOTE_GAP_PX: f32 = 4.0;
