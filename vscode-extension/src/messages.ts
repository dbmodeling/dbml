/**
 * Validação das mensagens vindas da Webview. A Webview é código que roda em um contexto
 * menos confiável que o host da extensão: nada é usado antes de checar tipo e formato.
 */
import { TLS_MODES } from './constants.js';
import type { IntrospectRequest } from './introspect.js';

export interface Position {
  line: number;
  character: number;
}
export interface Range {
  start: Position;
  end: Position;
}
export interface PatchMessage {
  range: Range;
  new_text: string;
  expected_source: string;
}
export interface DiagnosticMessage {
  severity: string;
  message: string;
  range: Range;
}

export type WebviewMessage =
  | { command: 'ready' | 'new_file' | 'check_cli'; id?: number }
  | { command: 'apply_patch'; id?: number; base_version: number; patch: PatchMessage }
  | { command: 'save_document'; id?: number; base_version: number; expected_source: string }
  | { command: 'save_layout'; id?: number; layout: Record<string, unknown> }
  | { command: 'diagnostics'; id?: number; diagnostics: DiagnosticMessage[] }
  | { command: 'export_file'; id?: number; filename: string; content: string; isBinary: boolean }
  | { command: 'test_connection' | 'introspect'; id?: number; request: IntrospectRequest }
  | { command: 'open_file'; id?: number; path: string }
  | { command: 'save_session'; id?: number; session: Record<string, unknown> };

export type ParseResult =
  | { ok: true; message: WebviewMessage }
  | { ok: false; reason: string; command?: string; id?: number };

const MAX_DIAGNOSTICS = 10_000;
const MAX_FILENAME = 255;
const DIALECTS = ['postgres', 'mysql', 'sqlserver', 'sqlite'];

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
const isString = (v: unknown): v is string => typeof v === 'string';
const isIndex = (v: unknown): v is number => typeof v === 'number' && Number.isInteger(v) && v >= 0;

function isPosition(v: unknown): v is Position {
  return isRecord(v) && isIndex(v.line) && isIndex(v.character);
}
function isRange(v: unknown): v is Range {
  return isRecord(v) && isPosition(v.start) && isPosition(v.end);
}
function isPatch(v: unknown): v is PatchMessage {
  return isRecord(v) && isRange(v.range) && isString(v.new_text) && isString(v.expected_source);
}
function isDiagnostic(v: unknown): v is DiagnosticMessage {
  return isRecord(v) && isString(v.severity) && isString(v.message) && isRange(v.range);
}
function isSafeFilename(v: unknown): v is string {
  return isString(v) && v.trim() !== '' && v.length <= MAX_FILENAME && !/[\\/\0]/.test(v) && v !== '.' && v !== '..';
}
function isIntrospectRequest(v: unknown): v is IntrospectRequest {
  return (
    isRecord(v) &&
    isString(v.dialect) &&
    DIALECTS.includes(v.dialect) &&
    isString(v.host) &&
    (v.port === null || (isIndex(v.port) && v.port <= 65535)) &&
    isString(v.database) &&
    isString(v.user) &&
    isString(v.password) &&
    isString(v.path) &&
    typeof v.trust_server_certificate === 'boolean' &&
    (v.tls === undefined || (isString(v.tls) && (TLS_MODES as readonly string[]).includes(v.tls))) &&
    Array.isArray(v.schemas) &&
    v.schemas.every(isString)
  );
}

export function parseWebviewMessage(raw: unknown): ParseResult {
  if (!isRecord(raw)) return { ok: false, reason: 'message is not an object' };
  const command = raw.command;
  if (!isString(command)) return { ok: false, reason: 'missing command' };
  if (raw.id !== undefined && !(typeof raw.id === 'number' && Number.isFinite(raw.id))) {
    return { ok: false, reason: 'invalid id', command };
  }
  const id = raw.id as number | undefined;
  const invalid = (reason: string): ParseResult => ({ ok: false, reason: `${command}: ${reason}`, command, id });

  switch (command) {
    case 'ready':
    case 'new_file':
    case 'check_cli':
      return { ok: true, message: { command, id } };
    case 'apply_patch':
      if (!isIndex(raw.base_version)) return invalid('invalid base_version');
      return isPatch(raw.patch)
        ? { ok: true, message: { command, id, base_version: raw.base_version, patch: raw.patch } }
        : invalid('invalid patch');
    case 'save_document':
      if (!isIndex(raw.base_version)) return invalid('invalid base_version');
      return isString(raw.expected_source)
        ? { ok: true, message: { command, id, base_version: raw.base_version, expected_source: raw.expected_source } }
        : invalid('invalid expected_source');
    case 'save_layout':
      return isRecord(raw.layout) ? { ok: true, message: { command, id, layout: raw.layout } } : invalid('invalid layout');
    case 'diagnostics':
      return Array.isArray(raw.diagnostics) && raw.diagnostics.length <= MAX_DIAGNOSTICS && raw.diagnostics.every(isDiagnostic)
        ? { ok: true, message: { command, id, diagnostics: raw.diagnostics } }
        : invalid('invalid diagnostics');
    case 'export_file':
      if (!isSafeFilename(raw.filename)) return invalid('invalid filename');
      if (!isString(raw.content) || typeof raw.isBinary !== 'boolean') return invalid('invalid content');
      return { ok: true, message: { command, id, filename: raw.filename, content: raw.content, isBinary: raw.isBinary } };
    case 'test_connection':
    case 'introspect':
      return isIntrospectRequest(raw.request)
        ? { ok: true, message: { command, id, request: raw.request } }
        : invalid('invalid request');
    case 'open_file':
      return isString(raw.path) && raw.path !== '' ? { ok: true, message: { command, id, path: raw.path } } : invalid('invalid path');
    case 'save_session':
      return isRecord(raw.session) ? { ok: true, message: { command, id, session: raw.session } } : invalid('invalid session');
    default:
      return { ok: false, reason: `unknown command: ${command}`, command, id };
  }
}
