import { describe, it, expect } from 'vitest';
import { exportToMermaid, exportToSvg } from '../src/export';
import { Schema } from '../src/types';

describe('Diagram Export', () => {
  const schema: Schema = {
    tables: [
      {
        name: 'users',
        columns: [
          { name: 'id', ty: 'int', pk: true, not_null: true, unique: false },
          { name: 'email', ty: 'varchar', pk: false, not_null: true, unique: true },
        ],
      },
      {
        name: 'posts',
        columns: [
          { name: 'id', ty: 'int', pk: true, not_null: true, unique: false },
          { name: 'user_id', ty: 'int', pk: false, not_null: true, unique: false },
        ],
      },
    ],
    refs: [
      {
        from: { table: 'posts', column: 'user_id' },
        to: { table: 'users', column: 'id' },
        kind: 'ManyToOne',
      },
    ],
    groups: [],
  };

  const positions = {
    users: { x: 50, y: 50 },
    posts: { x: 350, y: 50 },
  };

  it('should export to Mermaid erDiagram with valid syntax and cardinality', () => {
    const mermaid = exportToMermaid(schema);

    expect(mermaid).toContain('erDiagram');
    expect(mermaid).toContain('users {');
    expect(mermaid).toContain('int id PK');
    expect(mermaid).toContain('varchar email UK');
    expect(mermaid).toContain('posts {');
    expect(mermaid).toContain('posts }o--|| users : "user_id to id"');
  });

  it('should export to SVG containing vector nodes and edges', () => {
    const svg = exportToSvg(schema, positions);

    expect(svg).toContain('<svg');
    expect(svg).toContain('</svg>');
    expect(svg).toContain('users');
    expect(svg).toContain('posts');
    expect(svg).toContain('path d="M');
    expect(svg).toContain('class="table-box"');
  });

  it('preserves the canvas grid, moved group, schema labels and enum cards', () => {
    const accented = { ...schema.tables[0], schema: 'auth', color: '#589aed' };
    const svg = exportToSvg(
      { ...schema, tables: [accented], refs: [] },
      { 'auth.users': { x: 200, y: 100 }, 'enum:status': { x: 600, y: 100 } },
      {
        groups: [{ name: 'Identity', tables: ['auth.users'], x: 0, y: 0, w: 10, h: 10, color: '#589aed', count: 1 }],
        tables: [{ name: 'auth.users', x: 0, y: 0, w: 260, h: 112, tint: '#589aed', fk_columns: [] }],
        enums: [{ name: 'status', x: 0, y: 0, w: 220, h: 78, values: ['Active'], referenced_by: ['auth.users'] }],
        edges: [],
        positions: {},
      },
      'light',
      {},
      { zoom: 0.5, selection: ['auth.users'] }
    );

    expect(svg).toContain('id="dot-grid" width="48"');
    expect(svg).toContain('@font-face');
    expect(svg).toContain('x="176.5" y="68.5" width="307"');
    expect(svg).toContain('>auth</text>');
    expect(svg).toContain('transform="translate(600 100)"');
    expect(svg).toContain('>Active</text>');
    expect(svg).toContain('class="column-badge middle" text-anchor="end" fill="#6b6e75">UQ</text>');
  });
});
