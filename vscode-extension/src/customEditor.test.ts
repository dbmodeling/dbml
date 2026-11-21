import { beforeEach, describe, expect, it } from 'vitest';
import { DbmlCustomEditorProvider } from './customEditor.js';
import { nodeCliHost } from './cliHost.node.js';
import { fakeContext, openEditor } from './test/harness.js';
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

describe('registro do editor', () => {
  it('não retém o contexto da webview quando a aba fica oculta (DOM, canvas e Wasm são liberados)', () => {
    const { context } = fakeContext();
    DbmlCustomEditorProvider.register(context, { delete() {} } as never, nodeCliHost);
    const reg = mockState.registeredEditors.find((r) => r.viewType === 'dbmodeling.editor');
    expect(reg).toBeDefined();
    expect(reg!.options.webviewOptions.retainContextWhenHidden).toBeFalsy();
  });
});

describe('restauração do estado do canvas', () => {
  const session = { pane: 'schema', editorOpen: false, zoom: 1.25, pan: { x: 40, y: -12 } };

  it('save_session persiste e o próximo "ready" devolve a sessão (reabrir/recarregar a janela)', async () => {
    const first = await openEditor(DbmlCustomEditorProvider);
    await first.send({ command: 'save_session', session });
    // janela recarregada: provider, documento e webview novos; workspaceState sobrevive
    const second = await openEditor(DbmlCustomEditorProvider, { reuse: first });
    await second.send({ command: 'ready' });
    expect(second.posted.find((m) => m.type === 'update').session).toEqual(session);
  });

  it('webview recriada (aba volta a ficar visível) recebe de novo texto, layout e sessão', async () => {
    mockState.files.set(
      'file:///ws/schema.dbml.layout.json',
      new TextEncoder().encode(JSON.stringify({ positions: { a: { x: 5, y: 6 } } }))
    );
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'save_session', session });
    await h.send({ command: 'ready' });
    h.posted.length = 0;
    await h.send({ command: 'ready' }); // nova webview montada no mesmo painel
    const update = h.posted.find((m) => m.type === 'update');
    expect(update.session).toEqual(session);
    expect(update.layout).toEqual({ positions: { a: { x: 5, y: 6 } } });
    expect(update.source).toBe(h.document.getText());
  });

  it('sessão com valores inválidos é saneada antes de persistir', async () => {
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'save_session', session: { zoom: 'grande', pan: { x: 1 }, pane: 'schema' } });
    await h.send({ command: 'ready' });
    expect(h.posted.find((m) => m.type === 'update').session).toEqual({ pane: 'schema' });
  });

  it('sessão de um documento não vaza para outro', async () => {
    const a = await openEditor(DbmlCustomEditorProvider, { uri: Uri.file('/ws/a.dbml') });
    await a.send({ command: 'save_session', session });
    const b = await openEditor(DbmlCustomEditorProvider, { uri: Uri.file('/ws/b.dbml'), reuse: a });
    await b.send({ command: 'ready' });
    expect(b.posted.find((m) => m.type === 'update').session).toBeUndefined();
  });
});

describe('falhas de gravação visíveis ao usuário', () => {
  const layout = { positions: { a: { x: 1, y: 2 } } };

  it('falha ao gravar o sidecar avisa uma vez por arquivo, com a causa', async () => {
    const h = await openEditor(DbmlCustomEditorProvider);
    mockState.writeError = new Error('EACCES: permission denied');
    await h.send({ command: 'save_layout', id: 1, layout });
    await h.send({ command: 'save_layout', id: 2, layout });
    expect(mockState.messages.warnings).toHaveLength(1);
    expect(mockState.messages.warnings[0].message).toContain('EACCES: permission denied');
    expect(mockState.messages.warnings[0].message).toContain('schema.dbml');
    // a webview continua recebendo o erro de cada tentativa
    expect(h.posted.filter((m) => m.type === 'result' && m.error)).toHaveLength(2);
  });

  it('sidecar gravado com sucesso não avisa', async () => {
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'save_layout', id: 1, layout });
    expect(mockState.messages.warnings).toHaveLength(0);
  });

  it('applyEdit recusado avisa uma vez por arquivo', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    mockState.applyEditResult = false;
    const patch = {
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
      new_text: 'X',
      expected_source: 'abc',
    };
    await h.send({ command: 'apply_patch', id: 1, patch });
    await h.send({ command: 'apply_patch', id: 2, patch });
    expect(mockState.messages.warnings).toHaveLength(1);
    expect(mockState.messages.warnings[0].message).toMatch(/could not apply/i);
  });

  it('conflito de texto desatualizado não gera aviso (é fluxo normal de reconciliação)', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    await h.send({
      command: 'apply_patch',
      id: 1,
      patch: { range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } }, new_text: 'X', expected_source: 'zzz' },
    });
    expect(mockState.messages.warnings).toHaveLength(0);
  });
});

describe('lista de arquivos do workspace', () => {
  it('não substitui files.exclude do usuário (exclude indefinido) e limita o resultado', async () => {
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'ready' });
    expect(mockState.findFilesCalls.length).toBeGreaterThan(0);
    for (const call of mockState.findFilesCalls) {
      expect(call.include).toBe('**/*.dbml');
      expect(call.exclude).toBeUndefined();
      expect(call.max).toBeGreaterThan(0);
    }
  });
});

describe('conexão com o banco', () => {
  const request = {
    dialect: 'sqlite',
    host: '',
    port: null,
    database: '',
    user: '',
    password: '',
    path: '/tmp/a.db',
    trust_server_certificate: false,
    schemas: [],
  };

  it('workspace não confiável bloqueia check_cli, test_connection e introspect', async () => {
    mockState.isTrusted = false;
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'check_cli', id: 1 });
    await h.send({ command: 'test_connection', id: 2, request });
    await h.send({ command: 'introspect', id: 3, request });
    expect(h.posted.find((m) => m.type === 'result' && m.id === 1).error).toMatch(/untrusted/);
    expect(h.posted.find((m) => m.type === 'test_connection_result' && m.id === 2).error).toMatch(/untrusted/);
    expect(h.posted.find((m) => m.type === 'introspect_result' && m.id === 3).error).toMatch(/untrusted/);
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
