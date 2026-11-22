import { describe, expect, it } from 'vitest';
import { CARD_W } from '../src/glass';
import { extendBounds, ghostRects, placeGhosts, type Bounds, type GhostSlot } from '../src/diff/layout';

const SLOTS: GhostSlot[] = [
  { key: 'old', w: CARD_W, h: 100 },
  { key: 'enum:kind', w: 180, h: 60 },
];

describe('ghostRects', () => {
  it('une cada slot à sua posição, ignorando slots sem posição', () => {
    const rects = ghostRects(SLOTS, { old: { x: 10, y: 20 } });
    expect(rects).toEqual([{ x: 10, y: 20, w: CARD_W, h: 100 }]);
  });
});

describe('extendBounds', () => {
  const base: Bounds = { minX: 0, minY: 0, maxX: 100, maxY: 100 };

  it('sem retângulos devolve os mesmos limites', () => {
    expect(extendBounds(base, [])).toEqual(base);
  });

  it('inclui retângulos fora dos limites (fantasmas abaixo do diagrama)', () => {
    const out = extendBounds(base, [{ x: -10, y: 140, w: 50, h: 60 }]);
    expect(out).toEqual({ minX: -10, minY: 0, maxX: 100, maxY: 200 });
  });

  it('aceita limites vazios (Infinity) como ponto de partida', () => {
    const empty: Bounds = { minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity };
    expect(extendBounds(empty, [{ x: 5, y: 6, w: 10, h: 20 }])).toEqual({ minX: 5, minY: 6, maxX: 15, maxY: 26 });
  });

  it('fantasmas posicionados por placeGhosts entram nos limites do fit/minimapa', () => {
    const occupied = [{ x: 0, y: 0, w: CARD_W, h: 100 }];
    const pos = placeGhosts(occupied, SLOTS);
    const b = extendBounds({ minX: 0, minY: 0, maxX: CARD_W, maxY: 100 }, ghostRects(SLOTS, pos));
    expect(b.maxY).toBeGreaterThan(100);
    expect(b.maxY).toBe(Math.max(...SLOTS.map((s) => pos[s.key].y + s.h)));
  });
});
