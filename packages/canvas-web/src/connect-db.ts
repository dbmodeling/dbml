import {
  CONNECT_URL_ERROR,
  EDIT_ORIGIN,
  ENGINE_SCHEME,
  PASSWORD_MASK,
  SQLSERVER_TRUST_QUERY,
  TLS_MODE,
  TLS_QUERY,
} from './constants';
import { t } from './i18n';
import type {
  ConnectionUrlErrorCode,
  IntrospectRequest,
  IntrospectResult,
  ParsedConnectionUrl,
  TlsMode,
} from './types';

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
  trustCert: boolean,
  tls: TlsMode = TLS_MODE.AUTO
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
    tls: engine === 'sqlite' ? TLS_MODE.AUTO : tls,
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

export type EditOrigin = (typeof EDIT_ORIGIN)[keyof typeof EDIT_ORIGIN];

/** Lê uma connection string (Wasm `parse_connection_url`); lança `ConnectionUrlError`. */
export type UrlParser = (url: string) => ParsedConnectionUrl;

const KNOWN_URL_ERRORS: ReadonlySet<string> = new Set(Object.values(CONNECT_URL_ERROR));

/** Mensagem curta, por código, da string inválida (as mesmas chaves do Desktop). */
export function urlErrorMessage(code: ConnectionUrlErrorCode): string {
  switch (code) {
    case CONNECT_URL_ERROR.EMPTY: return t('Informe a connection string.');
    case CONNECT_URL_ERROR.UNKNOWN_SCHEME: return t('Esquema desconhecido. Use postgres://, mysql://, sqlserver:// ou um arquivo SQLite.');
    case CONNECT_URL_ERROR.UNSUPPORTED_ENGINE: return t('Este banco ainda não é suportado.');
    case CONNECT_URL_ERROR.MISSING_HOST: return t('Informe o host na connection string.');
    case CONNECT_URL_ERROR.INVALID_PORT: return t('Porta inválida. Use um número de 1 a 65535.');
    case CONNECT_URL_ERROR.MALFORMED: return t('Connection string malformada. Formato: esquema://usuário:senha@host:porta/banco.');
  }
}

/** Percent-encode igual ao do núcleo: só `A-Za-z0-9-._~` ficam como estão. */
function percentEncode(value: string): string {
  return encodeURIComponent(value).replace(
    /[!'()*]/g,
    (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`
  );
}

function validPort(text: string): number | null {
  const port = Number.parseInt(text.trim(), 10);
  return Number.isInteger(port) && port > 0 && port < 65536 ? port : null;
}

function tlsQuery(engine: Exclude<DbEngine, 'sqlite'>, tls: TlsMode, trustCert: boolean): string | null {
  if (tls !== TLS_MODE.AUTO) return TLS_QUERY[engine][tls];
  return engine === 'sqlserver' && trustCert ? SQLSERVER_TRUST_QUERY : null;
}

/**
 * Connection string montada a partir dos campos, só para exibição e cópia (o pedido real
 * usa `buildRequest`). Espelha `ConnectionParams::to_url`: usuário, senha e banco em
 * percent-encoding, IPv6 entre colchetes, TLS na query. Diferenças deliberadas: a porta só
 * aparece se foi digitada, e um formulário em branco gera texto vazio.
 */
export function buildDisplayUrl(
  engine: DbEngine,
  fields: ConnectFields,
  tls: TlsMode,
  trustCert: boolean,
  maskPassword: boolean
): string {
  if (engine === 'sqlite') return fields.path.trim();
  const host = fields.host.trim();
  const user = fields.user.trim();
  const database = fields.database.trim();
  const port = validPort(fields.port);
  const query = tlsQuery(engine, tls, trustCert);
  if (!host && !user && !database && !fields.password && port === null && !query) return '';
  const shownHost = host === '' ? 'localhost' : host.includes(':') && !host.startsWith('[') ? `[${host}]` : host;
  let url = `${ENGINE_SCHEME[engine]}://`;
  if (user || fields.password) {
    url += percentEncode(user);
    if (fields.password) url += `:${maskPassword ? PASSWORD_MASK : percentEncode(fields.password)}`;
    url += '@';
  }
  url += shownHost;
  if (port !== null) url += `:${port}`;
  if (database) url += `/${percentEncode(database)}`;
  if (query) url += `?${query}`;
  return url;
}

/** Próxima opção habilitada do dropdown (`1`/`-1` andam; `first`/`last` saltam). */
export function nextEnabledOption(
  options: ReadonlyArray<{ disabled: boolean }>,
  from: number,
  step: 1 | -1 | 'first' | 'last'
): number {
  const enabled = options.map((o, i) => (o.disabled ? -1 : i)).filter((i) => i >= 0);
  if (enabled.length === 0) return from;
  if (step === 'first') return enabled[0];
  if (step === 'last') return enabled[enabled.length - 1];
  const next = step === 1 ? enabled.find((i) => i > from) : [...enabled].reverse().find((i) => i < from);
  return next ?? from;
}

const emptyFields = (): ConnectFields => ({
  host: '', port: '', database: '', user: '', password: '', path: '', schemas: '',
});

/**
 * Estado do modal "Conectar ao banco". Guarda só em memória os campos, inclusive a senha;
 * nada aqui é persistido e a revelação da senha vale apenas enquanto o diálogo existir.
 */
export class ConnectDbModel {
  engine: DbEngine;
  fields: ConnectFields = emptyFields();
  trustCert = false;
  tls: TlsMode = TLS_MODE.AUTO;
  /** Texto do campo Connection string (mascarado, salvo com a senha revelada). */
  urlText = '';
  /** Última edição: campo reescreve a string; string reescreve os campos. */
  origin: EditOrigin = EDIT_ORIGIN.FIELD;
  urlError: ConnectionUrlErrorCode | null = null;
  passwordRevealed = false;
  status: ConnectStatus = 'idle';
  dbml = '';
  warnings: string[] = [];
  tables = 0;
  error = '';
  confirmReplace = false;

  constructor(engine: DbEngine = 'postgres', private readonly parser?: UrlParser) {
    this.engine = engine;
  }

  get canParse(): boolean {
    return this.parser !== undefined;
  }

  /** Dica do campo da string enquanto ele está vazio. */
  get urlPlaceholder(): string {
    return this.engine === 'sqlite'
      ? t('/caminho/para/banco.sqlite')
      : `${ENGINE_SCHEME[this.engine]}://user:password@host:port/database`;
  }

  /** Troca o banco no dropdown: mantém os dados digitados e volta a senha a oculto. */
  setEngine(engine: DbEngine): void {
    this.engine = engine;
    this.reset();
    this.passwordRevealed = false;
    this.rewriteUrl();
  }

  setField(name: keyof ConnectFields, value: string): void {
    this.fields[name] = value;
    this.rewriteUrl();
  }

  setTls(tls: TlsMode): void {
    this.tls = tls;
    this.rewriteUrl();
  }

  setTrustCert(trust: boolean): void {
    this.trustCert = trust;
    this.rewriteUrl();
  }

  /** O usuário editou a string: ela vira a fonte e os campos seguem, sem laço. */
  setUrl(text: string): void {
    this.urlText = text;
    this.origin = EDIT_ORIGIN.STRING;
    if (!this.parser || text.trim() === '') {
      this.urlError = null;
      return;
    }
    try {
      this.applyParsed(this.parser(text));
      this.urlError = null;
    } catch (e) {
      const code = (e as { code?: unknown } | null)?.code;
      this.urlError =
        typeof code === 'string' && KNOWN_URL_ERRORS.has(code)
          ? (code as ConnectionUrlErrorCode)
          : CONNECT_URL_ERROR.MALFORMED;
    }
  }

  /** Fim da edição da string: troca uma senha real digitada pelo marcador. */
  commitUrl(): void {
    if (this.origin !== EDIT_ORIGIN.STRING || this.urlError || this.passwordRevealed) return;
    if (this.urlText.trim() === '') return;
    this.urlText = this.assembled(true);
  }

  togglePassword(): void {
    if (this.passwordRevealed) this.hidePassword();
    else this.revealPassword();
  }

  revealPassword(): void {
    this.passwordRevealed = true;
    this.refreshUrlText();
  }

  hidePassword(): void {
    this.passwordRevealed = false;
    this.refreshUrlText();
  }

  /** String para o botão Copiar: mascarada, exceto com a senha revelada. */
  urlForCopy(): string {
    return this.assembled(!this.passwordRevealed);
  }

  buildRequest(): IntrospectRequest {
    return buildRequest(this.engine, this.fields, this.trustCert, this.tls);
  }

  /** Fechar o diálogo descarta a revelação. */
  close(): void {
    this.passwordRevealed = false;
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
    this.hidePassword();
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

  private assembled(mask: boolean): string {
    return buildDisplayUrl(this.engine, this.fields, this.tls, this.trustCert, mask);
  }

  /** Edição de campo: os campos mandam, então a string é refeita e qualquer erro some. */
  private rewriteUrl(): void {
    this.origin = EDIT_ORIGIN.FIELD;
    this.urlError = null;
    this.urlText = this.assembled(!this.passwordRevealed);
  }

  /** Mudou só a máscara: refaz o texto, sem atropelar uma string inválida em edição. */
  private refreshUrlText(): void {
    if (this.urlError === null) this.urlText = this.assembled(!this.passwordRevealed);
  }

  private applyParsed(parsed: ParsedConnectionUrl): void {
    if (parsed.dialect !== this.engine) {
      this.engine = parsed.dialect;
      this.reset();
    }
    if (parsed.dialect === 'sqlite') {
      this.fields.path = parsed.path;
      return;
    }
    const f = this.fields;
    f.host = parsed.host;
    f.port = parsed.port === null ? '' : String(parsed.port);
    f.database = parsed.database;
    f.user = parsed.user;
    if (!parsed.password_masked) f.password = parsed.password;
    if (parsed.schemas.length > 0) f.schemas = parsed.schemas.join(', ');
    this.trustCert = parsed.trust_server_certificate;
    this.tls = parsed.tls;
  }
}
