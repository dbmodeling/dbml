import { chmodSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { checkCli, compareVersions, MIN_CLI_VERSION, parseCliVersion } from './introspect.js';

describe('parseCliVersion', () => {
  it('extrai x.y.z da saída de --version', () => {
    expect(parseCliVersion('dbml 0.1.4\n')).toEqual([0, 1, 4]);
    expect(parseCliVersion('dbml 1.20.3-beta.1')).toEqual([1, 20, 3]);
  });
  it('devolve undefined para saída sem versão', () => {
    expect(parseCliVersion('')).toBeUndefined();
    expect(parseCliVersion('command not found')).toBeUndefined();
  });
});

describe('compareVersions', () => {
  it('compara numericamente', () => {
    expect(compareVersions([0, 1, 4], [0, 1, 4])).toBe(0);
    expect(compareVersions([0, 1, 10], [0, 1, 4])).toBeGreaterThan(0);
    expect(compareVersions([0, 9, 0], [0, 10, 0])).toBeLessThan(0);
    expect(compareVersions([1, 0, 0], [0, 99, 99])).toBeGreaterThan(0);
  });
});

describe.skipIf(process.platform === 'win32')('checkCli', () => {
  const dir = mkdtempSync(join(tmpdir(), 'dbm-cli-'));
  const script = (name: string, body: string) => {
    const path = join(dir, name);
    writeFileSync(path, `#!/bin/sh\n${body}\n`);
    chmodSync(path, 0o755);
    return path;
  };
  const current = MIN_CLI_VERSION.join('.');

  it('aceita CLI com versão suficiente', async () => {
    await expect(checkCli(script('ok', `echo "dbml ${current}"`))).resolves.toBe(current);
    await expect(checkCli(script('newer', 'echo "dbml 99.0.0"'))).resolves.toBe('99.0.0');
  });

  it('recusa CLI antiga com orientação de atualização', async () => {
    const err = await checkCli(script('old', 'echo "dbml 0.0.1"')).catch((e: Error) => e);
    expect(err).toBeInstanceOf(Error);
    expect((err as Error).message).toMatch(/0\.0\.1/);
    expect((err as Error).message).toContain(current);
    expect((err as Error).message).toMatch(/github\.com\/dbmodeling\/dbml\/releases/);
  });

  it('recusa binário que não imprime versão', async () => {
    await expect(checkCli(script('weird', 'echo hello'))).rejects.toThrow(/version/i);
  });

  it('recusa binário que falha em --version', async () => {
    await expect(checkCli(script('broken', 'exit 3'))).rejects.toThrow(/version/i);
  });

  it('CLI ausente orienta a instalação', async () => {
    await expect(checkCli(join(dir, 'nao-existe'))).rejects.toThrow(/releases/);
  });
});
