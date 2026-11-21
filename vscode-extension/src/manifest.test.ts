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

  it('o editor customizado não assume todo .dbml (priority option)', () => {
    expect(manifest.contributes.customEditors[0].priority).toBe('option');
  });

  it('expõe dbmodeling.defaultEditor (text|canvas) com padrão text, só no nível do usuário', () => {
    const prop = manifest.contributes.configuration.properties['dbmodeling.defaultEditor'];
    expect(prop.enum).toEqual(['text', 'canvas']);
    expect(prop.default).toBe('text');
    expect(prop.scope).toBe('application');
  });

  it('há botão na barra de título para abrir o canvas', () => {
    const item = manifest.contributes.menus['editor/title'].find((m: { command: string }) => m.command === 'dbmodeling.openCanvas');
    expect(item?.when).toContain('.dbml');
  });

  it('tem metadados de repositório e o empacotamento não suprime o aviso do vsce', () => {
    expect(manifest.repository.url).toBe('https://github.com/dbmodeling/dbml.git');
    expect(manifest.bugs.url).toMatch(/^https:\/\/github\.com\/dbmodeling\/dbml\/issues/);
    expect(manifest.homepage).toMatch(/^https:\/\/github\.com\/dbmodeling\/dbml/);
    expect(manifest.scripts.package).not.toContain('--allow-missing-repository');
  });

  it('declara suporte limitado a workspaces virtuais e entrada para o extension host web', () => {
    expect(manifest.capabilities.virtualWorkspaces.supported).toBe('limited');
    expect(manifest.browser).toBe('./dist/extension.web.js');
    expect(manifest.main).toBe('./dist/extension.cjs');
  });

  it('a versão mínima do VS Code suporta eventos de ativação implícitos (>= 1.74)', () => {
    const [major, minor] = String(manifest.engines.vscode).replace(/^[^\d]*/, '').split('.').map(Number);
    expect(major > 1 || minor >= 74).toBe(true);
  });
});
