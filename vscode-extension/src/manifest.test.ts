import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const manifest = JSON.parse(readFileSync(fileURLToPath(new URL('../package.json', import.meta.url)), 'utf-8'));

describe('manifesto da extensão', () => {
  it('não ativa em toda janela (sem onStartupFinished nem eventos implícitos)', () => {
    const events: string[] = manifest.activationEvents ?? [];
    expect(events).not.toContain('onStartupFinished');
    expect(events.some((e) => /^(onCommand|onView|onLanguage|onCustomEditor):/.test(e))).toBe(false);
    expect(events.includes('*')).toBe(false);
  });

  it('a versão mínima do VS Code suporta eventos de ativação implícitos (>= 1.74)', () => {
    const [major, minor] = String(manifest.engines.vscode).replace(/^[^\d]*/, '').split('.').map(Number);
    expect(major > 1 || minor >= 74).toBe(true);
  });
});
