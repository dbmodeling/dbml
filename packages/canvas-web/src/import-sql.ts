export type ImportDialect = 'postgres' | 'mysql' | 'sqlite' | 'sqlserver' | 'oracle';

/** Mesmos bancos, rótulos e ordem do dropdown de exportação. */
export const IMPORT_DIALECTS: ReadonlyArray<{ value: ImportDialect; label: string }> = [
  { value: 'postgres', label: 'PostgreSQL' },
  { value: 'mysql', label: 'MySQL' },
  { value: 'sqlite', label: 'SQLite' },
  { value: 'sqlserver', label: 'SQL Server' },
  { value: 'oracle', label: 'Oracle' },
];

export interface ImportWarning {
  line: number;
  column: number;
  message: string;
}

export interface ImportSqlResult {
  dbml: string;
  warnings: ImportWarning[];
}

export type RunSqlImport = (sql: string, dialect: ImportDialect) => ImportSqlResult;

export const MAX_IMPORT_BYTES = 5 * 1024 * 1024;

export class ImportSqlModel {
  dialect: ImportDialect;
  sql = '';
  dbml = '';
  warnings: ImportWarning[] = [];
  tooLarge = false;
  confirmReplace = false;

  constructor(initialDialect: ImportDialect = 'postgres') {
    this.dialect = initialDialect;
  }

  setDialect(dialect: ImportDialect, runImport: RunSqlImport): void {
    this.dialect = dialect;
    this.recompute(runImport);
  }

  setSql(sql: string, runImport: RunSqlImport): void {
    this.sql = sql;
    this.confirmReplace = false;
    this.recompute(runImport);
  }

  recompute(runImport: RunSqlImport): void {
    if (this.sql.length > MAX_IMPORT_BYTES) {
      this.tooLarge = true;
      this.dbml = '';
      this.warnings = [];
      return;
    }
    this.tooLarge = false;
    if (!this.sql.trim()) {
      this.dbml = '';
      this.warnings = [];
      return;
    }
    const res = runImport(this.sql, this.dialect);
    this.dbml = res.dbml ?? '';
    this.warnings = Array.isArray(res.warnings) ? res.warnings : [];
  }

  canApply(): boolean {
    return !this.tooLarge && this.sql.trim().length > 0 && this.dbml.trim().length > 0;
  }

  buildMergedSource(existing: string, mode: 'insert' | 'replace'): string {
    if (mode === 'replace' || existing.trim().length === 0) {
      return this.dbml;
    }
    const trimmed = existing.replace(/\s+$/, '');
    return `${trimmed}\n\n${this.dbml}`;
  }
}
