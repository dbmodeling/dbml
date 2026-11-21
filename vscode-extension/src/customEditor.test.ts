import { beforeEach, describe, expect, it } from 'vitest';
import { DbmlCustomEditorProvider } from './customEditor.js';
import { openEditor } from './test/harness.js';
import { mockState, resetMock, Uri } from './test/vscode-mock.js';

beforeEach(() => resetMock());

describe('DbmlCustomEditorProvider', () => {
  it('ao abrir um .dbml e receber "ready", envia "update" com o texto', async () => {
    mockState.files.set('file:///ws/schema.dbml', new TextEncoder().encode('x'));
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'Table users {\n  id int\n}\n' });
    await h.send({ command: 'ready' });

    const update = h.posted.find((m) => m.type === 'update');
    expect(update).toBeDefined();
    expect(update.source).toBe('Table users {\n  id int\n}\n');
    expect(update.documentId).toBe('file:///ws/schema.dbml');
    expect(update.saved).toBe(true);
    expect(update.files.map((f: { name: string }) => f.name)).toContain('schema.dbml');
  });

  it('carrega o sidecar de layout existente no primeiro update', async () => {
    mockState.files.set(
      'file:///ws/schema.dbml.layout.json',
      new TextEncoder().encode(JSON.stringify({ positions: { users: { x: 1, y: 2 } } }))
    );
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'ready' });
    expect(h.posted.find((m) => m.type === 'update').layout).toEqual({ positions: { users: { x: 1, y: 2 } } });
  });

  it('apply_patch aplica o patch e confirma com "result"', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    await h.send({
      command: 'apply_patch',
      id: 7,
      patch: {
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
        new_text: 'X',
        expected_source: 'abc',
      },
    });
    expect(h.posted).toContainEqual({ type: 'result', id: 7 });
  });

  it('apply_patch com texto desatualizado responde erro', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    await h.send({
      command: 'apply_patch',
      id: 8,
      patch: {
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
        new_text: 'X',
        expected_source: 'outro texto',
      },
    });
    const result = h.posted.find((m) => m.type === 'result' && m.id === 8);
    expect(result.error).toMatch(/changed outside/);
  });
});

describe('validação das mensagens da Webview', () => {
  it('apply_patch malformado responde erro e não toca no documento', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    await h.send({ command: 'apply_patch', id: 1, patch: { range: null, new_text: 1 } });
    expect(h.posted.find((m) => m.type === 'result' && m.id === 1).error).toMatch(/invalid patch/);
  });

  it('diagnostics malformado é descartado', async () => {
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'diagnostics', diagnostics: [{ severity: 'error', message: 'x' }] });
    await h.send({ command: 'diagnostics', diagnostics: 'x' });
    expect(h.diagnosticSets).toHaveLength(0);
  });

  it('diagnostics válido vira Diagnostic', async () => {
    const h = await openEditor(DbmlCustomEditorProvider);
    const range = { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } };
    await h.send({ command: 'diagnostics', diagnostics: [{ severity: 'warning', message: 'm', range }] });
    expect(h.diagnosticSets).toHaveLength(1);
    expect(h.diagnosticSets[0].list).toHaveLength(1);
  });

  it('export_file com nome fora do padrão não abre diálogo nem grava', async () => {
    const h = await openEditor(DbmlCustomEditorProvider);
    mockState.saveDialogResult = Uri.file('/tmp/out.svg');
    await h.send({ command: 'export_file', id: 2, filename: '../../etc/passwd', content: 'x', isBinary: false });
    expect(mockState.files.has('file:///tmp/out.svg')).toBe(false);
    expect(h.posted.find((m) => m.type === 'result' && m.id === 2).error).toMatch(/filename/);
  });

  it('introspect malformado responde em introspect_result', async () => {
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'introspect', id: 4, request: { dialect: 'oracle' } });
    expect(h.posted.find((m) => m.type === 'introspect_result' && m.id === 4).error).toMatch(/invalid request/);
  });

  it('comando desconhecido ou mensagem não-objeto é ignorado', async () => {
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'rm_rf' });
    await h.send(null);
    await h.send('x');
    expect(h.posted).toHaveLength(0);
  });
});

describe('open_file', () => {
  const folder = { uri: Uri.file('/ws'), name: 'ws', index: 0 };
  const opened = () => mockState.executed.filter((e) => e.command === 'vscode.openWith');

  it('abre .dbml do workspace', async () => {
    mockState.workspaceFolders = [folder];
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'open_file', id: 1, path: 'file:///ws/sub/other.dbml' });
    expect(opened()).toHaveLength(1);
    expect(h.posted).toContainEqual({ type: 'result', id: 1 });
  });

  it('recusa esquema arbitrário, arquivo fora do workspace e extensão errada', async () => {
    mockState.workspaceFolders = [folder];
    const h = await openEditor(DbmlCustomEditorProvider);
    for (const [i, path] of ['http://evil.example/ws/a.dbml', 'file:///etc/passwd', 'file:///ws/a.sh', 'file:///ws/../etc/a.dbml', 'not a uri'].entries()) {
      await h.send({ command: 'open_file', id: 10 + i, path });
      expect(h.posted.find((m) => m.type === 'result' && m.id === 10 + i).error).toBeTruthy();
    }
    expect(opened()).toHaveLength(0);
  });

  it('permite reabrir o próprio documento mesmo fora do workspace', async () => {
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'open_file', id: 1, path: h.document.uri.toString() });
    expect(opened()).toHaveLength(1);
  });
});

describe('mock de Uri', () => {
  it('faz roundtrip de parse/toString', () => {
    expect(Uri.parse('file:///a/b.dbml').toString()).toBe('file:///a/b.dbml');
  });
});
