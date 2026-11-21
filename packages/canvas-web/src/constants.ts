/**
 * Constantes compartilhadas do canvas-web (sem strings/números mágicos em TS ou HTML).
 */

import type { TlsMode } from './types';

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

export const CONNECT_NETWORK_ROW_IDS = [
  'host',
  'port',
  'database',
  'user',
  'password',
  'schemas',
  TLS_ROW_ID,
] as const;

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
} as const;

export const TRANSPORT_ERROR = {
  CONNECTION_CLOSED: 'Connection closed',
  OFFLINE_CONFLICT: 'Document changed while offline; reconcile or export your changes',
} as const;
