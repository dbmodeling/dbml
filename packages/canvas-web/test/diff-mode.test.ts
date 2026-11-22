// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { catalog } from '../src/actions';
import { DIFF_ACT, GIT_BASE_ERROR } from '../src/constants';
import { setLanguage } from '../src/i18n';
import type { Column, DiffResult, GitBaseResult, WasmApi } from '../src/types';
import { fakeTransport, fakeWasm, frame, installDomStubs, mount, type FakeTransport } from './helpers/harness';

const SOURCE = 'Table users { id int [pk]\n email varchar }';
const BASE = 'Table users { id int [pk] }\nTable old { id int }';
const col = (name: string, ty = 'int'): Column => ({ name, ty, pk: false, not_null: false, unique: false }) as Column;

const DIFF: DiffResult = {
  summary: { added: 1, removed: 1, modified: 0 },
  diff: {
    tables: [
      { kind: 'modified', value: { name: 'users', props: null, columns: [{ kind: 'added', value: col('email', 'varchar') }], indexes: [] } },
      { kind: 'removed', value: { name: 'old', columns: [col('id')], indexes: [] } as never },
    ],
    enums: [],
    refs: [],
  },
};
const EMPTY: DiffResult = { summary: { added: 0, removed: 0, modified: 0 }, diff: { tables: [], enums: [], refs: [] } };

function setup(opts: { base?: GitBaseResult; diff?: DiffResult | Error } = {}) {
  const base: GitBaseResult = opts.base ?? { ok: true, source: BASE };
  const requestGitBase = vi.fn(async () => base);
  const transport: FakeTransport = Object.assign(fakeTransport(), { requestGitBase });
  const diff = opts.diff ?? DIFF;
  const wasm = {
    ...fakeWasm(),
    diff_dbml: vi.fn(() => {
      if (diff instanceof Error) throw diff;
      return diff;
    }),
  } as unknown as WasmApi;
  const mounted = mount(transport, wasm);
  transport.update(SOURCE, undefined, { saved: true, documentId: '/ws/schema.dbml' });
  return { ...mounted, transport, requestGitBase, wasm };
}

const press = (key: string, init: KeyboardEventInit = {}) =>
  window.dispatchEvent(new KeyboardEvent('keydown', { key, code: `Key${key.toUpperCase()}`, ...init }));
/** "Reduzir movimento" ligado: a câmera salta direto ao destino (o relógio do rAF do jsdom não anima o deslize). */
const reduceMotion = () => vi.stubGlobal('matchMedia', () => ({ matches: true }));
const settle = () => frame();
const panel = (root: HTMLElement) => root.querySelector('#sidebar') as HTMLElement;

beforeEach(() => {
  installDomStubs();
  document.body.innerHTML = '';
  localStorage.setItem('dbmodeling.prefs', JSON.stringify({ lang: 'pt-BR' }));
});
afterEach(() => {
  setLanguage('pt-BR');
  vi.unstubAllGlobals();
});

describe('modo Comparar no workbench', () => {
  it('está no catálogo (paleta de comandos) com atalho de teclado', () => {
    const entry = catalog().find((a) => a.id === 'ToggleDiff');
    expect(entry).toMatchObject({ key: 'd', label: 'Comparar com o git HEAD' });
    expect(catalog().filter((a) => a.key === 'd')).toHaveLength(1);
  });

  it('atalho abre o resumo, colore cartões e linhas e mostra o cartão fantasma removido', async () => {
    const { root, requestGitBase, wasm } = setup();
    await frame();
    expect(root.querySelectorAll('.card.diff-ghost')).toHaveLength(0);

    press('d');
    await frame();

    expect(requestGitBase).toHaveBeenCalledWith('/ws/schema.dbml');
    expect(wasm.diff_dbml).toHaveBeenCalledWith(BASE, SOURCE);
    const sidebar = panel(root);
    expect(sidebar.textContent).toContain('COLUNAS');
    expect(sidebar.querySelectorAll(`[data-act="${DIFF_ACT.FOCUS}"]`).length).toBeGreaterThan(0);

    const users = root.querySelector('.card[data-table="users"]') as HTMLElement;
    expect(users.classList.contains('diff-modified')).toBe(true);
    expect(users.querySelector('.row.diff-added[data-col="email"]')).not.toBeNull();
    const ghost = root.querySelector('.card.diff-ghost[data-diff-ghost="old"]') as HTMLElement;
    expect(ghost).not.toBeNull();
    expect(ghost.hasAttribute('data-table')).toBe(false);
  });

  it('é somente leitura: nenhum patch, texto intacto e histórico de desfazer inalterado', async () => {
    const { root, transport, workbench } = setup();
    press('d');
    await frame();
    press('d');
    await frame();
    expect(transport.patches).toHaveLength(0);
    expect((workbench as unknown as { source: string }).source).toBe(SOURCE);
    expect((workbench as unknown as { undoStack: unknown[] }).undoStack).toHaveLength(0);
    expect(root.querySelectorAll('.diff-ghost, .diff-added, .diff-modified')).toHaveLength(0);
  });

  it('o mesmo atalho sai do modo e restaura o painel anterior', async () => {
    const { root } = setup();
    await frame();
    const before = panel(root).textContent;
    press('d');
    await frame();
    expect(panel(root).textContent).not.toBe(before);
    press('d');
    await frame();
    expect(panel(root).textContent).toBe(before);
  });

  it('clicar num item do resumo move a câmera até ele e seleciona a tabela', async () => {
    reduceMotion();
    const { root, workbench } = setup();
    press('d');
    await frame();
    const wb = workbench as unknown as { pan: { x: number; y: number }; selection: string[] };
    wb.pan = { x: 5000, y: 5000 };
    const row = panel(root).querySelector(`[data-diff-id="columns:added:users.email"]`) as HTMLElement;
    row.click();
    await settle();
    expect(wb.selection).toEqual(['users']);
    expect(wb.pan.x).not.toBe(5000);
  });

  it('item removido (fantasma) também recebe foco, sem selecionar tabela do head', async () => {
    reduceMotion();
    const { root, workbench } = setup();
    press('d');
    await frame();
    const wb = workbench as unknown as { pan: { x: number; y: number }; selection: string[] };
    wb.pan = { x: 5000, y: 5000 };
    (panel(root).querySelector(`[data-diff-id="tables:removed:old"]`) as HTMLElement).click();
    await settle();
    expect(wb.selection).toEqual([]);
    expect(wb.pan.x).not.toBe(5000);
  });

  it('sem diferenças: mensagem de estado vazio', async () => {
    const { root } = setup({ diff: EMPTY });
    press('d');
    await frame();
    expect(panel(root).textContent).toContain('Sem diferenças em relação ao git HEAD.');
    expect(root.querySelectorAll('.diff-ghost')).toHaveLength(0);
  });

  it('arquivo novo (não está no HEAD): mensagem de erro clara, sem cores, com tentar de novo', async () => {
    const { root, requestGitBase } = setup({ base: { ok: false, code: GIT_BASE_ERROR.UNTRACKED, message: 'new' } });
    press('d');
    await frame();
    expect(panel(root).textContent).toContain('Arquivo novo');
    expect(root.querySelectorAll('.diff-ghost, .diff-modified, .diff-added')).toHaveLength(0);
    requestGitBase.mockClear();
    (panel(root).querySelector(`[data-act="${DIFF_ACT.RETRY}"]`) as HTMLElement).click();
    await frame();
    expect(requestGitBase).toHaveBeenCalledTimes(1);
  });

  it('sem git/repositório: mensagem traduzida no idioma ativo', async () => {
    localStorage.setItem('dbmodeling.prefs', JSON.stringify({ lang: 'en' }));
    const { root } = setup({ base: { ok: false, code: GIT_BASE_ERROR.NOT_A_REPO, message: 'x' } });
    press('d');
    await frame();
    expect(panel(root).textContent).toContain('not in a git repository');
  });

  it('editar o texto com o modo ligado recalcula o diff contra a mesma base, sem nova consulta ao git', async () => {
    const { transport, requestGitBase, wasm } = setup();
    press('d');
    await frame();
    requestGitBase.mockClear();
    transport.update('Table users { id int [pk]\n email varchar\n age int }', undefined, { saved: true, documentId: '/ws/schema.dbml' });
    await frame();
    expect(requestGitBase).not.toHaveBeenCalled();
    expect(wasm.diff_dbml).toHaveBeenLastCalledWith(BASE, 'Table users { id int [pk]\n email varchar\n age int }');
  });

  it('Esc sem seleção sai do modo', async () => {
    const { root } = setup();
    press('d');
    await frame();
    press('Escape', { code: 'Escape' });
    await frame();
    expect(root.querySelectorAll('.diff-ghost')).toHaveLength(0);
  });
});
