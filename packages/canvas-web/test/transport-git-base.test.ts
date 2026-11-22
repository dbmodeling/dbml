// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GIT_BASE_ERROR, TRANSPORT_COMMAND, TRANSPORT_EVENT } from '../src/constants';
import { VsCodeTransport, WebSocketTransport } from '../src/transport';

class FakeSocket {
  static OPEN = 1;
  static instances: FakeSocket[] = [];
  readyState = 0;
  sent: Array<{ type: string; payload: Record<string, unknown>; id: number }> = [];
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  constructor(public url: string) {
    FakeSocket.instances.push(this);
  }
  send(data: string) {
    this.sent.push(JSON.parse(data));
  }
  receive(msg: unknown) {
    this.onmessage?.({ data: JSON.stringify(msg) });
  }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
  open() {
    this.readyState = 1;
    this.onopen?.();
  }
}

describe('WebSocketTransport.requestGitBase', () => {
  let t: WebSocketTransport;
  let ws: FakeSocket;
  beforeEach(() => {
    FakeSocket.instances = [];
    vi.stubGlobal('WebSocket', FakeSocket);
    t = new WebSocketTransport('ws://x/ws');
    ws = FakeSocket.instances[0];
    ws.open();
  });
  afterEach(() => {
    t.close();
    vi.unstubAllGlobals();
  });

  it('envia git_base com o arquivo esperado e resolve com o conteúdo do HEAD', async () => {
    const p = t.requestGitBase('/ws/schema.dbml');
    const sent = ws.sent[ws.sent.length - 1];
    expect(sent).toMatchObject({ type: TRANSPORT_COMMAND.GIT_BASE, payload: { expected_file: '/ws/schema.dbml' } });
    ws.receive({ type: TRANSPORT_EVENT.GIT_BASE_RESULT, id: sent.id, source: 'Table a { id int }', error: null, error_code: null });
    await expect(p).resolves.toEqual({ ok: true, source: 'Table a { id int }' });
  });

  it('erro esperado do git volta como { ok: false } com código e mensagem', async () => {
    const p = t.requestGitBase('f.dbml');
    const sent = ws.sent[ws.sent.length - 1];
    ws.receive({ type: TRANSPORT_EVENT.GIT_BASE_RESULT, id: sent.id, source: null, error: 'untracked', error_code: GIT_BASE_ERROR.UNTRACKED });
    await expect(p).resolves.toEqual({ ok: false, code: GIT_BASE_ERROR.UNTRACKED, message: 'untracked' });
  });

  it('código desconhecido vira "other"', async () => {
    const p = t.requestGitBase('f.dbml');
    const sent = ws.sent[ws.sent.length - 1];
    ws.receive({ type: TRANSPORT_EVENT.GIT_BASE_RESULT, id: sent.id, source: null, error: 'x', error_code: 'novo_codigo' });
    await expect(p).resolves.toMatchObject({ ok: false, code: GIT_BASE_ERROR.OTHER });
  });

  it('ignora respostas de outro id e rejeita se a conexão cai', async () => {
    const p = t.requestGitBase('f.dbml');
    const sent = ws.sent[ws.sent.length - 1];
    ws.receive({ type: TRANSPORT_EVENT.GIT_BASE_RESULT, id: sent.id + 100, source: 'outro', error: null, error_code: null });
    ws.close();
    await expect(p).rejects.toThrow(/connection/i);
  });

  it('desconectado: rejeita sem enviar', async () => {
    ws.close();
    await expect(t.requestGitBase('f.dbml')).rejects.toThrow(/disconnected/i);
  });
});

describe('VsCodeTransport.requestGitBase', () => {
  const posted: Array<Record<string, unknown>> = [];
  const fromHost = (data: unknown) => window.dispatchEvent(new MessageEvent('message', { data }));
  let transport: VsCodeTransport;
  beforeEach(() => {
    posted.length = 0;
    vi.stubGlobal('acquireVsCodeApi', () => ({
      postMessage: (m: Record<string, unknown>) => posted.push(m),
      getState: () => null,
      setState: () => {},
    }));
    transport = new VsCodeTransport();
  });
  afterEach(() => {
    transport.close();
    vi.unstubAllGlobals();
  });

  it('posta git_base e resolve com o conteúdo devolvido pelo host', async () => {
    const p = transport.requestGitBase();
    const msg = posted[posted.length - 1];
    expect(msg).toMatchObject({ command: TRANSPORT_COMMAND.GIT_BASE });
    fromHost({ type: TRANSPORT_EVENT.GIT_BASE_RESULT, id: msg.id, source: 'Table a { id int }' });
    await expect(p).resolves.toEqual({ ok: true, source: 'Table a { id int }' });
  });

  it('erro do host vira { ok: false } com o código', async () => {
    const p = transport.requestGitBase();
    const msg = posted[posted.length - 1];
    fromHost({ type: TRANSPORT_EVENT.GIT_BASE_RESULT, id: msg.id, error: 'not a repo', error_code: GIT_BASE_ERROR.NOT_A_REPO });
    await expect(p).resolves.toEqual({ ok: false, code: GIT_BASE_ERROR.NOT_A_REPO, message: 'not a repo' });
  });

  it('close() rejeita o pedido pendente', async () => {
    const p = transport.requestGitBase();
    transport.close();
    await expect(p).rejects.toThrow(/closed/i);
  });
});
