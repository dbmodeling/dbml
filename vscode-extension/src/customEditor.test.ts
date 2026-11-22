import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DbmlCustomEditorProvider } from './customEditor.js';
import type { CliHost } from './cliHost.js';
import { nodeCliHost } from './cliHost.node.js';
import { webCliHost } from './cliHost.web.js';
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

  it('apply_patch aplica o patch e confirma com "result" trazendo a nova versão', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    await h.send({
      command: 'apply_patch',
      id: 7,
      base_version: h.document.version,
      patch: {
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
        new_text: 'X',
        expected_source: 'abc',
      },
    });
    expect(h.document.version).toBe(2);
    expect(h.posted).toContainEqual({ type: 'result', id: 7, version: 2 });
  });

  it('apply_patch com texto desatualizado responde erro', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    await h.send({
      command: 'apply_patch',
      id: 8,
      base_version: h.document.version,
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

describe('versão do documento (#51)', () => {
  const patch = {
    range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
    new_text: 'X',
    expected_source: 'abc',
  };

  it('"update" leva document.version', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    h.document.version = 5;
    await h.send({ command: 'ready' });
    expect(h.posted.find((m) => m.type === 'update').version).toBe(5);
  });

  it('apply_patch com versão velha é rejeitado sem tocar no documento e reenvia o update', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    await h.send({ command: 'ready' });
    const staleVersion = h.document.version;
    // O texto mudou fora da Webview (digitação no editor nativo): versão e texto novos.
    h.document.text = 'abc!';
    h.document.version = staleVersion + 1;
    h.posted.length = 0;
    let applied = false;
    mockState.afterApplyEdit = () => void (applied = true);

    await h.send({ command: 'apply_patch', id: 9, base_version: staleVersion, patch: { ...patch, expected_source: 'abc!' } });

    expect(applied).toBe(false);
    expect(h.document.getText()).toBe('abc!');
    const update = h.posted.find((m) => m.type === 'update');
    expect(update).toMatchObject({ source: 'abc!', version: staleVersion + 1 });
    const result = h.posted.find((m) => m.type === 'result' && m.id === 9);
    expect(result.error).toMatch(/version/);
    expect(mockState.messages.warnings).toHaveLength(0);
  });

  it('versão velha é rejeitada mesmo com o texto esperado igual ao atual', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    const staleVersion = h.document.version;
    h.document.version = staleVersion + 1; // ex.: digitou e desfez, texto igual
    await h.send({ command: 'apply_patch', id: 1, base_version: staleVersion, patch });
    expect(h.posted.find((m) => m.type === 'result' && m.id === 1).error).toMatch(/version/);
    expect(h.document.version).toBe(staleVersion + 1);
  });

  it('save_document exige a versão atual', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    const staleVersion = h.document.version;
    h.document.version = staleVersion + 1;
    h.document.isDirty = true;
    await h.send({ command: 'save_document', id: 1, base_version: staleVersion, expected_source: 'abc' });
    expect(h.posted.find((m) => m.type === 'result' && m.id === 1).error).toMatch(/version/);
    expect(h.document.isDirty).toBe(true);
    expect(h.posted.find((m) => m.type === 'update').version).toBe(staleVersion + 1);

    h.posted.length = 0;
    await h.send({ command: 'save_document', id: 2, base_version: staleVersion + 1, expected_source: 'abc' });
    expect(h.posted.find((m) => m.type === 'result' && m.id === 2)).toMatchObject({ id: 2, version: staleVersion + 1 });
    expect(h.posted.find((m) => m.type === 'result' && m.id === 2).error).toBeUndefined();
    expect(h.document.isDirty).toBe(false);
  });

  it('mudança só de versão (texto igual) ainda gera update', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'a' });
    await h.send({ command: 'ready' });
    h.posted.length = 0;
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      h.document.version += 2; // digitou e desfez dentro do debounce
      mockState.textListeners.forEach((l) => l({ document: h.document }));
      vi.advanceTimersByTime(100);
      expect(h.posted.find((m) => m.type === 'update').version).toBe(3);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('validação das mensagens da Webview', () => {
  it('apply_patch malformado responde erro e não toca no documento', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    await h.send({ command: 'apply_patch', id: 1, base_version: 1, patch: { range: null, new_text: 1 } });
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

describe('sincronização de texto com a webview', () => {
  const changed = (h: { document: { uri: Uri } }) => mockState.textListeners.forEach((l) => l({ document: h.document }));

  afterEach(() => vi.useRealTimers());

  it('rajada de alterações vira um único update, sem recalcular a lista de arquivos', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'a' });
    await h.send({ command: 'ready' });
    h.posted.length = 0;
    const findsBefore = mockState.findFilesCalls.length;
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    for (const t of ['ab', 'abc', 'abcd']) {
      h.document.text = t;
      h.document.isDirty = true;
      changed(h);
    }
    vi.advanceTimersByTime(100);
    const updates = h.posted.filter((m) => m.type === 'update');
    expect(updates).toHaveLength(1);
    expect(updates[0].source).toBe('abcd');
    expect(updates[0].files).toBeUndefined();
    expect(mockState.findFilesCalls.length).toBe(findsBefore);
  });

  it('não reenvia quando texto e estado de salvo não mudaram', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'a' });
    await h.send({ command: 'ready' });
    h.posted.length = 0;
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    changed(h); // nada mudou desde o 'ready'
    vi.advanceTimersByTime(100);
    expect(h.posted).toHaveLength(0);
  });

  it('aba oculta não recebe updates; ao voltar, o ready reenvia tudo', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'a' });
    await h.send({ command: 'ready' });
    h.posted.length = 0;
    h.panel.visible = false;
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    h.document.text = 'novo';
    changed(h);
    vi.advanceTimersByTime(100);
    expect(h.posted).toHaveLength(0);
    h.panel.visible = true;
    vi.useRealTimers();
    await h.send({ command: 'ready' });
    expect(h.posted.find((m) => m.type === 'update').source).toBe('novo');
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
    await h.send({ command: 'apply_patch', id: 1, base_version: h.document.version, patch });
    await h.send({ command: 'apply_patch', id: 2, base_version: h.document.version, patch });
    expect(mockState.messages.warnings).toHaveLength(1);
    expect(mockState.messages.warnings[0].message).toMatch(/could not apply/i);
  });

  it('conflito de texto desatualizado não gera aviso (é fluxo normal de reconciliação)', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    await h.send({
      command: 'apply_patch',
      id: 1,
      base_version: h.document.version,
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

describe('git_base (modo Comparar)', () => {
  const withGit = (readGitHead: CliHost['readGitHead']): CliHost => ({ ...nodeCliHost, readGitHead });

  it('responde com o conteúdo do HEAD do arquivo aberto', async () => {
    const readGitHead = vi.fn(async () => ({ ok: true as const, source: 'Table a { id int }\n' }));
    const h = await openEditor(DbmlCustomEditorProvider, { cli: withGit(readGitHead) });
    await h.send({ command: 'git_base', id: 11 });
    expect(readGitHead).toHaveBeenCalledWith('/ws/schema.dbml');
    expect(h.posted).toContainEqual({ type: 'git_base_result', id: 11, source: 'Table a { id int }\n' });
  });

  it('falha esperada (arquivo novo) volta com código e mensagem, sem tocar o documento', async () => {
    const readGitHead = vi.fn(async () => ({ ok: false as const, code: 'untracked' as const, message: 'new file' }));
    const h = await openEditor(DbmlCustomEditorProvider, { cli: withGit(readGitHead) });
    const version = h.document.version;
    await h.send({ command: 'git_base', id: 12 });
    expect(h.posted).toContainEqual({ type: 'git_base_result', id: 12, error: 'new file', error_code: 'untracked' });
    expect(h.document.version).toBe(version);
    expect(h.document.getText()).toBe('Table a {\n  id int\n}\n');
  });

  it('documento que não é arquivo local (untitled, virtual) não consulta o git', async () => {
    const readGitHead = vi.fn();
    const h = await openEditor(DbmlCustomEditorProvider, { cli: withGit(readGitHead), uri: Uri.parse('untitled:Untitled-1') });
    await h.send({ command: 'git_base', id: 13 });
    expect(readGitHead).not.toHaveBeenCalled();
    expect(h.posted.find((m) => m.type === 'git_base_result')).toMatchObject({ id: 13, error_code: 'not_a_repo' });
  });

  it('workspace não confiável não executa o git', async () => {
    mockState.isTrusted = false;
    const readGitHead = vi.fn();
    const h = await openEditor(DbmlCustomEditorProvider, { cli: withGit(readGitHead) });
    await h.send({ command: 'git_base', id: 14 });
    expect(readGitHead).not.toHaveBeenCalled();
    expect(h.posted.find((m) => m.type === 'git_base_result')).toMatchObject({ id: 14, error_code: 'other' });
    expect(h.posted.find((m) => m.type === 'git_base_result').error).toMatch(/untrusted/);
  });

  it('exceção inesperada do host vira erro "other" (a Webview nunca fica esperando)', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { cli: withGit(async () => { throw new Error('boom'); }) });
    await h.send({ command: 'git_base', id: 15 });
    expect(h.posted.find((m) => m.type === 'git_base_result')).toMatchObject({ id: 15, error: 'boom', error_code: 'other' });
  });

  it('host web (sem processos) responde git_unavailable', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { cli: webCliHost });
    await h.send({ command: 'git_base', id: 16 });
    expect(h.posted.find((m) => m.type === 'git_base_result')).toMatchObject({ id: 16, error_code: 'git_unavailable' });
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
