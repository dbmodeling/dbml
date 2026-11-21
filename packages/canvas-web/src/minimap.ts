/**
 * Pure coordinate mapping between diagram world space and minimap pixels.
 * Exact 1:1 mirror of `crates/app/src/canvas_geom.rs::MinimapTransform`.
 */

export const MINIMAP_W = 210;
export const MINIMAP_H = 136;
export const MINIMAP_PAD = 10;

export interface MinimapTransform {
  scale: number;
  offsetX: number;
  offsetY: number;
}

export function computeMinimapTransform(
  minX: number,
  minY: number,
  maxX: number,
  maxY: number,
  mapW = MINIMAP_W,
  mapH = MINIMAP_H,
  pad = MINIMAP_PAD
): MinimapTransform {
  const innerW = Math.max(1, mapW - 2 * pad);
  const innerH = Math.max(1, mapH - 2 * pad);
  const worldW = Math.max(1, maxX - minX);
  const worldH = Math.max(1, maxY - minY);
  const scale = Math.min(innerW / worldW, innerH / worldH);
  const offsetX = pad + (innerW - worldW * scale) * 0.5 - minX * scale;
  const offsetY = pad + (innerH - worldH * scale) * 0.5 - minY * scale;
  return { scale, offsetX, offsetY };
}

export function worldToMap(xf: MinimapTransform, wx: number, wy: number): { x: number; y: number } {
  return { x: wx * xf.scale + xf.offsetX, y: wy * xf.scale + xf.offsetY };
}

export function mapToWorld(xf: MinimapTransform, mx: number, my: number): { x: number; y: number } {
  return { x: (mx - xf.offsetX) / xf.scale, y: (my - xf.offsetY) / xf.scale };
}

export function panForMapPoint(
  xf: MinimapTransform,
  mx: number,
  my: number,
  zoom: number,
  viewportW: number,
  viewportH: number
): { x: number; y: number } {
  const w = mapToWorld(xf, mx, my);
  return {
    x: viewportW * 0.5 - w.x * zoom,
    y: viewportH * 0.5 - w.y * zoom,
  };
}
