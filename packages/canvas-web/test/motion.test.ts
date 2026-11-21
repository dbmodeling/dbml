import { describe, expect, it } from 'vitest';
import { actSelector, easeOut, lerp } from '../src/motion';

describe('motion', () => {
  it('easeOut vai de 0 a 1, cresce sempre e satura fora do intervalo', () => {
    expect(easeOut(0)).toBe(0);
    expect(easeOut(1)).toBe(1);
    expect(easeOut(-1)).toBe(0);
    expect(easeOut(2)).toBe(1);
    let last = 0;
    for (let k = 0.1; k <= 1; k += 0.1) { const v = easeOut(k); expect(v).toBeGreaterThan(last); last = v; }
    expect(easeOut(0.5)).toBeGreaterThan(0.5); // desacelera no fim
  });
  it('lerp interpola', () => expect(lerp(10, 20, 0.25)).toBe(12.5));
  it('actSelector distingue pelo dataset e escapa aspas', () => {
    expect(actSelector('set-mode', { act: 'set-mode', mode: 'dark', keep: '1' })).toBe('[data-act="set-mode"][data-mode="dark"]');
    expect(actSelector('focus-table', { table: 'a"b', fromCol: 'x' })).toBe('[data-act="focus-table"][data-table="a\\"b"][data-from-col="x"]');
  });
});
