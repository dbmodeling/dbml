import { describe, it, expect } from 'vitest';
import { GROUP_EXPORT_CODE, GROUP_EXPORT_IMAGE, catalog } from '../src/actions';

const labels = (group: string) => catalog().filter((e) => e.group === group).map((e) => e.label);

describe('export dropdown groups', () => {
  it('code dropdown lists the five databases, Mermaid, and documentation exports', () => {
    expect(labels(GROUP_EXPORT_CODE)).toEqual([
      'Exportar código: PostgreSQL (.sql)',
      'Exportar código: MySQL (.sql)',
      'Exportar código: SQLite (.sql)',
      'Exportar código: SQL Server (.sql)',
      'Exportar código: Oracle (.sql)',
      'Exportar código: Mermaid erDiagram (.mmd)',
      'Exportar documentação: HTML (.html)',
      'Exportar documentação: Markdown (.md)',
    ]);
  });

  it('image dropdown lists PNG, JPEG and SVG', () => {
    expect(labels(GROUP_EXPORT_IMAGE)).toEqual([
      'Exportar imagem: PNG',
      'Exportar imagem: JPEG',
      'Exportar imagem: SVG vetorial',
    ]);
  });

  it('keeps no leftover legacy Export group', () => {
    expect(catalog().some((e) => e.group === 'Exportar')).toBe(false);
  });
});
