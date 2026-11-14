import type { IntrospectRequest, IntrospectResult } from './types';

export type DbEngine = IntrospectRequest['dialect'];

/** Mesmos bancos e ordem do modal do Desktop. Oracle ainda não tem introspector. */
export const DB_ENGINES: ReadonlyArray<{ value: DbEngine; label: string; port: number | null }> = [
  { value: 'postgres', label: 'PostgreSQL', port: 5432 },
  { value: 'mysql', label: 'MySQL', port: 3306 },
  { value: 'sqlserver', label: 'SQL Server', port: 1433 },
  { value: 'sqlite', label: 'SQLite', port: null },
];

export interface ConnectFields {
  host: string;
  port: string;
  database: string;
  user: string;
  password: string;
  path: string;
  schemas: string;
}

/** `"a, b ,,c"` -> `['a', 'b', 'c']`. */
export function parseSchemas(input: string): string[] {
  return input
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

export function buildRequest(
  engine: DbEngine,
  fields: ConnectFields,
  trustCert: boolean
): IntrospectRequest {
  const port = Number.parseInt(fields.port.trim(), 10);
  return {
    dialect: engine,
    host: fields.host.trim(),
    port: Number.isInteger(port) && port > 0 && port < 65536 ? port : null,
    database: fields.database.trim(),
    user: fields.user.trim(),
    password: fields.password,
    path: fields.path.trim(),
    trust_server_certificate: engine === 'sqlserver' && trustCert,
    schemas: engine === 'sqlite' ? [] : parseSchemas(fields.schemas),
  };
}

export type ConnectStatus = 'idle' | 'connecting' | 'tested' | 'failed' | 'ready';

/** `Connected to X. Schemas: a, b (+3 more).` */
export function summarizeTest(server: string, schemas: string[]): string {
  const SHOWN = 8;
  if (schemas.length === 0) return `Connected to ${server}.`;
  const shown = schemas.slice(0, SHOWN).join(', ');
  const more = schemas.length - SHOWN;
  return more > 0
    ? `Connected to ${server}. Schemas: ${shown} (+${more} more).`
    : `Connected to ${server}. Schemas: ${shown}.`;
}

/** Estado do modal "Connect to Database"; a senha nunca é guardada aqui. */
export class ConnectDbModel {
  engine: DbEngine;
  trustCert = false;
  status: ConnectStatus = 'idle';
  dbml = '';
  warnings: string[] = [];
  tables = 0;
  error = '';
  confirmReplace = false;

  constructor(engine: DbEngine = 'postgres') {
    this.engine = engine;
  }

  setEngine(engine: DbEngine): void {
    this.engine = engine;
    this.reset();
  }

  reset(): void {
    this.status = 'idle';
    this.dbml = '';
    this.warnings = [];
    this.tables = 0;
    this.error = '';
    this.confirmReplace = false;
  }

  start(): void {
    this.reset();
    this.status = 'connecting';
  }

  finish(result: IntrospectResult): void {
    this.confirmReplace = false;
    if (result.tables === 0 || !result.dbml.trim()) {
      this.fail('Connected, but no tables were found. Check the database and schema.');
      return;
    }
    this.dbml = result.dbml;
    this.warnings = result.warnings;
    this.tables = result.tables;
    this.error = '';
    this.status = 'ready';
  }

  /** Resultado de "Test connection": não gera DBML, então nada pode ser aplicado. */
  finishTest(server: string, schemas: string[]): void {
    this.reset();
    this.error = summarizeTest(server, schemas);
    this.status = 'tested';
  }

  fail(message: string): void {
    this.dbml = '';
    this.warnings = [];
    this.tables = 0;
    this.confirmReplace = false;
    this.error = message;
    this.status = 'failed';
  }

  canApply(): boolean {
    return this.status === 'ready' && this.dbml.trim().length > 0;
  }

  buildMergedSource(existing: string, mode: 'insert' | 'replace'): string {
    if (mode === 'replace' || existing.trim().length === 0) return this.dbml;
    return `${existing.replace(/\s+$/, '')}\n\n${this.dbml}`;
  }
}
