// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fakeTransport, frame, installDomStubs, mount } from './helpers/harness';
import { WebSocketTransport } from '../src/transport';

class FakeSocket {
  static OPEN = 1;
  static instances: FakeSocket[] = [];
  readyState = 0;
  closed = false;
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  constructor(public url: string) {
    FakeSocket.instances.push(this);
  }
  send() {}
  close() {
    this.closed = true;
    this.readyState = 3;
    this.onclose?.();
  }
}

beforeEach(() => {
  installDomStubs();
  document.body.innerHTML = '';
  document.head.innerHTML = '';
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('Workbench.destroy', () => {
  it('remove os listeners globais, o DOM e o estilo', async () => {
    const transport = fakeTransport();
    const { root, workbench } = mount(transport);
    transport.update('Table users { id int }', undefined, { saved: true });
    await frame();

    const internals = workbench as unknown as Record<string, (e: Event) => void>;
    const onKey = vi.spyOn(internals, 'onKey');
    const onMove = vi.spyOn(internals, 'onMove');
    const onUp = vi.spyOn(internals, 'onUp');

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'a' }));
    window.dispatchEvent(new MouseEvent('mousemove'));
    window.dispatchEvent(new MouseEvent('mouseup'));
    expect([onKey.mock.calls.length, onMove.mock.calls.length, onUp.mock.calls.length]).toEqual([1, 1, 1]);
    expect(document.head.querySelectorAll('style')).toHaveLength(1);

    workbench.destroy();

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'a' }));
    window.dispatchEvent(new MouseEvent('mousemove'));
    window.dispatchEvent(new MouseEvent('mouseup'));
    expect([onKey.mock.calls.length, onMove.mock.calls.length, onUp.mock.calls.length]).toEqual([1, 1, 1]);
    expect(root.childElementCount).toBe(0);
    expect(document.head.querySelectorAll('style')).toHaveLength(0);
  });

  it('ignora atualizações do transporte e não renderiza depois de destruído', async () => {
    const transport = fakeTransport();
    const { root, workbench } = mount(transport);
    transport.update('Table users { id int }', undefined, { saved: true });
    await frame();
    workbench.destroy();
    transport.update('Table orders { id int }', undefined, { saved: true });
    transport.setConnection('offline');
    await frame();
    expect(root.childElementCount).toBe(0);
  });

  it('é idempotente', () => {
    const { workbench } = mount();
    workbench.destroy();
    expect(() => workbench.destroy()).not.toThrow();
  });
});

describe('WebSocketTransport.close', () => {
  it('fecha o socket, rejeita pendências e não reconecta', async () => {
    vi.useFakeTimers();
    FakeSocket.instances = [];
    vi.stubGlobal('WebSocket', FakeSocket);
    const transport = new WebSocketTransport('ws://x/ws');
    FakeSocket.instances[0].readyState = 1;
    const pending = transport.sendPatch({ byte_start: 0, byte_end: 0, new_text: '' } as never);
    const outcome = pending.then(() => 'ok', () => 'rejected');
    transport.close();
    expect(await outcome).toBe('rejected');
    expect(FakeSocket.instances[0].closed).toBe(true);
    vi.advanceTimersByTime(120_000);
    expect(FakeSocket.instances).toHaveLength(1);
    document.dispatchEvent(new Event('visibilitychange'));
    expect(FakeSocket.instances).toHaveLength(1);
  });
});
