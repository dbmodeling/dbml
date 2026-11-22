/**
 * Estado do modo Comparar: obtém a base (git `HEAD` do arquivo aberto) pelo transporte, calcula o
 * diff no Wasm e o transforma em `DiffView`. Módulo sem DOM: o Workbench só guarda o `DiffState`
 * e desenha. O modo é somente leitura: nada aqui toca o buffer, o histórico ou o disco.
 */

import { DIFF_ERROR, GIT_BASE_ERROR } from '../constants';
import type { GitBaseErrorCode, HostTransport, WasmApi } from '../types';
import { buildDiffView, type DiffView } from './model';

export type DiffErrorCode = GitBaseErrorCode | (typeof DIFF_ERROR)[keyof typeof DIFF_ERROR];

export type DiffState =
  | { status: 'off' }
  | { status: 'loading' }
  | { status: 'ready'; base: string; view: DiffView }
  | { status: 'error'; code: DiffErrorCode; detail?: string };

export const DIFF_OFF: DiffState = { status: 'off' };

/** Mensagem (chave pt-BR de `i18n`) para cada falha possível do modo Comparar. */
const ERROR_MESSAGE: Record<DiffErrorCode, string> = {
  [GIT_BASE_ERROR.GIT_UNAVAILABLE]: 'Não foi possível comparar: o git não está instalado ou não está no PATH.',
  [GIT_BASE_ERROR.NOT_A_REPO]: 'Não foi possível comparar: o arquivo não está em um repositório git.',
  [GIT_BASE_ERROR.NO_COMMITS]: 'Não foi possível comparar: o repositório ainda não tem commits.',
  [GIT_BASE_ERROR.UNTRACKED]: 'Arquivo novo: ele ainda não existe no git HEAD, então não há versão anterior para comparar.',
  [GIT_BASE_ERROR.NOT_UTF8]: 'Não foi possível comparar: a versão do git HEAD não é texto UTF-8.',
  [GIT_BASE_ERROR.OTHER]: 'Não foi possível obter a versão do git HEAD.',
  [DIFF_ERROR.BASE_INVALID]: 'A versão do git HEAD tem erros de DBML e não pode ser comparada.',
  [DIFF_ERROR.HEAD_INVALID]: 'O documento atual tem erros de DBML. Corrija-os para comparar.',
  [DIFF_ERROR.UNSUPPORTED]: 'Este ambiente não suporta o modo Comparar.',
};

export function diffErrorMessageKey(code: DiffErrorCode): string {
  return ERROR_MESSAGE[code];
}

const errorText = (e: unknown): string => (e instanceof Error ? e.message : String(e));

const INVALID_SIDE = /^(base|head) has DBML errors:\s*(.*)$/s;

/** Calcula o diff `base` → `head` no Wasm; traduz as falhas em `DiffState` de erro. */
export function diffFromBase(wasm: Pick<WasmApi, 'diff_dbml'> | undefined, base: string, head: string): DiffState {
  if (!wasm?.diff_dbml) return { status: 'error', code: DIFF_ERROR.UNSUPPORTED };
  try {
    return { status: 'ready', base, view: buildDiffView(wasm.diff_dbml(base, head)) };
  } catch (e) {
    const message = errorText(e);
    const side = INVALID_SIDE.exec(message);
    if (side) {
      return {
        status: 'error',
        code: side[1] === 'base' ? DIFF_ERROR.BASE_INVALID : DIFF_ERROR.HEAD_INVALID,
        detail: side[2],
      };
    }
    return { status: 'error', code: GIT_BASE_ERROR.OTHER, detail: message };
  }
}

/** Busca a base no host e compara com `head` (o texto atual do documento). */
export async function loadDiff(
  transport: Pick<HostTransport, 'requestGitBase'>,
  wasm: Pick<WasmApi, 'diff_dbml'> | undefined,
  head: string,
  documentId?: string
): Promise<DiffState> {
  if (!transport.requestGitBase) return { status: 'error', code: DIFF_ERROR.UNSUPPORTED };
  try {
    const base = await transport.requestGitBase(documentId);
    if (!base.ok) return { status: 'error', code: base.code, detail: base.message };
    return diffFromBase(wasm, base.source, head);
  } catch (e) {
    return { status: 'error', code: GIT_BASE_ERROR.OTHER, detail: errorText(e) };
  }
}

/**
 * Recalcula um resultado pronto depois de o texto mudar (a base fica guardada, sem novo `git`).
 * Se o texto atual está inválido (digitação em curso) mantém o último resultado, sem piscar.
 */
export function refreshDiff(state: DiffState, wasm: Pick<WasmApi, 'diff_dbml'> | undefined, head: string): DiffState {
  if (state.status !== 'ready') return state;
  const next = diffFromBase(wasm, state.base, head);
  if (next.status === 'error' && next.code === DIFF_ERROR.HEAD_INVALID) return state;
  return next;
}
