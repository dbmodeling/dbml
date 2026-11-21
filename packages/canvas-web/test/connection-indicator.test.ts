// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { frame, installDomStubs, mount, fakeTransport } from './helpers/harness';
import { WebSocketTransport } from '../src/transport';

class FakeSocket {
  static OPEN = 1;
  static instances: FakeSocket[] = [];
  readyState = 0;
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  constructor(public url: string) {
    FakeSocket.instances.push(this);
  }
  send() {}
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
}

beforeEach(() => {
  installDomStubs();
  document.body.innerHTML = '';
});
afterEach(() => {
  vi.unstubAllGlobals();
});

describe('WebSocketTransport estado da conexão', () => {
  it('notifica connecting, online e offline', () => {
    FakeSocket.instances = [];
    vi.stubGlobal('WebSocket', FakeSocket);
    const transport = new WebSocketTransport('ws://x/ws');
    const states: string[] = [];
    transport.onConnectionChange((s) => states.push(s));
    FakeSocket.instances[0].readyState = 1;
    FakeSocket.instances[0].onopen?.();
    FakeSocket.instances[0].close();
    expect(states).toEqual(['connecting', 'online', 'offline']);
  });
});

describe('indicador de conexão no workbench', () => {
  it('aparece quando o servidor cai e some ao reconectar', async () => {
    const transport = fakeTransport();
    const { root } = mount(transport);
    transport.update('Table users { id int }', undefined, { saved: true });
    transport.setConnection('online');
    await frame();
    expect(root.querySelector('[data-conn]')).toBeNull();

    transport.setConnection('offline');
    const indicator = root.querySelector('[data-conn]') as HTMLElement;
    expect(indicator).not.toBeNull();
    expect(indicator.getAttribute('role')).toBe('status');
    expect(indicator.dataset.conn).toBe('offline');

    transport.setConnection('online');
    expect(root.querySelector('[data-conn]')).toBeNull();
  });
});
