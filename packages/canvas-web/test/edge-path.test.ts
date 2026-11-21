import { describe, expect, it } from 'vitest';
import { curvedEdge } from '../src/edge-path';
import { edgeKey } from '../src/types';
import { exportToSvg } from '../src/export';
import type { Schema } from '../src/types';

describe('estilo de relação', () => {
  it('mantém a curva automática sem estilo', () => {
    const curve = curvedEdge(0, 10, 400, 110);
    expect(curve.path).toBe(`M${curve.x1} 10 C${curve.x1 + curve.d1} 10 ${curve.x2 + curve.d2} 110 ${curve.x2} 110`);
    expect(curvedEdge(0, 10, 400, 110, {}).path).toBe(curve.path);
  });

  it('desloca os controles e o ponto médio', () => {
    const base = curvedEdge(0, 10, 400, 110);
    const moved = curvedEdge(0, 10, 400, 110, { dx: 40, dy: -20 });
    expect(moved.c1).toEqual({ x: base.c1.x + 40, y: base.c1.y - 20 });
    expect(moved.c2).toEqual({ x: base.c2.x + 40, y: base.c2.y - 20 });
    expect(moved.midpoint).toEqual({ x: base.midpoint.x + 30, y: base.midpoint.y - 15 });
  });

  it('faz reta e ignora deslocamento', () => {
    const curve = curvedEdge(0, 10, 400, 110, { straight: true, dx: 900, dy: 900 });
    expect(curve.c1.x).toBeCloseTo(curve.x1 + (curve.x2 - curve.x1) / 3);
    expect(curve.c2.y).toBeCloseTo(10 + 200 / 3);
    expect(curve.midpoint.x).toBeCloseTo((curve.x1 + curve.x2) / 2);
    expect(curve.midpoint.y).toBeCloseTo(60);
  });

  it('usa chave qualificada e aplica estilo na exportação', () => {
    expect(edgeKey({ from_table: 'a.users', from_col: 'id,org', to_table: 'b.posts', to_col: 'user,org' })).toBe('a.users.id,org->b.posts.user,org');
    const schema: Schema = { tables: [
      { name: 'a', columns: [{ name: 'id', ty: 'int', pk: true, not_null: true, unique: true }] },
      { name: 'b', columns: [{ name: 'id', ty: 'int', pk: true, not_null: true, unique: true }] },
    ], refs: [{ from: { table: 'a', column: 'id' }, to: { table: 'b', column: 'id' }, kind: 'OneToOne' }], groups: [] };
    const positions = { a: { x: 0, y: 0 }, b: { x: 400, y: 0 } };
    const base = exportToSvg(schema, positions);
    const moved = exportToSvg(schema, positions, null, 'dark', { 'a.id->b.id': { dy: -1000 } });
    expect(moved).not.toBe(base);
    expect(moved).toContain('C330 -937 330 -937');
  });
});

import { vi } from 'vitest';
import { Workbench } from '../src/workbench';

it('arrasta, desfaz, alterna e restaura com persistência do sidecar', () => {
  vi.useFakeTimers();
  vi.stubGlobal('window', globalThis);
  const wb = Object.create(Workbench.prototype) as any;
  const saved: any[] = [];
  Object.assign(wb, {
    source: '', positions: {}, selection: [], selectedEnum: null,
    selectedRef: { from_table: 'a', from_col: 'id', to_table: 'b', to_col: 'id' },
    edges: {}, extraLayout: { custom: { mantido: true } }, undoStack: [], redoStack: [],
    zoom: 1, pan: { x: 0, y: 0 }, saveTimer: null, fx: [],
    transport: { saveLayout: (layout: unknown) => { saved.push(layout); return Promise.resolve(); } },
    scheduleRender: () => {}, reanalyze: () => {}, toWorld: (e: any) => ({ x: e.clientX, y: e.clientY }),
  });
  const snapshot = wb.captureSnapshot();
  wb.drag = { kind: 'edge', last: { x: 0, y: 0 }, snapshot, moved: false };
  wb.onMove({ clientX: 30, clientY: -15 });
  wb.onUp();
  vi.runAllTimers();
  expect(saved.at(-1)).toMatchObject({ custom: { mantido: true }, edges: { 'a.id->b.id': { dx: 40, dy: -20 } } });
  wb.undo();
  expect(wb.edges).toEqual({});
  wb.redo();
  expect(wb.edges['a.id->b.id']).toEqual({ dx: 40, dy: -20 });
  wb.runAction('ToggleEdgeStraight');
  expect(wb.edges['a.id->b.id']).toEqual({ dx: 40, dy: -20, straight: true });
  wb.runAction('ToggleEdgeStraight');
  expect(wb.edges['a.id->b.id']).toEqual({ dx: 40, dy: -20 });
  wb.runAction('ResetEdgeCurve');
  expect(wb.edges).toEqual({});
  wb.undo();
  expect(wb.edges['a.id->b.id']).toEqual({ dx: 40, dy: -20 });
  vi.runAllTimers();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});
