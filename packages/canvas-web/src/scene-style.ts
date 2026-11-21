import { CARD_W, GROUP_PAD, cardH } from './glass';
import type { GroupBox, Schema } from './types';
import { qualifiedTableName } from './types';
import type { Point } from './viewport';

/** Mantém a grade legível ao reduzir o zoom. */
export function canvasGridStep(zoom: number): number {
  if (!Number.isFinite(zoom) || zoom <= 0) return 24;
  let step = 24 * zoom;
  while (step < 20) step *= 2;
  return step;
}

/** A moldura acompanha a posição atual das tabelas no canvas, inclusive ao arrastar. */
export function canvasGroups(groups: readonly GroupBox[], schema: Schema, positions: Record<string, Point>): GroupBox[] {
  const tables = new Map(schema.tables.map(table => [qualifiedTableName(table), table]));
  return groups.flatMap(group => {
    let left = Infinity, top = Infinity, right = -Infinity, bottom = -Infinity;
    let count = 0;
    for (const name of group.tables) {
      const p = positions[name], table = tables.get(name);
      if (!p || !table) continue;
      left = Math.min(left, p.x);
      top = Math.min(top, p.y);
      right = Math.max(right, p.x + CARD_W);
      bottom = Math.max(bottom, p.y + cardH(table.columns.length));
      count++;
    }
    if (!count) return [];
    const pad = GROUP_PAD - 8;
    return [{ ...group, x: left - pad, y: top - pad - 8, w: right - left + 2 * pad, h: bottom - top + 2 * pad + 8, count }];
  });
}
