/**
 * Constantes compartilhadas da extensão VS Code (sem strings/números mágicos em TS ou HTML).
 */

import type { TlsMode } from './introspect.js';

export const CSP_NONCE_BYTES = 16;
export const HEX_RADIX = 16;
export const HEX_PAD_LEN = 2;
export const HEX_PAD_CHAR = '0';

export const WEBVIEW_HTML_LANG = 'pt-BR';
export const WEBVIEW_TITLE = 'DB Modeling';
export const WEBVIEW_APP_ROOT_ID = 'app';
export const WEBVIEW_BG_COLOR = '#0a0b0c';
export const WEBVIEW_DIST_DIR = 'dist';
export const WEBVIEW_WASM_BUNDLE = 'dbml-wasm.bundle.js';
export const WEBVIEW_CANVAS_BUNDLE = 'canvas-web.js';

export const DELTA_OVERHEAD_BYTES = 24;
export const DELTA_SIZE_RATIO = 0.5;
export const TEXT_CHANGE_DEBOUNCE_MS = 80;
export const MAX_SESSION_SELECTION = 500;
export const MAX_GUIDE_VERSION_LEN = 32;

export const GLOBAL_STATE_KEY = {
  ONBOARDING_DONE: 'dbmodeling.onboardingDone',
  LAST_SEEN_VERSION: 'dbmodeling.lastSeenVersion',
} as const;

export const WEBVIEW_COMMAND = {
  SAVE_GUIDE_STATE: 'save_guide_state',
} as const;

export const WEBVIEW_OUTBOUND_TYPE = {
  UPDATE: 'update',
  DELTA: 'delta',
  RESULT: 'result',
  WORKSPACE_FILES: 'workspace_files',
} as const;

export const TLS_MODE = {
  AUTO: 'auto',
  VERIFY: 'verify',
  REQUIRE: 'require',
  DISABLE: 'disable',
} as const satisfies Record<string, TlsMode>;

export const TLS_MODES: readonly TlsMode[] = [
  TLS_MODE.AUTO,
  TLS_MODE.VERIFY,
  TLS_MODE.REQUIRE,
  TLS_MODE.DISABLE,
];

export const TLS_QUERY_PARAM = {
  PG_VERIFY: 'sslmode=verify-full',
  PG_REQUIRE: 'sslmode=require',
  PG_DISABLE: 'sslmode=disable',
  MYSQL_VERIFY: 'ssl-mode=VERIFY_IDENTITY',
  MYSQL_REQUIRE: 'ssl-mode=REQUIRED',
  MYSQL_DISABLE: 'ssl-mode=DISABLED',
  SQLSERVER_VERIFY: 'encrypt=true',
  SQLSERVER_REQUIRE: 'encrypt=true&trustServerCertificate=true',
  SQLSERVER_DISABLE: 'encrypt=false',
  SQLSERVER_TRUST_CERT: 'trustServerCertificate=true',
} as const;
