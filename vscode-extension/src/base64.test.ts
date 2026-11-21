import { describe, expect, it } from 'vitest';
import { base64ToBytes } from './base64.js';

describe('base64ToBytes', () => {
  it('decodifica bytes arbitrários, igual ao Buffer', () => {
    const original = Uint8Array.from({ length: 256 }, (_, i) => i);
    const b64 = Buffer.from(original).toString('base64');
    expect(Array.from(base64ToBytes(b64))).toEqual(Array.from(original));
  });
  it('string vazia vira array vazio', () => {
    expect(base64ToBytes('')).toHaveLength(0);
  });
});
