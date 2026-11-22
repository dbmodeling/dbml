import { describe, expect, it } from 'vitest';
import { parseWebviewMessage } from './messages.js';

const pos = (line: number, character: number) => ({ line, character });
const range = { start: pos(0, 0), end: pos(0, 3) };
const request = {
  dialect: 'postgres',
  host: 'h',
  port: 5432,
  database: 'd',
  user: 'u',
  password: 'p',
  path: '',
  trust_server_certificate: false,
  schemas: ['public'],
};

function ok(raw: unknown) {
  const r = parseWebviewMessage(raw);
  if (!r.ok) throw new Error(`esperava válida: ${r.reason}`);
  return r.message;
}

function bad(raw: unknown) {
  const r = parseWebviewMessage(raw);
  expect(r.ok).toBe(false);
  return r as Extract<typeof r, { ok: false }>;
}

describe('parseWebviewMessage', () => {
  it('rejeita o que não é objeto ou não tem command conhecido', () => {
    for (const raw of [null, undefined, 'x', 42, [], {}, { command: 5 }, { command: 'rm_rf' }]) bad(raw);
  });

  it('rejeita id que não é número finito', () => {
    bad({ command: 'ready', id: 'x' });
    bad({ command: 'ready', id: NaN });
  });

  it('git_base (modo Comparar) é um comando sem payload, com id', () => {
    expect(ok({ command: 'git_base', id: 4 })).toEqual({ command: 'git_base', id: 4 });
    expect(ok({ command: 'git_base' })).toEqual({ command: 'git_base', id: undefined });
  });

  it('comandos sem payload passam', () => {
    for (const command of ['ready', 'new_file', 'check_cli']) expect(ok({ command, id: 1 }).command).toBe(command);
  });

  describe('apply_patch', () => {
    const patch = { range, new_text: 'x', expected_source: 'abc' };
    it('aceita patch válido', () => {
      expect(ok({ command: 'apply_patch', id: 1, base_version: 4, patch })).toMatchObject({ command: 'apply_patch', base_version: 4, patch });
    });
    it('exige base_version inteiro não negativo', () => {
      for (const base_version of [undefined, -1, 1.5, '3', null]) {
        expect(bad({ command: 'apply_patch', id: 1, base_version, patch }).reason).toMatch(/base_version/);
      }
    });
    it('rejeita patch malformado e preserva o id para resposta de erro', () => {
      const r = bad({ command: 'apply_patch', id: 3, base_version: 1, patch: { ...patch, new_text: 5 } });
      expect(r.id).toBe(3);
      bad({ command: 'apply_patch', base_version: 1, patch: null });
      bad({ command: 'apply_patch', base_version: 1, patch: { ...patch, expected_source: undefined } });
      bad({ command: 'apply_patch', base_version: 1, patch: { ...patch, range: { start: pos(-1, 0), end: pos(0, 0) } } });
      bad({ command: 'apply_patch', base_version: 1, patch: { ...patch, range: { start: pos(0, 1.5), end: pos(0, 2) } } });
      bad({ command: 'apply_patch', base_version: 1, patch: { ...patch, range: { start: pos(0, 0) } } });
    });
  });

  describe('save_document', () => {
    it('exige expected_source string e base_version', () => {
      ok({ command: 'save_document', base_version: 1, expected_source: '' });
      bad({ command: 'save_document', base_version: 1 });
      bad({ command: 'save_document', base_version: 1, expected_source: 1 });
      bad({ command: 'save_document', expected_source: '' });
      bad({ command: 'save_document', base_version: -2, expected_source: '' });
    });
  });

  describe('save_layout', () => {
    it('exige layout objeto', () => {
      ok({ command: 'save_layout', layout: { a: 1 } });
      bad({ command: 'save_layout', layout: null });
      bad({ command: 'save_layout', layout: [] });
      bad({ command: 'save_layout', layout: 'x' });
    });
  });

  describe('diagnostics', () => {
    const d = { severity: 'error', message: 'm', range };
    it('aceita lista válida (inclusive vazia)', () => {
      ok({ command: 'diagnostics', diagnostics: [d, { ...d, severity: 'warning' }] });
      ok({ command: 'diagnostics', diagnostics: [] });
    });
    it('rejeita lista malformada', () => {
      bad({ command: 'diagnostics' });
      bad({ command: 'diagnostics', diagnostics: 'x' });
      bad({ command: 'diagnostics', diagnostics: [{ ...d, message: 1 }] });
      bad({ command: 'diagnostics', diagnostics: [{ ...d, range: {} }] });
      bad({ command: 'diagnostics', diagnostics: [null] });
    });
    it('limita a quantidade', () => {
      bad({ command: 'diagnostics', diagnostics: Array.from({ length: 10_001 }, () => d) });
    });
  });

  describe('export_file', () => {
    it('aceita texto e binário', () => {
      ok({ command: 'export_file', id: 1, filename: 'a.svg', content: '<svg/>', isBinary: false });
      ok({ command: 'export_file', filename: 'a.png', content: 'AAAA', isBinary: true });
    });
    it('rejeita nome com separador de caminho, vazio ou longo demais', () => {
      for (const filename of ['../x.svg', 'a/b.svg', 'a\\b.svg', '', ' ', 'x'.repeat(300), 5]) {
        bad({ command: 'export_file', filename, content: '', isBinary: false });
      }
    });
    it('rejeita content/isBinary com tipo errado', () => {
      bad({ command: 'export_file', filename: 'a', content: 1, isBinary: false });
      bad({ command: 'export_file', filename: 'a', content: '', isBinary: 'sim' });
    });
  });

  describe('test_connection / introspect', () => {
    it('aceita request válido', () => {
      ok({ command: 'introspect', id: 1, request });
      ok({ command: 'test_connection', id: 1, request: { ...request, port: null, dialect: 'sqlite' } });
    });
    it('rejeita dialeto/campos inválidos', () => {
      const cases = [
        { ...request, dialect: 'oracle' },
        { ...request, host: 5 },
        { ...request, port: '5432' },
        { ...request, port: 70000 },
        { ...request, trust_server_certificate: 'sim' },
        { ...request, schemas: 'public' },
        { ...request, schemas: [1] },
        { ...request, password: undefined },
      ];
      for (const r of cases) bad({ command: 'introspect', id: 1, request: r });
      bad({ command: 'test_connection', request: null });
    });
  });

  describe('open_file', () => {
    it('exige path string não vazio', () => {
      ok({ command: 'open_file', path: 'file:///a.dbml' });
      bad({ command: 'open_file' });
      bad({ command: 'open_file', path: '' });
      bad({ command: 'open_file', path: { toString: () => 'x' } });
    });
  });

  describe('save_session', () => {
    it('exige session objeto', () => {
      ok({ command: 'save_session', session: { zoom: 1 } });
      bad({ command: 'save_session', session: 'x' });
      bad({ command: 'save_session' });
    });
  });

  describe('save_guide_state', () => {
    it('aceita estado válido com versão string ou null', () => {
      ok({ command: 'save_guide_state', state: { onboardingDone: true, lastSeenVersion: '0.2.0' } });
      ok({ command: 'save_guide_state', state: { onboardingDone: false, lastSeenVersion: null } });
    });
    it('rejeita payloads malformados', () => {
      bad({ command: 'save_guide_state' });
      bad({ command: 'save_guide_state', state: null });
      bad({ command: 'save_guide_state', state: { onboardingDone: 'true', lastSeenVersion: '0.2.0' } });
      bad({ command: 'save_guide_state', state: { onboardingDone: true, lastSeenVersion: 123 } });
      bad({ command: 'save_guide_state', state: { onboardingDone: true, lastSeenVersion: 'x'.repeat(40) } });
    });
  });
});
