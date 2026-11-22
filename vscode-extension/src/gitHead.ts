import { execFile } from 'node:child_process';
import { basename, dirname } from 'node:path';
import {
  GIT_BASE_ERROR,
  GIT_ENV_OPTIONAL_LOCKS,
  GIT_ENV_OPTIONAL_LOCKS_OFF,
  GIT_MAX_BUFFER_BYTES,
  GIT_PROGRAM,
  GIT_TIMEOUT_MS,
} from './constants.js';
import { gitHeadError, type GitHeadResult } from './gitBase.js';

export type GitRunResult = { status: 'unavailable' } | { status: 'exit'; code: number; stdout: Uint8Array };

/** Executa `git <args>` em `cwd` (injetável nos testes). */
export type GitRunner = (cwd: string, args: string[]) => Promise<GitRunResult>;

const text = (bytes: Uint8Array): string => new TextDecoder().decode(bytes).trim();

/** Runner real: `child_process.execFile` com saída binária, sem shell e sem escrever no índice. */
export const nodeGitRunner: GitRunner = (cwd, args) =>
  new Promise((resolve) => {
    execFile(
      GIT_PROGRAM,
      args,
      {
        cwd,
        encoding: 'buffer',
        timeout: GIT_TIMEOUT_MS,
        maxBuffer: GIT_MAX_BUFFER_BYTES,
        env: { ...process.env, [GIT_ENV_OPTIONAL_LOCKS]: GIT_ENV_OPTIONAL_LOCKS_OFF },
      },
      (error, stdout) => {
        if (!error) return resolve({ status: 'exit', code: 0, stdout });
        const code = (error as NodeJS.ErrnoException).code;
        if (code === 'ENOENT' || code === 'EACCES') return resolve({ status: 'unavailable' });
        resolve({ status: 'exit', code: typeof code === 'number' ? code : 1, stdout: stdout ?? new Uint8Array() });
      }
    );
  });

/**
 * Conteúdo de `fsPath` no `HEAD` do repositório que o contém (`git cat-file blob HEAD:./<nome>`).
 * O nome do arquivo só aparece como parte do argumento `HEAD:./…`, nunca como opção, e o git roda
 * na pasta do arquivo, então o caminho relativo vale para qualquer raiz de repositório.
 */
export async function readGitHead(fsPath: string, run: GitRunner = nodeGitRunner): Promise<GitHeadResult> {
  const cwd = dirname(fsPath);
  const inside = await run(cwd, ['rev-parse', '--is-inside-work-tree']);
  if (inside.status === 'unavailable') return gitHeadError(GIT_BASE_ERROR.GIT_UNAVAILABLE);
  if (inside.code !== 0 || text(inside.stdout) !== 'true') return gitHeadError(GIT_BASE_ERROR.NOT_A_REPO);

  const head = await run(cwd, ['rev-parse', '--verify', '--quiet', 'HEAD^{commit}']);
  if (head.status === 'unavailable') return gitHeadError(GIT_BASE_ERROR.GIT_UNAVAILABLE);
  if (head.code !== 0) return gitHeadError(GIT_BASE_ERROR.NO_COMMITS);

  const blob = await run(cwd, ['cat-file', 'blob', `HEAD:./${basename(fsPath)}`]);
  if (blob.status === 'unavailable') return gitHeadError(GIT_BASE_ERROR.GIT_UNAVAILABLE);
  if (blob.code !== 0) return gitHeadError(GIT_BASE_ERROR.UNTRACKED);
  try {
    return { ok: true, source: new TextDecoder('utf-8', { fatal: true }).decode(blob.stdout) };
  } catch {
    return gitHeadError(GIT_BASE_ERROR.NOT_UTF8);
  }
}
