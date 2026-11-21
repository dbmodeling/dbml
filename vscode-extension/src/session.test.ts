import { describe, expect, it } from 'vitest';
import { sanitizeSession } from './session.js';

describe('sanitizeSession', () => {
  it('mantém zoom, pan, painel e editor', () => {
    const s = { pane: 'schema', editorOpen: true, zoom: 1.5, pan: { x: -10, y: 20 } };
    expect(sanitizeSession(s)).toEqual(s);
  });

  it('descarta campos conhecidos com tipo/valor inválido', () => {
    expect(sanitizeSession({ pane: 'x', editorOpen: 'sim', zoom: NaN, pan: { x: 1 } })).toEqual({});
    expect(sanitizeSession({ zoom: Infinity })).toEqual({});
    expect(sanitizeSession({ zoom: 0 })).toEqual({});
    expect(sanitizeSession({ zoom: 1000 })).toEqual({});
  });

  it('preserva campos extras serializáveis (ex.: seleção) para o webview evoluir sem mudar o host', () => {
    expect(sanitizeSession({ zoom: 1, selection: ['users', 'posts'] })).toEqual({ zoom: 1, selection: ['users', 'posts'] });
  });

  it('rejeita o que não é objeto simples ou é grande demais', () => {
    expect(sanitizeSession(null)).toBeUndefined();
    expect(sanitizeSession('x')).toBeUndefined();
    expect(sanitizeSession([])).toBeUndefined();
    expect(sanitizeSession({ selection: 'x'.repeat(100_000) })).toBeUndefined();
  });
});
