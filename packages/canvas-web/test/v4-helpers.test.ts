import { describe, it, expect } from 'vitest';
import { tokenizeLine, tableLineRange } from '../src/dbml-highlight';
import { curvedEdge, roundedPath } from '../src/edge-path';
import { TABLE_COLORS, RADIUS_CARD, getGlass } from '../src/glass';

describe('glass v5 tokens and palette', () => {
  it('usa os 8 hex da paleta v5 (matizes 255, 200, 150, 120, 85, 42, 350, 295)', () => {
    expect(TABLE_COLORS.map(([, hex]) => hex)).toEqual([
      '#589aed',
      '#00b1ba',
      '#4eb068',
      '#8ea432',
      '#c09000',
      '#df784e',
      '#d771a4',
      '#9f85e5',
    ]);
  });

  it('aplica derivações do v5 nos temas escuro e claro', () => {
    const dark = getGlass('dark');
    const light = getGlass('light');
    expect(RADIUS_CARD).toBe(12);
    expect(dark.relStroke).toBe('rgba(220, 225, 235, 0.32)');
    expect(light.relStroke).toBe('rgba(30, 30, 40, 0.3)');
    expect(dark.cardShadowSelected).toContain('rgba(229, 103, 49, 0.18)');
    expect(light.cardShadowSelected).toContain('rgba(229, 103, 49, 0.18)');
    expect(dark.swatchRing).toBe('#121316');
    expect(light.swatchRing).toBe('#f6f6f4');
    expect(dark.link).toBe('#589aed');
    expect(dark.idx).toBe('#00b1ba');
  });
});

describe('dbml-highlight', () => {
  it('classifica palavra-chave, tipo, atributo, cor e comentário', () => {
    const kinds = (l: string) => tokenizeLine(l).filter((t) => t.kind !== 'plain' && t.kind !== 'punc').map((t) => [t.text, t.kind]);
    expect(kinds('Table users [headercolor: #8b5cf6] {')).toEqual([['Table', 'kw'], ['headercolor', 'attr'], ['#8b5cf6', 'num']]);
    expect(kinds('  email varchar(255) [unique, not null] // x')).toEqual([
      ['varchar', 'ty'], ['255', 'num'], ['unique', 'attr'], ['not null', 'attr'], ['// x', 'cm'],
    ]);
  });
  it('não quebra nomes que contêm palavras reservadas', () => {
    expect(tokenizeLine('null_count').every((t) => t.kind === 'plain')).toBe(true);
  });
  it('acha o bloco da tabela (com e sem schema)', () => {
    const src = ['Table a {', '  id int', '}', '', 'Table s.b [x] {', '  id int', '}'];
    expect(tableLineRange(src, null, 'a')).toEqual([0, 2]);
    expect(tableLineRange(src, 's', 'b')).toEqual([4, 6]);
    expect(tableLineRange(src, null, 'zz')).toBeNull();
  });
});

describe('roundedPath', () => {
  it('arredonda cantos com raio 8 e limita a metade do trecho', () => {
    const p = [{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 10 }, { x: 200, y: 10 }];
    expect(roundedPath(p)).toBe('M0 0 L95 0 Q100 0 100 5 L100 5 Q100 10 105 10 L200 10');
  });
  it('trecho reto continua reto', () => {
    expect(roundedPath([{ x: 0, y: 5 }, { x: 50, y: 5 }])).toBe('M0 5 L50 5');
  });
});

describe('curvedEdge', () => {
  it('usa tangentes horizontais nas portas e contorna cartões sobrepostos', () => {
    expect(curvedEdge(0, 20, 400, 80).path).toBe('M260 20 C330 20 330 80 400 80');
    expect(curvedEdge(400, 20, 0, 80).path).toBe('M400 20 C330 20 330 80 260 80');
    expect(curvedEdge(0, 20, 100, 80).path).toBe('M0 20 C-70 20 30 80 100 80');
  });
});
