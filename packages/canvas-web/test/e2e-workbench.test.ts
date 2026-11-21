// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WebSocketTransport } from '../src/transport';
import { TRANSPORT_EVENT } from '../src/constants';
import { fakeWasm, installDomStubs, mount } from './helpers/harness';

const WS_URL = 'ws://127.0.0.1:3000/ws';
const INITIAL_SOURCE = 'Table users { id int }';
const EDITED_SOURCE = 'Table users { id int }\nTable orders { id int }';
const DOC_ID = '/tmp/e2e.dbml';
const DOC_TITLE = 'e2e.dbml';
const RECONNECT_MS = 1000;
const FRAME_MS = 16;

class LoopbackSocket {
  static OPEN = 1;
  static instances: LoopbackSocket[] = [];
  readyState = 0;
  sent: Array<{ type: string; payload: Record<string, unknown>; id: number }> = [];
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  constructor(public url: string) {
    LoopbackSocket.instances.push(this);
  }
  send(data: string) {
    this.sent.push(JSON.parse(data));
  }
  receive(msg: unknown) {
    this.onmessage?.({ data: JSON.stringify(msg) });
  }
  open() {
    this.readyState = 1;
    this.onopen?.();
  }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
}

describe('E2E Workbench + WebSocketTransport (#83)', () => {
  beforeEach(() => {
    LoopbackSocket.instances = [];
    installDomStubs();
    document.body.innerHTML = '';
    vi.useFakeTimers();
    vi.stubGlobal('WebSocket', LoopbackSocket);
    vi.spyOn(Math, 'random').mockReturnValue(1);
    localStorage.setItem('dbmodeling.prefs', JSON.stringify({ lang: 'pt-BR' }));
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('conecta, renderiza, exibe indicador ao cair o servidor e reconcilia edição ao reconectar', async () => {
    const transport = new WebSocketTransport(WS_URL);
    const { root, workbench } = mount(transport, fakeWasm());

    const ws1 = LoopbackSocket.instances[0];
    ws1.open();
    ws1.receive({
      type: TRANSPORT_EVENT.INIT,
      source: INITIAL_SOURCE,
      version: 1,
      documentId: DOC_ID,
      title: DOC_TITLE,
      saved: true,
    });
    vi.advanceTimersByTime(FRAME_MS);
    expect(root.querySelectorAll('.card')).toHaveLength(1);
    expect(root.querySelector('[data-conn]')).toBeNull();

    // Servidor cai: indicador offline aparece.
    ws1.close();
    const offline = root.querySelector('[data-conn]') as HTMLElement;
    expect(offline).not.toBeNull();
    expect(offline.dataset.conn).toBe('offline');

    // Edição feita offline entra na fila em vez de ser descartada.
    const offlineEdit = transport.sendPatch({
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: INITIAL_SOURCE.length } },
      byte_start: 0,
      byte_end: INITIAL_SOURCE.length,
      new_text: EDITED_SOURCE,
      expected_source: INITIAL_SOURCE,
      expected_file: DOC_ID,
    });

    // Servidor volta: reconecta, indicador some e patch enfileirado é enviado.
    vi.advanceTimersByTime(RECONNECT_MS);
    const ws2 = LoopbackSocket.instances[1];
    ws2.open();
    ws2.receive({
      type: TRANSPORT_EVENT.INIT,
      source: INITIAL_SOURCE,
      version: 1,
      documentId: DOC_ID,
      title: DOC_TITLE,
      saved: true,
    });
    expect(root.querySelector('[data-conn]')).toBeNull();
    expect(ws2.sent).toHaveLength(1);
    ws2.receive({ type: TRANSPORT_EVENT.RESULT, id: ws2.sent[0].id, version: 2 });
    await expect(offlineEdit).resolves.toBeUndefined();

    workbench.destroy();
    transport.close();
  });
});
