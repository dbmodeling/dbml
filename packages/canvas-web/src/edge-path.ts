/** Polilinha ortogonal com cantos arredondados (raio máximo `r`, limitado à metade de cada trecho). */
export function roundedPath(points: ReadonlyArray<{ x: number; y: number }>, r = 8): string {
  if (points.length === 0) return '';
  let d = `M${points[0].x} ${points[0].y}`;
  for (let i = 1; i < points.length; i++) {
    const p = points[i];
    const prev = points[i - 1];
    const next = points[i + 1];
    if (!next) {
      d += ` L${p.x} ${p.y}`;
      continue;
    }
    const l1 = Math.hypot(p.x - prev.x, p.y - prev.y);
    const l2 = Math.hypot(next.x - p.x, next.y - p.y);
    const k = Math.min(r, l1 / 2, l2 / 2);
    if (k <= 0) {
      d += ` L${p.x} ${p.y}`;
      continue;
    }
    const a = { x: p.x - ((p.x - prev.x) / l1) * k, y: p.y - ((p.y - prev.y) / l1) * k };
    const b = { x: p.x + ((next.x - p.x) / l2) * k, y: p.y + ((next.y - p.y) / l2) * k };
    d += ` L${a.x} ${a.y} Q${p.x} ${p.y} ${b.x} ${b.y}`;
  }
  return d;
}

import { CARD_W } from './glass';
import type { EdgeStyle, RoutedEdge } from './types';

/** Curva entre as portas horizontais mais próximas dos cartões. */
export function curvedEdge(ax: number, ay: number, bx: number, by: number, style?: EdgeStyle) {
  const right = bx >= ax + CARD_W;
  const left = bx + CARD_W <= ax;
  const x1 = right ? ax + CARD_W : ax;
  const x2 = left ? bx + CARD_W : bx;
  // controle = metade do vão (mín. 50): relação longa sai horizontal e faz "S", não diagonal
  const d = Math.max(50, Math.abs(x2 - x1) / 2);
  const d1 = right ? d : left ? -d : -70;
  const d2 = right ? -d : left ? d : -70;
  const c1 = style?.straight ? { x: x1 + (x2 - x1) / 3, y: ay + (by - ay) / 3 } : { x: x1 + d1 + (style?.dx ?? 0), y: ay + (style?.dy ?? 0) };
  const c2 = style?.straight ? { x: x1 + 2 * (x2 - x1) / 3, y: ay + 2 * (by - ay) / 3 } : { x: x2 + d2 + (style?.dx ?? 0), y: by + (style?.dy ?? 0) };
  const midpoint = { x: (x1 + 3 * c1.x + 3 * c2.x + x2) / 8, y: (ay + 3 * c1.y + 3 * c2.y + by) / 8 };
  return { x1, x2, d1, d2, c1, c2, midpoint, path: `M${x1} ${ay} C${c1.x} ${c1.y} ${c2.x} ${c2.y} ${x2} ${by}` };
}

/** Caminho desenhado: com estilo, a curva editada; sem estilo, a rota do Wasm que desvia dos cartões. */
export function edgeDrawing(e: Pick<RoutedEdge, 'svg_path' | 'auto_path' | 'auto_mid'>, curve: ReturnType<typeof curvedEdge>, style?: EdgeStyle) {
  return !style && e.auto_path
    ? { path: e.auto_path, midpoint: e.auto_mid ?? curve.midpoint }
    : { path: curve.path, midpoint: curve.midpoint };
}
