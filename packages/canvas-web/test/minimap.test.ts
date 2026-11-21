import { describe, it, expect } from 'vitest';
import { computeMinimapTransform, mapToWorld, panForMapPoint, worldToMap } from '../src/minimap';

describe('minimap coordinate mapping', () => {
  it('roundtrips world coordinates and centers camera pan accurately', () => {
    const xf = computeMinimapTransform(-200, 100, 1000, 700, 210, 136, 10);
    const mid = worldToMap(xf, 400, 400);
    expect(mid.x).toBeCloseTo(105, 3);
    expect(mid.y).toBeCloseTo(68, 3);

    const back = mapToWorld(xf, mid.x, mid.y);
    expect(back.x).toBeCloseTo(400, 3);
    expect(back.y).toBeCloseTo(400, 3);

    const pan = panForMapPoint(xf, mid.x, mid.y, 1.5, 1200, 800);
    expect(pan.x).toBeCloseTo(600 - 400 * 1.5, 3);
    expect(pan.y).toBeCloseTo(400 - 400 * 1.5, 3);
  });
});
