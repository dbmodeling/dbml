import { describe, it, expect } from 'vitest';
import { GROUP_EXPORT_CODE, GROUP_EXPORT_IMAGE, catalog } from '../src/actions';

const labels = (group: string) => catalog().filter((e) => e.group === group).map((e) => e.label);

describe('export dropdown groups', () => {
  it('code dropdown lists the five databases then Mermaid', () => {
    expect(labels(GROUP_EXPORT_CODE)).toEqual([
      'Export Code: PostgreSQL (.sql)',
      'Export Code: MySQL (.sql)',
      'Export Code: SQLite (.sql)',
      'Export Code: SQL Server (.sql)',
      'Export Code: Oracle (.sql)',
      'Export Code: Mermaid erDiagram (.mmd)',
    ]);
  });

  it('image dropdown lists PNG, JPEG and SVG', () => {
    expect(labels(GROUP_EXPORT_IMAGE)).toEqual([
      'Export Image: PNG',
      'Export Image: JPEG',
      'Export Image: Vector SVG',
    ]);
  });

  it('keeps no leftover legacy Export group', () => {
    expect(catalog().some((e) => e.group === 'Export')).toBe(false);
  });
});
