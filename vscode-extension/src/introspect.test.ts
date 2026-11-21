import { describe, expect, it } from 'vitest';
import { buildArgs, buildUrl, type IntrospectRequest } from './introspect.js';

const base: IntrospectRequest = {
  dialect: 'postgres',
  host: 'db.example.com',
  port: 5433,
  database: 'app',
  user: 'alice',
  password: 's3cret',
  path: '',
  trust_server_certificate: false,
  schemas: [],
};

describe('buildUrl', () => {
  it('monta a URL sem a senha', () => {
    const url = buildUrl(base);
    expect(url).toBe('postgres://alice@db.example.com:5433/app');
    expect(url).not.toContain('s3cret');
  });

  it('usa localhost e a porta padrão do dialeto', () => {
    expect(buildUrl({ ...base, host: '  ', port: null })).toBe('postgres://alice@localhost:5432/app');
    expect(buildUrl({ ...base, dialect: 'mysql', port: null })).toBe('mysql://alice@db.example.com:3306/app');
    expect(buildUrl({ ...base, dialect: 'sqlserver', port: null })).toBe('sqlserver://alice@db.example.com:1433/app');
  });

  it('codifica usuário e banco', () => {
    expect(buildUrl({ ...base, user: 'a b@c', database: 'x/y?z' })).toBe(
      'postgres://a%20b%40c@db.example.com:5433/x%2Fy%3Fz'
    );
  });

  it('coloca IPv6 entre colchetes', () => {
    expect(buildUrl({ ...base, host: '::1' })).toBe('postgres://alice@[::1]:5433/app');
    expect(buildUrl({ ...base, host: '[::1]' })).toBe('postgres://alice@[::1]:5433/app');
  });

  it('omite o banco quando vazio', () => {
    expect(buildUrl({ ...base, database: '' })).toBe('postgres://alice@db.example.com:5433');
  });

  it('SQL Server aceita trustServerCertificate', () => {
    expect(buildUrl({ ...base, dialect: 'sqlserver', trust_server_certificate: true })).toBe(
      'sqlserver://alice@db.example.com:5433/app?trustServerCertificate=true'
    );
    expect(buildUrl({ ...base, trust_server_certificate: true })).not.toContain('trustServerCertificate');
  });

  it('SQLite usa o caminho do arquivo', () => {
    expect(buildUrl({ ...base, dialect: 'sqlite', path: ' /tmp/a.db ' })).toBe('/tmp/a.db');
    expect(() => buildUrl({ ...base, dialect: 'sqlite', path: '  ' })).toThrow(/SQLite/);
  });

  it('exige usuário nos demais dialetos', () => {
    expect(() => buildUrl({ ...base, user: ' ' })).toThrow('user is required');
  });
});

describe('buildArgs', () => {
  it('senha vai por stdin, nunca na linha de comando', () => {
    const args = buildArgs(base);
    expect(args).toEqual(['import', '--url', 'postgres://alice@db.example.com:5433/app', '--password-stdin']);
    expect(args.join(' ')).not.toContain('s3cret');
  });

  it('repete --schema para cada schema', () => {
    expect(buildArgs({ ...base, password: '', schemas: ['public', 'audit'] })).toEqual([
      'import',
      '--url',
      'postgres://alice@db.example.com:5433/app',
      '--schema',
      'public',
      '--schema',
      'audit',
    ]);
  });

  it('SQLite ignora schemas e senha', () => {
    expect(buildArgs({ ...base, dialect: 'sqlite', path: '/tmp/a.db', schemas: ['x'] })).toEqual([
      'import',
      '--url',
      '/tmp/a.db',
    ]);
  });

  it('valores com aparência de flag permanecem como argumento único', () => {
    const args = buildArgs({ ...base, password: '', schemas: ['--password-stdin'] });
    expect(args.slice(-2)).toEqual(['--schema', '--password-stdin']);
  });
});
