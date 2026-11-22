import { describe, expect, it, vi } from 'vitest';
import { DIFF_ERROR, GIT_BASE_ERROR } from '../src/constants';
import {
  DIFF_OFF,
  diffErrorMessageKey,
  diffFromBase,
  loadDiff,
  refreshDiff,
  type DiffState,
} from '../src/diff/state';
import type { Column, DiffResult, GitBaseResult, HostTransport, WasmApi } from '../src/types';

const col = (name: string): Column => ({ name, ty: 'int', pk: false, not_null: false, unique: false } as Column);

const addedColumn: DiffResult = {
  summary: { added: 1, removed: 0, modified: 0 },
  diff: {
    tables: [{ kind: 'modified', value: { name: 'users', props: null, columns: [{ kind: 'added', value: col('email') }], indexes: [] } }],
    enums: [],
    refs: [],
  },
};
const empty: DiffResult = { summary: { added: 0, removed: 0, modified: 0 }, diff: { tables: [], enums: [], refs: [] } };

const wasmReturning = (result: DiffResult) => ({ diff_dbml: vi.fn(() => result) }) as unknown as WasmApi;
const wasmThrowing = (error: unknown) =>
  ({ diff_dbml: vi.fn(() => { throw error; }) }) as unknown as WasmApi;
const transportWith = (result: GitBaseResult | Error) =>
  ({
    requestGitBase: vi.fn(async () => {
      if (result instanceof Error) throw result;
      return result;
    }),
  }) as unknown as HostTransport;

describe('diffFromBase', () => {
  it('devolve o modelo pronto com o diff do Wasm', () => {
    const wasm = wasmReturning(addedColumn);
    const state = diffFromBase(wasm, 'base', 'head');
    expect(wasm.diff_dbml).toHaveBeenCalledWith('base', 'head');
    expect(state.status).toBe('ready');
    if (state.status !== 'ready') return;
    expect(state.base).toBe('base');
    expect(state.view.summary.added).toBe(1);
    expect(state.view.tables.get('users')?.columns.get('email')?.status).toBe('added');
  });

  it('Wasm sem diff_dbml (host antigo) é "unsupported"', () => {
    expect(diffFromBase({} as WasmApi, 'a', 'b')).toEqual({ status: 'error', code: DIFF_ERROR.UNSUPPORTED });
    expect(diffFromBase(undefined, 'a', 'b')).toEqual({ status: 'error', code: DIFF_ERROR.UNSUPPORTED });
  });

  it('erro do Wasm no lado base/head vira código com o detalhe da primeira falha', () => {
    const base = diffFromBase(wasmThrowing('base has DBML errors: line 3: unexpected token'), 'a', 'b');
    expect(base).toEqual({ status: 'error', code: DIFF_ERROR.BASE_INVALID, detail: 'line 3: unexpected token' });
    const head = diffFromBase(wasmThrowing(new Error('head has DBML errors: line 1: x')), 'a', 'b');
    expect(head).toEqual({ status: 'error', code: DIFF_ERROR.HEAD_INVALID, detail: 'line 1: x' });
  });

  it('outro erro do Wasm vira "other" com a mensagem', () => {
    expect(diffFromBase(wasmThrowing('boom'), 'a', 'b')).toEqual({ status: 'error', code: GIT_BASE_ERROR.OTHER, detail: 'boom' });
  });
});

describe('loadDiff', () => {
  it('busca a base no transporte e calcula o diff contra o texto atual', async () => {
    const transport = transportWith({ ok: true, source: 'BASE' });
    const wasm = wasmReturning(empty);
    const state = await loadDiff(transport, wasm, 'HEAD', '/ws/a.dbml');
    expect(transport.requestGitBase).toHaveBeenCalledWith('/ws/a.dbml');
    expect(wasm.diff_dbml).toHaveBeenCalledWith('BASE', 'HEAD');
    expect(state.status).toBe('ready');
  });

  it('falha esperada do git vira erro com o mesmo código (arquivo novo, sem repo)', async () => {
    const state = await loadDiff(
      transportWith({ ok: false, code: GIT_BASE_ERROR.UNTRACKED, message: 'new file' }),
      wasmReturning(empty),
      'HEAD'
    );
    expect(state).toEqual({ status: 'error', code: GIT_BASE_ERROR.UNTRACKED, detail: 'new file' });
  });

  it('transporte sem requestGitBase é "unsupported"', async () => {
    const state = await loadDiff({} as HostTransport, wasmReturning(empty), 'HEAD');
    expect(state).toEqual({ status: 'error', code: DIFF_ERROR.UNSUPPORTED });
  });

  it('erro de canal vira "other" com a mensagem', async () => {
    const state = await loadDiff(transportWith(new Error('Disconnected')), wasmReturning(empty), 'HEAD');
    expect(state).toEqual({ status: 'error', code: GIT_BASE_ERROR.OTHER, detail: 'Disconnected' });
  });
});

describe('refreshDiff', () => {
  const ready = diffFromBase(wasmReturning(addedColumn), 'base', 'head1');

  it('recalcula contra a base já guardada, sem consultar o git', () => {
    const wasm = wasmReturning(empty);
    const next = refreshDiff(ready, wasm, 'head2');
    expect(wasm.diff_dbml).toHaveBeenCalledWith('base', 'head2');
    expect(next.status === 'ready' && next.view.items).toEqual([]);
  });

  it('head inválido durante a digitação mantém o último resultado pronto', () => {
    const next = refreshDiff(ready, wasmThrowing('head has DBML errors: line 2: x'), 'head quebrado');
    expect(next).toBe(ready);
  });

  it('só recalcula quando há resultado pronto', () => {
    const wasm = wasmReturning(empty);
    for (const state of [DIFF_OFF, { status: 'loading' }, { status: 'error', code: GIT_BASE_ERROR.OTHER }] as DiffState[]) {
      expect(refreshDiff(state, wasm, 'h')).toBe(state);
    }
    expect(wasm.diff_dbml).not.toHaveBeenCalled();
  });
});

describe('diffErrorMessageKey', () => {
  it('todo código tem uma mensagem (chave pt-BR de i18n)', () => {
    const codes = [...Object.values(GIT_BASE_ERROR), ...Object.values(DIFF_ERROR)];
    const keys = codes.map((c) => diffErrorMessageKey(c));
    expect(new Set(keys).size).toBe(codes.length);
    for (const k of keys) expect(k.length).toBeGreaterThan(10);
  });
});
