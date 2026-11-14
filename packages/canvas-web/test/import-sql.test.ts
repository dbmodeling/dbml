import { describe, it, expect } from 'vitest';
import { IMPORT_DIALECTS, ImportSqlModel, MAX_IMPORT_BYTES, RunSqlImport } from '../src/import-sql';

const stubImport: RunSqlImport = (sql, dialect) => ({
  dbml: sql.trim() ? `// ${dialect}\nTable t {\n  id int [pk]\n}\n` : '',
  warnings: sql.includes('VIEW') ? [{ line: 1, column: 1, message: 'skipped view' }] : [],
});

describe('IMPORT_DIALECTS', () => {
  it('lists the same five databases, in the same order, as the export menu', () => {
    expect(IMPORT_DIALECTS.map((d) => d.value)).toEqual(['postgres', 'mysql', 'sqlite', 'sqlserver', 'oracle']);
    expect(IMPORT_DIALECTS.map((d) => d.label)).toEqual(['PostgreSQL', 'MySQL', 'SQLite', 'SQL Server', 'Oracle']);
  });

  it('previews SQL Server and Oracle through the model', () => {
    for (const d of ['sqlserver', 'oracle'] as const) {
      const model = new ImportSqlModel(d);
      model.setSql('CREATE TABLE t (id int);', stubImport);
      expect(model.dbml).toContain(`// ${d}`);
    }
  });
});

describe('ImportSqlModel', () => {
  it('recomputes preview when dialect changes', () => {
    const model = new ImportSqlModel('postgres');
    model.setSql('CREATE TABLE t (id int);', stubImport);
    expect(model.dbml).toContain('// postgres');

    model.setDialect('mysql', stubImport);
    expect(model.dbml).toContain('// mysql');
  });

  it('disables apply when input is empty or whitespace', () => {
    const model = new ImportSqlModel('postgres');
    expect(model.canApply()).toBe(false);

    model.setSql('   \n  ', stubImport);
    expect(model.canApply()).toBe(false);
    expect(model.dbml).toBe('');
  });

  it('flags inputs larger than 5 MB without running importer', () => {
    const model = new ImportSqlModel('sqlite');
    let called = false;
    const huge = 'a'.repeat(MAX_IMPORT_BYTES + 1);
    model.setSql(huge, () => {
      called = true;
      return { dbml: '', warnings: [] };
    });
    expect(called).toBe(false);
    expect(model.tooLarge).toBe(true);
    expect(model.canApply()).toBe(false);
  });

  it('appends with one blank line on insert and overwrites on replace', () => {
    const model = new ImportSqlModel('postgres');
    model.setSql('CREATE TABLE t (id int);', stubImport);

    expect(model.buildMergedSource('Table a {\n  id int\n}\n', 'insert')).toBe(
      'Table a {\n  id int\n}\n\n// postgres\nTable t {\n  id int [pk]\n}\n'
    );
    expect(model.buildMergedSource('Table a {\n  id int\n}\n', 'replace')).toBe(
      '// postgres\nTable t {\n  id int [pk]\n}\n'
    );
    expect(model.buildMergedSource('   \n', 'insert')).toBe(
      '// postgres\nTable t {\n  id int [pk]\n}\n'
    );
  });

  it('surfaces warnings from the importer', () => {
    const model = new ImportSqlModel('postgres');
    model.setSql('CREATE VIEW v AS SELECT 1;', stubImport);
    expect(model.warnings).toHaveLength(1);
    expect(model.warnings[0].message).toBe('skipped view');
  });
});
