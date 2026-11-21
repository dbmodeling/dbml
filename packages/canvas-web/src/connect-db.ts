import { t } from './i18n';
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

/** Prazo máximo de um teste/leitura de catálogo antes de abortar. */
export const INTROSPECT_TIMEOUT_MS = 60_000;

/**
 * Controla uma chamada de introspecção: timeout e cancelamento pelo usuário
 * disparam o mesmo `AbortSignal`, e `race` rejeita mesmo que o transporte
 * ignore o sinal.
 */
export class IntrospectGuard {
  private readonly controller = new AbortController();
  private readonly timer: ReturnType<typeof setTimeout>;
  timedOut = false;
  cancelled = false;

  constructor(timeoutMs: number = INTROSPECT_TIMEOUT_MS) {
    this.timer = setTimeout(() => {
      this.timedOut = true;
      this.controller.abort();
    }, timeoutMs);
  }

  get signal(): AbortSignal {
    return this.controller.signal;
  }

  cancel(): void {
    this.cancelled = true;
    this.controller.abort();
  }

  dispose(): void {
    clearTimeout(this.timer);
  }

  race<T>(promise: Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const abort = () => reject(new Error(this.timedOut ? 'Introspection timed out' : 'Introspection cancelled'));
      if (this.signal.aborted) return abort();
      this.signal.addEventListener('abort', abort, { once: true });
      promise.then(resolve, reject).finally(() => this.signal.removeEventListener('abort', abort));
    });
  }
}

export type ConnectStatus = 'idle' | 'connecting' | 'tested' | 'failed' | 'ready';

/** `Conectado a X. Schemas: a, b (+3 a mais).` */
export function summarizeTest(server: string, schemas: string[]): string {
  const SHOWN = 8;
  if (schemas.length === 0) return t('Conectado a {}.', server);
  const shown = schemas.slice(0, SHOWN).join(', ');
  const more = schemas.length - SHOWN;
  return more > 0
    ? t('Conectado a {}. Schemas: {} (+{} a mais).', server, shown, more)
    : t('Conectado a {}. Schemas: {}.', server, shown);
}

/** Estado do modal "Conectar ao banco"; a senha nunca é guardada aqui. */
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
      this.fail(t('Conectado, mas nenhuma tabela foi encontrada. Verifique o banco e o schema.'));
      return;
    }
    this.dbml = result.dbml;
    this.warnings = result.warnings;
    this.tables = result.tables;
    this.error = '';
    this.status = 'ready';
  }

  /** Resultado de "Testar conexão": não gera DBML, então nada pode ser aplicado. */
  finishTest(server: string, schemas: string[]): void {
    this.reset();
    this.error = summarizeTest(server, schemas);
    this.status = 'tested';
  }

  /** O usuário desistiu da chamada em curso. */
  cancel(): void {
    this.reset();
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
