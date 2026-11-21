// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RECONNECT_MAX_MS, WebSocketTransport, reconnectDelay } from '../src/transport';

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
