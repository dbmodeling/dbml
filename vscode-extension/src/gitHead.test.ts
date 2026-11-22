import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { GIT_BASE_ERROR } from './constants.js';
import { nodeGitRunner, readGitHead, type GitRunner } from './gitHead.js';

const enc = (s: string) => new TextEncoder().encode(s);
const ok = (stdout: string) => ({ status: 'exit' as const, code: 0, stdout: enc(stdout) });
const fail = { status: 'exit' as const, code: 128, stdout: enc('') };

/** Runner falso que responde por subcomando do git. */
function runner(answers: Record<string, ReturnType<typeof ok> | typeof fail | { status: 'unavailable' }>): GitRunner & { calls: string[][] } {
  const calls: string[][] = [];
  const run = (async (_cwd: string, args: string[]) => {
    calls.push(args);
    const key = args[0] === 'rev-parse' ? args.slice(0, 2).join(' ') : args[0];
    return answers[key] ?? fail;
  }) as GitRunner & { calls: string[][] };
  run.calls = calls;
  return run;
}

describe('readGitHead (runner falso)', () => {
  it('devolve o conteúdo do blob HEAD:./<arquivo> rodando na pasta do arquivo', async () => {
    const run = runner({
      'rev-parse --is-inside-work-tree': ok('true\n'),
      'rev-parse --verify': ok('abc\n'),
      'cat-file': ok('Table a { id int }\n'),
    });
    await expect(readGitHead('/ws/sub/schema.dbml', run)).resolves.toEqual({ ok: true, source: 'Table a { id int }\n' });
    expect(run.calls.at(-1)).toEqual(['cat-file', 'blob', 'HEAD:./schema.dbml']);
  });

  it('sem executável git', async () => {
    const run = runner({ 'rev-parse --is-inside-work-tree': { status: 'unavailable' } });
    await expect(readGitHead('/ws/a.dbml', run)).resolves.toMatchObject({ ok: false, code: GIT_BASE_ERROR.GIT_UNAVAILABLE });
  });

  it('fora de repositório (falha ou "false")', async () => {
    await expect(readGitHead('/ws/a.dbml', runner({}))).resolves.toMatchObject({ ok: false, code: GIT_BASE_ERROR.NOT_A_REPO });
    const bare = runner({ 'rev-parse --is-inside-work-tree': ok('false\n') });
    await expect(readGitHead('/ws/a.dbml', bare)).resolves.toMatchObject({ ok: false, code: GIT_BASE_ERROR.NOT_A_REPO });
  });

  it('repositório sem commits', async () => {
    const run = runner({ 'rev-parse --is-inside-work-tree': ok('true\n') });
    await expect(readGitHead('/ws/a.dbml', run)).resolves.toMatchObject({ ok: false, code: GIT_BASE_ERROR.NO_COMMITS });
  });

  it('arquivo novo (não está no HEAD)', async () => {
    const run = runner({ 'rev-parse --is-inside-work-tree': ok('true\n'), 'rev-parse --verify': ok('abc\n') });
    await expect(readGitHead('/ws/a.dbml', run)).resolves.toMatchObject({ ok: false, code: GIT_BASE_ERROR.UNTRACKED });
  });

  it('blob que não é UTF-8', async () => {
    const run = runner({
      'rev-parse --is-inside-work-tree': ok('true\n'),
      'rev-parse --verify': ok('abc\n'),
      'cat-file': { status: 'exit', code: 0, stdout: new Uint8Array([0xff, 0xfe, 0x41]) },
    });
    await expect(readGitHead('/ws/a.dbml', run)).resolves.toMatchObject({ ok: false, code: GIT_BASE_ERROR.NOT_UTF8 });
  });

  it('o nome do arquivo nunca vira opção do git', async () => {
    const run = runner({
      'rev-parse --is-inside-work-tree': ok('true\n'),
      'rev-parse --verify': ok('abc\n'),
      'cat-file': ok('x'),
    });
    await readGitHead('/ws/--output=x.dbml', run);
    expect(run.calls.at(-1)).toEqual(['cat-file', 'blob', 'HEAD:./--output=x.dbml']);
  });
});

describe('readGitHead (git real)', () => {
  const dirs: string[] = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
  });
  const git = (cwd: string, ...args: string[]) =>
    execFileSync('git', ['-C', cwd, '-c', 'user.name=t', '-c', 'user.email=t@t', '-c', 'commit.gpgsign=false', ...args], { stdio: 'ignore' });

  it('lê o HEAD, não a árvore de trabalho, e distingue arquivo novo', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'dbml-git-'));
    dirs.push(dir);
    git(dir, 'init', '-q');
    mkdirSync(join(dir, 'sub dir'));
    const file = join(dir, 'sub dir', 'schema.dbml');
    writeFileSync(file, 'Table a { id int }\n');
    git(dir, 'add', '.');
    git(dir, 'commit', '-q', '-m', 'init');
    writeFileSync(file, 'Table a { id int\n  x text }\n');
    const fresh = join(dir, 'sub dir', 'novo.dbml');
    writeFileSync(fresh, 'Table n { id int }\n');

    await expect(readGitHead(file, nodeGitRunner)).resolves.toEqual({ ok: true, source: 'Table a { id int }\n' });
    await expect(readGitHead(fresh, nodeGitRunner)).resolves.toMatchObject({ ok: false, code: GIT_BASE_ERROR.UNTRACKED });
  });

  it('pasta sem repositório', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'dbml-nogit-'));
    dirs.push(dir);
    const file = join(dir, 'a.dbml');
    writeFileSync(file, 'x');
    const res = await readGitHead(file, nodeGitRunner);
    // /tmp dentro de um repositório é um cenário exótico: só valida quando não há repo acima
    if (!res.ok) expect([GIT_BASE_ERROR.NOT_A_REPO, GIT_BASE_ERROR.UNTRACKED, GIT_BASE_ERROR.NO_COMMITS]).toContain(res.code);
  });
});
