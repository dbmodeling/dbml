import { MAX_SESSION_SELECTION } from './constants.js';

/** Estado de sessão do canvas (zoom, pan, painel...) guardado em `workspaceState` por documento. */
export type SessionState = Record<string, unknown>;

const MAX_BYTES = 16 * 1024;
const PANES = ['files', 'schema', 'problems'];
const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

/**
 * Valida a sessão que a Webview pede para persistir e que será devolvida a ela ao reabrir.
 * Campos conhecidos inválidos são descartados; campos extras (ex.: seleção) passam, desde que caibam no limite.
 */
export function sanitizeSession(raw: unknown): SessionState | undefined {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return undefined;
  let json: string;
  try {
    json = JSON.stringify(raw);
  } catch {
    return undefined;
  }
  if (json.length > MAX_BYTES) return undefined;

  const out = JSON.parse(json) as SessionState;
  if ('pane' in out && !(typeof out.pane === 'string' && PANES.includes(out.pane))) delete out.pane;
  if ('editorOpen' in out && typeof out.editorOpen !== 'boolean') delete out.editorOpen;
  if ('zoom' in out && !(isNum(out.zoom) && out.zoom > 0 && out.zoom <= 100)) delete out.zoom;
  if ('pan' in out) {
    const pan = out.pan as { x?: unknown; y?: unknown } | null;
    if (!(pan && typeof pan === 'object' && isNum(pan.x) && isNum(pan.y))) delete out.pan;
  }
  if ('selection' in out) {
    if (!Array.isArray(out.selection) || out.selection.length > MAX_SESSION_SELECTION || !out.selection.every((s) => typeof s === 'string')) {
      delete out.selection;
    }
  }
  if ('selectedEnum' in out && out.selectedEnum !== null && typeof out.selectedEnum !== 'string') {
    delete out.selectedEnum;
  }
  return out;
}
