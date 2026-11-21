import { describe, it, expect } from 'vitest';
import { ConnectDbModel, buildRequest, parseSchemas, summarizeTest, DB_ENGINES } from '../src/connect-db';
import { catalog } from '../src/actions';

const fields = {
  host: ' db.local ',
  port: '5433',
  database: 'shop',
  user: 'app',
  password: ' p@ss ',
  path: '',
  schemas: ' public, sales ,,',
};

describe('connect to database', () => {
  it('builds the request without trimming the password', () => {
    const r = buildRequest('postgres', fields, true);
    expect(r).toMatchObject({
      dialect: 'postgres',
      host: 'db.local',
      port: 5433,
      database: 'shop',
      user: 'app',
      password: ' p@ss ',
      schemas: ['public', 'sales'],
      trust_server_certificate: false,
    });
  });

  it('only SQL Server honours trust-certificate and SQLite ignores schemas', () => {
    expect(buildRequest('sqlserver', fields, true).trust_server_certificate).toBe(true);
    expect(buildRequest('sqlite', { ...fields, path: '/a.db' }, true)).toMatchObject({
      schemas: [],
      trust_server_certificate: false,
      path: '/a.db',
    });
  });

  it('falls back to the default port for blank or invalid values', () => {
    for (const p of ['', 'abc', '0', '70000']) {
      expect(buildRequest('mysql', { ...fields, port: p }, false).port).toBeNull();
    }
  });

  it('parses the schema list', () => {
    expect(parseSchemas(' a, b ,,c ')).toEqual(['a', 'b', 'c']);
    expect(parseSchemas('  ')).toEqual([]);
  });

  it('walks idle -> connecting -> ready and merges by replace or insert', () => {
    const m = new ConnectDbModel();
    expect(m.canApply()).toBe(false);
    m.start();
    expect(m.status).toBe('connecting');
    m.finish({ dbml: 'Table b {\n  id int\n}\n', warnings: ['w'], tables: 1 });
    expect(m.canApply()).toBe(true);
    const existing = 'Table a {\n  id int\n}\n';
    expect(m.buildMergedSource(existing, 'insert')).toBe(
      'Table a {\n  id int\n}\n\nTable b {\n  id int\n}\n'
    );
    expect(m.buildMergedSource(existing, 'replace')).toBe('Table b {\n  id int\n}\n');
    expect(m.buildMergedSource('  ', 'insert')).toBe('Table b {\n  id int\n}\n');
  });

  it('reports failures and zero-table results, and clears them on engine change', () => {
    const m = new ConnectDbModel();
    m.fail('boom');
    expect(m.status).toBe('failed');
    expect(m.canApply()).toBe(false);
    m.start();
    m.finish({ dbml: '', warnings: [], tables: 0 });
    expect(m.status).toBe('failed');
    expect(m.error).toContain('nenhuma tabela');
    m.setEngine('mysql');
    expect(m.status).toBe('idle');
  });

  it('lists the four supported engines and exposes the action in the palette', () => {
    expect(DB_ENGINES.map((e) => e.label)).toEqual(['PostgreSQL', 'MySQL', 'SQL Server', 'SQLite']);
    const a = catalog().find((e) => e.id === 'ConnectDatabase');
    expect(a?.group).toBe('Arquivo');
    expect(a?.label).toBe('Conectar ao banco…');
  });
});

describe('test connection', () => {
  it('summarizes the server and truncates long schema lists', () => {
    expect(summarizeTest('SQLite 3.45', [])).toBe('Conectado a SQLite 3.45.');
    expect(summarizeTest('PG 16', ['a', 'b'])).toBe('Conectado a PG 16. Schemas: a, b.');
    const many = Array.from({ length: 11 }, (_, i) => `s${i}`);
    expect(summarizeTest('X', many).endsWith('s7 (+3 a mais).')).toBe(true);
  });

  it('a successful test never enables Replace/Insert and clears older results', () => {
    const m = new ConnectDbModel();
    m.finish({ dbml: 'Table a {\n  id int\n}\n', warnings: [], tables: 1 });
    expect(m.canApply()).toBe(true);
    m.finishTest('MySQL 8.4', ['shop']);
    expect(m.status).toBe('tested');
    expect(m.error).toBe('Conectado a MySQL 8.4. Schemas: shop.');
    expect(m.canApply()).toBe(false);
  });
});
