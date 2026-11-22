/**
 * Constantes compartilhadas do canvas-web (sem strings/números mágicos em TS ou HTML).
 */

import type { ConnectionUrlErrorCode, TlsMode } from './types';

export const APP_ROOT_ID = 'app';
export const LOCAL_SESSION_PREFIX = 'dbmodeling.session:';
export const MAX_LOCAL_SESSIONS = 50;
export const MAX_WASM_RECOVERY_ATTEMPTS = 3;

export const TLS_MODE = {
  AUTO: 'auto',
  VERIFY: 'verify',
  REQUIRE: 'require',
  DISABLE: 'disable',
} as const satisfies Record<string, TlsMode>;

export const TLS_ROW_ID = 'tls';
export const TLS_SELECT_ID = 'db-tls';
export const TLS_FIELD_LABEL = 'TLS';

export const TLS_SELECT_OPTIONS: ReadonlyArray<{ value: TlsMode; labelKey: string }> = [
  { value: TLS_MODE.AUTO, labelKey: 'Auto (TLS verificado em hosts remotos)' },
  { value: TLS_MODE.VERIFY, labelKey: 'Verificar certificado (TLS)' },
  { value: TLS_MODE.REQUIRE, labelKey: 'Exigir TLS (autoassinado)' },
  { value: TLS_MODE.DISABLE, labelKey: 'Desativar TLS (texto claro)' },
];

/** Linhas (rótulo + controle) que só valem para bancos de rede; Host+Porta e Usuário+Senha dividem uma linha. */
export const CONNECT_NETWORK_ROW_IDS = [
  'host',
  'database',
  'user',
  'schemas',
  TLS_ROW_ID,
] as const;

/** Banco ainda sem introspector: aparece desabilitado no dropdown. */
export const ENGINE_COMING_SOON = 'oracle';
export const ENGINE_COMING_SOON_LABEL = 'Oracle';

/** Marcador que substitui a senha na connection string exibida (mesma convenção de `mask_url`). */
export const PASSWORD_MASK = '***';

/** Espera após a última tecla no campo Connection string antes de reinterpretá-la. */
export const URL_PARSE_DEBOUNCE_MS = 150;
/** O texto colado só chega ao campo depois do evento `paste`; esta espera deixa o navegador inseri-lo. */
export const URL_PASTE_SETTLE_MS = 0;
/** Tempo em que o botão Copiar mostra "Copiado". */
export const COPY_FEEDBACK_MS = 1600;

/** Quem editou por último: decide o sentido da sincronia e evita laço entre campos e string. */
export const EDIT_ORIGIN = {
  FIELD: 'field',
  STRING: 'string',
} as const;

export const CONNECT_URL_ERROR = {
  EMPTY: 'empty',
  UNKNOWN_SCHEME: 'unknown_scheme',
  UNSUPPORTED_ENGINE: 'unsupported_engine',
  MALFORMED: 'malformed',
  MISSING_HOST: 'missing_host',
  INVALID_PORT: 'invalid_port',
} as const satisfies Record<string, ConnectionUrlErrorCode>;

/** Esquema escrito na connection string montada para cada banco (SQLite usa o caminho puro). */
export const ENGINE_SCHEME = {
  postgres: 'postgres',
  mysql: 'mysql',
  sqlserver: 'sqlserver',
} as const;

/**
 * Query de TLS por banco e modo, igual a `ConnectionParams::tls_query` (crates/db-introspect).
 * Modo `auto` não escreve nada, salvo o SQL Server com "confiar no certificado".
 */
export const TLS_QUERY = {
  postgres: { verify: 'sslmode=verify-full', require: 'sslmode=require', disable: 'sslmode=disable' },
  mysql: { verify: 'ssl-mode=VERIFY_IDENTITY', require: 'ssl-mode=REQUIRED', disable: 'ssl-mode=DISABLED' },
  sqlserver: {
    verify: 'encrypt=true',
    require: 'encrypt=true&trustServerCertificate=true',
    disable: 'encrypt=false',
  },
} as const;
export const SQLSERVER_TRUST_QUERY = 'trustServerCertificate=true';

/** IDs e atributos dos controles novos do diálogo "Conectar ao banco". */
export const CONNECT_DIALOG = {
  ENGINE_TRIGGER_ID: 'db-engine',
  ENGINE_LABEL_ID: 'db-engine-label',
  ENGINE_VALUE_ID: 'db-engine-value',
  URL_COPY_LABEL_ID: 'db-url-copy-label',
  ENGINE_LIST_ID: 'db-engine-list',
  ENGINE_OPTION_ID_PREFIX: 'db-engine-opt-',
  URL_ID: 'db-url',
  URL_ERROR_ID: 'db-url-error',
  URL_COPY_ID: 'db-url-copy',
  PASSWORD_ID: 'db-password',
  PASSWORD_TOGGLE_ID: 'db-password-toggle',
  ROW_URL: 'url',
  ROW_ENGINE: 'engine',
} as const;

export const TRANSPORT_EVENT = {
  INIT: 'init',
  UPDATE: 'update',
  DELTA: 'delta',
  RESULT: 'result',
  PATCH_REJECTED: 'patch_rejected',
  EXTERNAL_CHANGE: 'external_change',
  WORKSPACE_FILES: 'workspace_files',
} as const;

export const TRANSPORT_COMMAND = {
  APPLY_PATCH: 'apply_patch',
  SAVE_DOCUMENT: 'save_document',
  SAVE_LAYOUT: 'save_layout',
  SAVE_GUIDE_STATE: 'save_guide_state',
} as const;

export const GUIDE_STORAGE_KEY = {
  ONBOARDING_DONE: 'dbmodeling.onboardingDone',
  LAST_SEEN_VERSION: 'dbmodeling.lastSeenVersion',
} as const;

export const TOUR_GEOM = {
  HOLE_PAD_PX: 6,
  HOLE_MIN_POS_PX: 2,
  CARD_GAP_PX: 14,
  CARD_WIDTH_PX: 320,
  CARD_HEIGHT_EST_PX: 180,
  VIEWPORT_MARGIN_PX: 12,
  ARROW_MIN_OFFSET_PX: 20,
  FALLBACK_VIEWPORT_W_PX: 1024,
  FALLBACK_VIEWPORT_H_PX: 720,
} as const;

export const TOUR_ACTION = {
  SKIP: 'skip',
  PREV: 'prev',
  NEXT: 'next',
} as const;

export const TRANSPORT_ERROR = {
  CONNECTION_CLOSED: 'Connection closed',
  OFFLINE_CONFLICT: 'Document changed while offline; reconcile or export your changes',
} as const;
