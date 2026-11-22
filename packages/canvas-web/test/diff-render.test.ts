import { afterEach, describe, expect, it } from 'vitest';
import { DIFF_ACT, DIFF_ERROR, GIT_BASE_ERROR } from '../src/constants';
import { buildDiffView, DIFF_GROUPS } from '../src/diff/model';
import {
  columnDiffTitle,
  diffStatusClass,
  ghostSlots,
  renderDiffPanel,
  renderGhostEdges,
  renderGhostEnum,
  renderGhostRows,
  renderGhostTable,
} from '../src/diff/render';
import type { DiffState } from '../src/diff/state';
import { removedEdges } from '../src/diff/layout';
import { CARD_W, cardH } from '../src/glass';
import { setLanguage } from '../src/i18n';
import { escHtml } from '../src/panels';
import type { Column, DiffResult, Ref, Table } from '../src/types';

afterEach(() => setLanguage('pt-BR'));

const col = (name: string, ty = 'int'): Column => ({ name, ty, pk: false, not_null: false, unique: false }) as Column;
const table = (name: string, cols: Column[]): Table => ({ name, columns: cols, indexes: [] }) as unknown as Table;
const ref = (from: string, to: string): Ref =>
  ({ from: { table: from, column: 'id' }, to: { table: to, column: 'id' }, kind: 'ManyToOne' }) as unknown as Ref;

const result: DiffResult = {
  summary: { added: 2, removed: 3, modified: 1 },
  diff: {
    tables: [
      { kind: 'added', value: table('audit', [col('id')]) },
      { kind: 'removed', value: table('old<script>', [col('id'), col('x')]) },
      {
        kind: 'modified',
        value: {
          name: 'users',
          props: null,
          columns: [
            { kind: 'added', value: col('email', 'text') },
            { kind: 'modified', value: { before: col('age', 'int'), after: col('age', 'bigint') } },
            { kind: 'removed', value: col('legacy') },
          ],
          indexes: [],
        },
      },
    ],
    enums: [{ kind: 'removed', value: { name: 'kind', values: [{ name: 'a' }] } }],
    refs: [{ kind: 'removed', value: ref('old<script>', 'users') }],
  },
};
const view = buildDiffView(result);
const ready = (v = view): DiffState => ({ status: 'ready', base: 'b', view: v });

describe('renderDiffPanel', () => {
  it('agrupa os itens na ordem de DIFF_GROUPS com contagem e botões de foco', () => {
    const html = renderDiffPanel(ready());
    const positions = [...html.matchAll(/data-diff-group="(\w+)"/g)].map((m) => m[1]);
    expect(positions).toEqual(DIFF_GROUPS.filter((g) => view.items.some((i) => i.group === g)));
    expect(html.match(new RegExp(`data-act="${DIFF_ACT.FOCUS}"`, 'g'))).toHaveLength(view.items.length);
    for (const item of view.items) expect(html).toContain(`data-diff-id="${escHtml(item.id)}"`);
  });

  it('cada linha tem marcador de texto e classe de status (nunca só cor)', () => {
    const html = renderDiffPanel(ready());
    expect(html).toContain('diff-added');
    expect(html).toContain('diff-modified');
    expect(html).toContain('diff-removed');
    expect(html).toMatch(/class="dmark"[^>]*>\+</);
    expect(html).toMatch(/class="dmark"[^>]*>~</);
    expect(html).toMatch(/class="dmark"[^>]*>−</);
  });

  it('escapa identificadores vindos do schema', () => {
    const html = renderDiffPanel(ready());
    expect(html).not.toContain('<script>');
    expect(html).toContain('old&lt;script&gt;');
  });

  it('resumo traduzido nos dois idiomas', () => {
    expect(renderDiffPanel(ready())).toContain('2 adicionados');
    setLanguage('en');
    expect(renderDiffPanel(ready())).toContain('2 added');
  });

  it('estado vazio: "sem diferenças"', () => {
    const empty = buildDiffView({ summary: { added: 0, removed: 0, modified: 0 }, diff: { tables: [], enums: [], refs: [] } });
    expect(renderDiffPanel(ready(empty))).toContain('Sem diferenças');
    setLanguage('en');
    expect(renderDiffPanel(ready(empty))).toContain('No differences');
  });

  it('carregando', () => {
    expect(renderDiffPanel({ status: 'loading' })).toContain('Carregando');
  });

  it('erro: mensagem do código, detalhe escapado e botão de tentar de novo', () => {
    const html = renderDiffPanel({ status: 'error', code: GIT_BASE_ERROR.UNTRACKED, detail: '<b>x</b>' });
    expect(html).toContain('Arquivo novo');
    expect(html).toContain('&lt;b&gt;x&lt;/b&gt;');
    expect(html).toContain(`data-act="${DIFF_ACT.RETRY}"`);
    const noGit = renderDiffPanel({ status: 'error', code: GIT_BASE_ERROR.NOT_A_REPO });
    expect(noGit).toContain('repositório git');
    setLanguage('en');
    expect(renderDiffPanel({ status: 'error', code: DIFF_ERROR.BASE_INVALID })).toContain('git HEAD');
  });

  it('sempre oferece a saída do modo', () => {
    for (const state of [ready(), { status: 'loading' } as DiffState, { status: 'error', code: GIT_BASE_ERROR.OTHER } as DiffState]) {
      expect(renderDiffPanel(state)).toContain(`data-act="${DIFF_ACT.EXIT}"`);
    }
  });
});

describe('cartões fantasma', () => {
  it('slots: um por tabela/enum removido, com tamanho do cartão', () => {
    expect(ghostSlots(view)).toEqual([
      { key: 'old<script>', w: CARD_W, h: cardH(2) },
      { key: 'enum:kind', w: 220, h: expect.any(Number) },
    ]);
  });

  it('tabela removida: translúcida, sem data-table (não arrasta nem seleciona) e com colunas', () => {
    const html = renderGhostTable(view.removedTables[0], { x: 10, y: 20 });
    expect(html).toContain('diff-ghost');
    expect(html).toContain('data-diff-ghost="old&lt;script&gt;"');
    expect(html).not.toMatch(/data-table=/);
    expect(html).toContain('left:10px;top:20px');
    expect(html).toContain(`height:${cardH(2)}px`);
    expect(html).toContain('opacity');
    expect(html).not.toContain('<script>');
  });

  it('enum removido', () => {
    const html = renderGhostEnum(view.removedEnums[0], { x: 1, y: 2 });
    expect(html).toContain('diff-ghost');
    expect(html).toContain('ENUM');
  });

  it('colunas removidas de tabelas do head viram linhas fantasma no fim do cartão', () => {
    const html = renderGhostRows(view.tables.get('users')!);
    expect(html).toContain('diff-removed');
    expect(html).toContain('legacy');
    expect(renderGhostRows(view.tables.get('audit')!)).toBe('');
  });

  it('aresta removida: tracejada e vermelha', () => {
    const head = [table('users', [col('id')])];
    const edges = removedEdges(view, head, { users: { x: 500, y: 0 } }, { 'old<script>': { x: 0, y: 300 } });
    expect(edges).toHaveLength(1);
    const svg = renderGhostEdges(edges);
    expect(svg).toContain('stroke-dasharray');
    expect(svg).toContain('var(--danger)');
    expect(svg).toContain('<path');
  });
});

describe('helpers', () => {
  it('classe por status', () => {
    expect(diffStatusClass('added')).toBe('diff-added');
    expect(diffStatusClass('modified')).toBe('diff-modified');
    expect(diffStatusClass('removed')).toBe('diff-removed');
  });

  it('título da coluna modificada lista as mudanças', () => {
    const cd = view.tables.get('users')!.columns.get('age')!;
    expect(columnDiffTitle(cd)).toBe('int → bigint');
    expect(columnDiffTitle(view.tables.get('users')!.columns.get('email')!)).toBe('');
  });
});
