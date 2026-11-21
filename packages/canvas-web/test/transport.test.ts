// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RECONNECT_MAX_MS, VsCodeTransport, WebSocketTransport, reconnectDelay } from '../src/transport';
import type { DocumentMetadata, TextPatch } from '../src/types';

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
  /** Mensagem do servidor para o transporte. */
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

function setHidden(hidden: boolean) {
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden });
  document.dispatchEvent(new Event('visibilitychange'));
}

describe('reconnectDelay', () => {
  it('cresce exponencialmente com jitter e respeita o teto de 30 s', () => {
    expect(reconnectDelay(0, () => 0)).toBe(500);
    expect(reconnectDelay(0, () => 1)).toBe(1000);
    expect(reconnectDelay(2, () => 1)).toBe(4000);
    expect(reconnectDelay(50, () => 1)).toBe(RECONNECT_MAX_MS);
    expect(reconnectDelay(50, () => 0)).toBe(RECONNECT_MAX_MS / 2);
  });
});

describe('WebSocketTransport reconexão', () => {
  beforeEach(() => {
    FakeSocket.instances = [];
    vi.useFakeTimers();
    vi.stubGlobal('WebSocket', FakeSocket);
    vi.spyOn(Math, 'random').mockReturnValue(1);
    setHidden(false);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('usa backoff crescente e zera o contador ao reconectar', () => {
    new WebSocketTransport('ws://x/ws');
    FakeSocket.instances[0].close();
    vi.advanceTimersByTime(999);
    expect(FakeSocket.instances).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(FakeSocket.instances).toHaveLength(2);
    FakeSocket.instances[1].close();
    vi.advanceTimersByTime(1999);
    expect(FakeSocket.instances).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(FakeSocket.instances).toHaveLength(3);
    FakeSocket.instances[2].open();
    FakeSocket.instances[2].close();
    vi.advanceTimersByTime(1000);
    expect(FakeSocket.instances).toHaveLength(4);
  });

  describe('versão do documento (#51)', () => {
    const patch: TextPatch = {
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
      byte_start: 0,
      byte_end: 0,
      new_text: 'x',
      expected_source: 'a',
      expected_file: 'f.dbml',
    };
    const connected = () => {
      const t = new WebSocketTransport('ws://x/ws');
      const ws = FakeSocket.instances[0];
      ws.open();
      return { t, ws };
    };
    const lastSent = (ws: FakeSocket) => ws.sent[ws.sent.length - 1];

    it('envia a versão recebida em init como base_version e adota a versão devolvida no result', async () => {
      const { t, ws } = connected();
      ws.receive({ type: 'init', source: 'a', version: 4, documentId: 'f.dbml' });
      const first = t.sendPatch(patch);
      expect(lastSent(ws).payload.base_version).toBe(4);
      ws.receive({ type: 'result', id: lastSent(ws).id, version: 5 });
      await first;
      void t.sendPatch(patch).catch(() => {});
      expect(lastSent(ws).payload.base_version).toBe(5);
    });

    it('saveDocument também leva base_version', () => {
      const { t, ws } = connected();
      ws.receive({ type: 'init', source: 'a', version: 7 });
      void t.saveDocument('a', 'f.dbml').catch(() => {});
      expect(lastSent(ws)).toMatchObject({ type: 'save_document', payload: { base_version: 7, expected_source: 'a' } });
    });

    it('update e external_change avançam a versão; versão mais velha não a faz recuar', () => {
      const { t, ws } = connected();
      ws.receive({ type: 'init', source: 'a', version: 1 });
      ws.receive({ type: 'update', source: 'b', version: 3 });
      ws.receive({ type: 'external_change', source: 'c', version: 6 });
      ws.receive({ type: 'result', id: 99, version: 5 }); // resposta atrasada
      void t.sendPatch(patch).catch(() => {});
      expect(lastSent(ws).payload.base_version).toBe(6);
    });

    it('init de uma nova conexão substitui a versão (servidor reiniciado)', () => {
      const { t, ws } = connected();
      ws.receive({ type: 'init', source: 'a', version: 40 });
      ws.close();
      vi.advanceTimersByTime(1000);
      const second = FakeSocket.instances[1];
      second.open();
      second.receive({ type: 'init', source: 'a', version: 1 });
      void t.sendPatch(patch).catch(() => {});
      expect(lastSent(second).payload.base_version).toBe(1);
    });

    it('sem versão conhecida recusa o patch em vez de enviá-lo às cegas', async () => {
      const { t, ws } = connected();
      await expect(t.sendPatch(patch)).rejects.toThrow(/not synced/i);
      expect(ws.sent).toHaveLength(0);
    });

    it('patch_rejected ressincroniza pelo callback de update e o erro do result continua chegando', async () => {
      const { t, ws } = connected();
      const updates: Array<{ source: string; layout: unknown; meta?: DocumentMetadata }> = [];
      t.onDocumentUpdated((source, layout, meta) => updates.push({ source, layout, meta }));
      ws.receive({ type: 'init', source: 'a', version: 2, documentId: 'f.dbml' });
      updates.length = 0;

      const sending = t.sendPatch(patch);
      const id = lastSent(ws).id;
      ws.receive({
        type: 'patch_rejected', source: 'outro', layout: { tables: {} }, version: 3, documentId: 'f.dbml', title: 'f.dbml', saved: true,
      });
      ws.receive({ type: 'result', id, error: 'Document version changed' });

      await expect(sending).rejects.toThrow(/version changed/);
      expect(updates).toHaveLength(1);
      expect(updates[0].source).toBe('outro');
      expect(updates[0].layout).toEqual({ tables: {} });
      expect(updates[0].meta).toMatchObject({ version: 3, documentId: 'f.dbml' });
      // a próxima edição já parte da versão corrente
      void t.sendPatch(patch).catch(() => {});
      expect(lastSent(ws).payload.base_version).toBe(3);
    });
  });

  it('não agenda tentativas com a aba oculta e reconecta ao voltar', () => {
    new WebSocketTransport('ws://x/ws');
    setHidden(true);
    FakeSocket.instances[0].close();
    vi.advanceTimersByTime(120_000);
    expect(FakeSocket.instances).toHaveLength(1);
    setHidden(false);
    expect(FakeSocket.instances).toHaveLength(2);
  });
});

describe('VsCodeTransport versão do documento (#51)', () => {
  const posted: Array<Record<string, unknown>> = [];
  const patch: TextPatch = {
    range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
    byte_start: 0,
    byte_end: 0,
    new_text: 'x',
    expected_source: 'a',
  };
  const fromHost = (data: unknown) => window.dispatchEvent(new MessageEvent('message', { data }));
  const lastPosted = () => posted[posted.length - 1];
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

  it('envia a versão do último update como base_version e adota a do result', async () => {
    fromHost({ type: 'update', source: 'a', version: 3 });
    const first = transport.sendPatch(patch);
    expect(lastPosted()).toMatchObject({ command: 'apply_patch', base_version: 3 });
    fromHost({ type: 'result', id: lastPosted().id, version: 4 });
    await first;
    void transport.saveDocument('a').catch(() => {});
    expect(lastPosted()).toMatchObject({ command: 'save_document', base_version: 4, expected_source: 'a' });
  });

  it('o update que o host reenvia ao rejeitar atualiza a versão e chega ao callback; o erro do result segue', async () => {
    const updates: Array<{ source: string; meta?: DocumentMetadata }> = [];
    transport.onDocumentUpdated((source, _layout, meta) => updates.push({ source, meta }));
    fromHost({ type: 'update', source: 'a', version: 3 });
    updates.length = 0;

    const sending = transport.sendPatch(patch);
    const id = lastPosted().id;
    fromHost({ type: 'update', source: 'digitado no editor', version: 5 });
    fromHost({ type: 'result', id, error: 'Document version changed (webview has 3, current is 5)' });

    await expect(sending).rejects.toThrow(/version changed/);
    expect(updates).toHaveLength(1);
    expect(updates[0]).toMatchObject({ source: 'digitado no editor', meta: { version: 5 } });
    void transport.sendPatch(patch).catch(() => {});
    expect(lastPosted().base_version).toBe(5);
  });

  it('versão mais velha (update ou result atrasado) não faz a versão recuar', () => {
    fromHost({ type: 'update', source: 'a', version: 9 });
    fromHost({ type: 'update', source: 'a', version: 7 });
    fromHost({ type: 'result', id: 123, version: 8 });
    void transport.sendPatch(patch).catch(() => {});
    expect(lastPosted().base_version).toBe(9);
  });

  it('sem versão conhecida recusa o patch', async () => {
    await expect(transport.sendPatch(patch)).rejects.toThrow(/not synced/i);
    expect(posted).toHaveLength(0);
  });

  it('aplica delta incremental sem reenviar o documento inteiro (#111)', () => {
    const updates: Array<{ source: string; version?: number }> = [];
    transport.onDocumentUpdated((source, _layout, meta) => updates.push({ source, version: meta?.version }));
    fromHost({ type: 'update', source: 'Table users { id int }', version: 1 });
    fromHost({
      type: 'delta',
      version: 2,
      saved: false,
      documentId: 'file:///a.dbml',
      changes: [{ rangeOffset: 6, rangeLength: 5, text: 'accounts' }],
    });
    expect(updates[updates.length - 1]).toEqual({ source: 'Table accounts { id int }', version: 2 });
  });
});

describe('WebSocketTransport fila offline (#75)', () => {
  beforeEach(() => {
    FakeSocket.instances = [];
    vi.useFakeTimers();
    vi.stubGlobal('WebSocket', FakeSocket);
    vi.spyOn(Math, 'random').mockReturnValue(1);
    setHidden(false);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('enfileira patches offline e envia ao reconectar quando o servidor não mudou', async () => {
    const t = new WebSocketTransport('ws://x/ws');
    const ws1 = FakeSocket.instances[0];
    ws1.open();
    ws1.receive({ type: 'init', source: 'Table a { id int }', version: 1, documentId: 'f.dbml' });
    ws1.close();

    const p = t.sendPatch({
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 18 } },
      byte_start: 0,
      byte_end: 18,
      new_text: 'Table b { id int }',
      expected_source: 'Table a { id int }',
      expected_file: 'f.dbml',
    });

    vi.advanceTimersByTime(1000);
    const ws2 = FakeSocket.instances[1];
    ws2.open();
    ws2.receive({ type: 'init', source: 'Table a { id int }', version: 1, documentId: 'f.dbml' });

    expect(ws2.sent).toHaveLength(1);
    expect(ws2.sent[0]).toMatchObject({ type: 'apply_patch', payload: { base_version: 1, new_text: 'Table b { id int }' } });
    ws2.receive({ type: 'result', id: ws2.sent[0].id, version: 2 });
    await expect(p).resolves.toBeUndefined();
    t.close();
  });

  it('rejeita fila offline se o documento mudou no servidor durante a desconexão', async () => {
    const t = new WebSocketTransport('ws://x/ws');
    const ws1 = FakeSocket.instances[0];
    ws1.open();
    ws1.receive({ type: 'init', source: 'Table a { id int }', version: 1, documentId: 'f.dbml' });
    ws1.close();

    const p = t.sendPatch({
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 18 } },
      byte_start: 0,
      byte_end: 18,
      new_text: 'Table b { id int }',
      expected_source: 'Table a { id int }',
      expected_file: 'f.dbml',
    });

    vi.advanceTimersByTime(1000);
    const ws2 = FakeSocket.instances[1];
    ws2.open();
    ws2.receive({ type: 'init', source: 'Table external { id int }', version: 2, documentId: 'f.dbml' });

    await expect(p).rejects.toThrow(/changed while offline/i);
    t.close();
  });
});
