import { describe, expect, it } from 'vitest';
import { DIFF_FOCUS_MIN_ZOOM, DIFF_FOCUS_PAD, ENUM_CARD_W } from '../src/constants';
import { cameraFor, focusRect, type FocusContext } from '../src/diff/focus';
import { buildDiffView } from '../src/diff/model';
import { CARD_W, MIN_ZOOM, cardH } from '../src/glass';
import type { Column, DiffResult, Ref, Table } from '../src/types';

const col = (name: string): Column => ({ name, ty: 'int', pk: false, not_null: false, unique: false }) as Column;
const table = (name: string, cols: string[]): Table => ({ name, columns: cols.map(col), indexes: [] }) as unknown as Table;
const ref = (from: string, to: string): Ref =>
  ({ from: { table: from, column: 'id' }, to: { table: to, column: 'id' }, kind: 'ManyToOne' }) as unknown as Ref;

const result: DiffResult = {
  summary: { added: 1, removed: 2, modified: 1 },
  diff: {
    tables: [
      { kind: 'removed', value: table('old', ['id', 'x']) },
      { kind: 'modified', value: { name: 'users', props: null, columns: [{ kind: 'removed', value: col('legacy') }], indexes: [] } },
    ],
    enums: [{ kind: 'removed', value: { name: 'kind', values: [{ name: 'a' }, { name: 'b' }] } }],
    refs: [{ kind: 'removed', value: ref('old', 'users') }],
  },
};
const view = buildDiffView(result);

const ctx: FocusContext = {
  view,
  tables: [table('users', ['id', 'name'])],
  positions: { users: { x: 500, y: 100 } },
  ghostPositions: { old: { x: 0, y: 400 }, 'enum:kind': { x: 300, y: 400 } },
  enumRects: new Map(),
};
const item = (id: string) => {
  const found = view.items.find((i) => i.id === id);
  if (!found) throw new Error(`item ${id} ausente: ${view.items.map((i) => i.id).join(', ')}`);
  return found;
};

describe('focusRect', () => {
  it('tabela removida: cartão fantasma na posição escolhida, altura das colunas da base', () => {
    expect(focusRect(item('tables:removed:old'), ctx)).toEqual({ x: 0, y: 400, w: CARD_W, h: cardH(2) });
  });

  it('coluna/tabela modificada: cartão do head incluindo as colunas fantasma', () => {
    expect(focusRect(item('columns:removed:users.legacy'), ctx)).toEqual({ x: 500, y: 100, w: CARD_W, h: cardH(3) });
  });

  it('enum removido: cartão fantasma com a largura do cartão de enum', () => {
    const rect = focusRect(item('enums:removed:kind'), ctx);
    expect(rect).toMatchObject({ x: 300, y: 400, w: ENUM_CARD_W });
    expect(rect!.h).toBeGreaterThan(0);
  });

  it('relação removida: união dos cartões das duas pontas (fantasma e do head)', () => {
    const rect = focusRect(item('refs:removed:old.id → users.id'), ctx)!;
    expect(rect.x).toBe(0);
    expect(rect.y).toBe(100);
    expect(rect.x + rect.w).toBe(500 + CARD_W);
    expect(rect.y + rect.h).toBe(400 + cardH(2));
  });

  it('alvo sem posição conhecida devolve null', () => {
    expect(focusRect(item('tables:removed:old'), { ...ctx, ghostPositions: {} })).toBeNull();
  });
});

describe('cameraFor', () => {
  const viewport = { x: 1000, y: 600 };

  it('centraliza o retângulo na vista mantendo o zoom atual quando ele cabe', () => {
    const cam = cameraFor({ x: 100, y: 200, w: 260, h: 100 }, viewport, 1);
    expect(cam.zoom).toBe(1);
    expect(cam.pan).toEqual({ x: 500 - 230, y: 300 - 250 });
  });

  it('zoom muito baixo sobe até o mínimo de foco', () => {
    expect(cameraFor({ x: 0, y: 0, w: 260, h: 100 }, viewport, 0.3).zoom).toBe(1);
    expect(DIFF_FOCUS_MIN_ZOOM).toBeLessThan(1);
  });

  it('retângulo maior que a vista reduz o zoom para enquadrar (nunca abaixo do mínimo global)', () => {
    const cam = cameraFor({ x: 0, y: 0, w: 4000, h: 100 }, viewport, 1);
    expect(cam.zoom).toBeCloseTo(viewport.x / (4000 + 2 * DIFF_FOCUS_PAD), 5);
    expect(cameraFor({ x: 0, y: 0, w: 1e6, h: 1e6 }, viewport, 1).zoom).toBe(MIN_ZOOM);
  });
});
