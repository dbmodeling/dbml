import { describe, expect, it } from 'vitest';
import { isAllowedDbmlUri } from './uriPolicy.js';

const folder = (s: string) => {
  const m = /^([\w-]+):\/\/([^/]*)(\/.*)$/.exec(s)!;
  return { scheme: m[1], authority: m[2], path: m[3] };
};
const u = folder;
const ws = [folder('file:///home/me/proj')];

describe('isAllowedDbmlUri', () => {
  it('aceita .dbml dentro de uma pasta do workspace', () => {
    expect(isAllowedDbmlUri(u('file:///home/me/proj/a.dbml'), ws)).toBe(true);
    expect(isAllowedDbmlUri(u('file:///home/me/proj/sub/dir/B.DBML'), ws)).toBe(true);
  });

  it('recusa extensão diferente de .dbml', () => {
    expect(isAllowedDbmlUri(u('file:///home/me/proj/a.txt'), ws)).toBe(false);
    expect(isAllowedDbmlUri(u('file:///home/me/proj/a.dbml.json'), ws)).toBe(false);
  });

  it('recusa fora do workspace, inclusive prefixo parecido', () => {
    expect(isAllowedDbmlUri(u('file:///etc/a.dbml'), ws)).toBe(false);
    expect(isAllowedDbmlUri(u('file:///home/me/proj-evil/a.dbml'), ws)).toBe(false);
  });

  it('recusa travessia com ..', () => {
    expect(isAllowedDbmlUri(u('file:///home/me/proj/../other/a.dbml'), ws)).toBe(false);
  });

  it('recusa esquema ou autoridade diferente', () => {
    expect(isAllowedDbmlUri(u('http://evil.example/home/me/proj/a.dbml'), ws)).toBe(false);
    expect(isAllowedDbmlUri(u('vscode-remote://host/home/me/proj/a.dbml'), ws)).toBe(false);
    expect(isAllowedDbmlUri(u('command://x/home/me/proj/a.dbml'), ws)).toBe(false);
  });

  it('funciona com workspace virtual', () => {
    const v = [{ scheme: 'vscode-vfs', authority: 'github', path: '/o/r' }];
    expect(isAllowedDbmlUri({ scheme: 'vscode-vfs', authority: 'github', path: '/o/r/a.dbml' }, v)).toBe(true);
  });

  it('sem pastas no workspace nada é permitido', () => {
    expect(isAllowedDbmlUri(u('file:///home/me/proj/a.dbml'), [])).toBe(false);
  });
});
