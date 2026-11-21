import { execFile } from 'child_process';

/** Mesmos campos do modal "Connect to Database" (`IntrospectRequest` em `canvas-web`). */
export interface IntrospectRequest {
  dialect: 'postgres' | 'mysql' | 'sqlserver' | 'sqlite';
  host: string;
  port: number | null;
  database: string;
  user: string;
  password: string;
  path: string;
  trust_server_certificate: boolean;
  schemas: string[];
}

export interface IntrospectResult {
  dbml: string;
  warnings: string[];
  tables: number;
}

const DEFAULT_PORT: Record<string, number> = { postgres: 5432, mysql: 3306, sqlserver: 1433 };

/** URL sem a senha: ela vai pelo stdin (`--password-stdin`) e nunca aparece na linha de comando. */
export function buildUrl(req: IntrospectRequest): string {
  if (req.dialect === 'sqlite') {
    if (!req.path.trim()) throw new Error('Choose a SQLite database file.');
    return req.path.trim();
  }
  if (!req.user.trim()) throw new Error('user is required');
  const host = req.host.trim() || 'localhost';
  const hostPart = host.includes(':') && !host.startsWith('[') ? `[${host}]` : host;
  const port = req.port ?? DEFAULT_PORT[req.dialect];
  const scheme = req.dialect;
  let url = `${scheme}://${encodeURIComponent(req.user.trim())}@${hostPart}:${port}`;
  if (req.database.trim()) url += `/${encodeURIComponent(req.database.trim())}`;
  if (req.dialect === 'sqlserver' && req.trust_server_certificate) url += '?trustServerCertificate=true';
  return url;
}

export function buildArgs(req: IntrospectRequest): string[] {
  const args = ['import', '--url', buildUrl(req)];
  if (req.dialect !== 'sqlite') {
    for (const s of req.schemas) args.push('--schema', s);
    if (req.password) args.push('--password-stdin');
  }
  return args;
}

export interface TestConnectionResult {
  server: string;
  schemas: string[];
}

/** Executa `dbml import --url ... --test` e devolve a versão do servidor e os schemas. */
export function runTestConnection(cliPath: string, req: IntrospectRequest): Promise<TestConnectionResult> {
  return runCli(cliPath, ['import', '--url', buildUrl(req), '--test', ...(req.dialect !== 'sqlite' && req.password ? ['--password-stdin'] : [])], req).then(
    (stdout) => JSON.parse(stdout) as TestConnectionResult
  );
}

/** Versão mínima da CLI que a extensão suporta (`import --url/--test/--password-stdin`). */
export const MIN_CLI_VERSION: readonly [number, number, number] = [0, 1, 4];

const INSTALL_HELP =
  'Install it from https://github.com/dbmodeling/dbml/releases (or run the install script from the README) and make sure it is on PATH, or set "dbmodeling.cliPath".';

/** Extrai `x.y.z` da saída de `dbml --version` (ex.: `dbml 0.1.4`). */
export function parseCliVersion(output: string): [number, number, number] | undefined {
  const m = /(\d+)\.(\d+)\.(\d+)/.exec(output);
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : undefined;
}

export function compareVersions(a: readonly number[], b: readonly number[]): number {
  for (let i = 0; i < 3; i++) {
    const d = (a[i] ?? 0) - (b[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

/** Confere que a CLI existe e que a versão é compatível; devolve a versão encontrada. */
export function checkCli(cliPath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(cliPath, ['--version'], { timeout: 10_000 }, (error, stdout) => {
      if (error) {
        const code = (error as NodeJS.ErrnoException).code;
        reject(
          new Error(
            code === 'ENOENT' || code === 'EACCES'
              ? `The dbml CLI ("${cliPath}") was not found. ${INSTALL_HELP}`
              : `Could not read the dbml CLI version from "${cliPath}": ${error.message}`
          )
        );
        return;
      }
      const version = parseCliVersion(stdout);
      if (!version) {
        reject(new Error(`Could not read the dbml CLI version from "${cliPath}" (unexpected output). ${INSTALL_HELP}`));
        return;
      }
      const found = version.join('.');
      if (compareVersions(version, MIN_CLI_VERSION) < 0) {
        reject(new Error(`dbml CLI ${found} is too old; this extension needs ${MIN_CLI_VERSION.join('.')} or newer. ${INSTALL_HELP}`));
        return;
      }
      resolve(found);
    });
  });
}

function runCli(cliPath: string, args: string[], req: IntrospectRequest): Promise<string> {
  return runCliFull(cliPath, args, req).then((r) => r.stdout);
}

function runCliFull(
  cliPath: string,
  args: string[],
  req: IntrospectRequest
): Promise<{ stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    const child = execFile(
      cliPath,
      args,
      { timeout: 60_000, maxBuffer: 64 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error) {
          const code = (error as NodeJS.ErrnoException).code;
          if (code === 'ENOENT') {
            reject(new Error(`Could not run "${cliPath}". ${INSTALL_HELP}`));
            return;
          }
          const msg = stderr.split('\n').find((l) => l.startsWith('Error:'))?.replace(/^Error:\s*/, '');
          reject(new Error(msg || stderr.trim() || error.message));
          return;
        }
        resolve({ stdout, stderr });
      }
    );
    if (req.dialect !== 'sqlite' && req.password) child.stdin?.write(req.password + '\n');
    child.stdin?.end();
  });
}

/** Executa `dbml import --url ...` (somente leitura) e devolve o DBML gerado. */
export async function runIntrospect(cliPath: string, req: IntrospectRequest): Promise<IntrospectResult> {
  const { stdout, stderr } = await runCliFull(cliPath, buildArgs(req), req);
  const warnings = stderr
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l.includes('warning:'));
  const tables = (stdout.match(/^Table\s/gm) ?? []).length;
  return { dbml: stdout, warnings, tables };
}
