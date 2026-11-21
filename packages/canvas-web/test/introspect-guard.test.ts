// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ConnectDbModel, IntrospectGuard } from '../src/connect-db';
import { WebSocketTransport } from '../src/transport';

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('IntrospectGuard', () => {
  it('aborta e marca timedOut quando o prazo estoura', async () => {
    const guard = new IntrospectGuard(5000);
    const pending = guard.race(new Promise<string>(() => {}));
    const outcome = pending.then(() => 'ok', () => 'rejected');
    vi.advanceTimersByTime(5000);
    expect(await outcome).toBe('rejected');
    expect(guard.timedOut).toBe(true);
    expect(guard.cancelled).toBe(false);
    expect(guard.signal.aborted).toBe(true);
  });

  it('cancel() rejeita a espera sem marcar timeout', async () => {
    const guard = new IntrospectGuard(5000);
    const outcome = guard.race(new Promise<string>(() => {})).then(() => 'ok', () => 'rejected');
    guard.cancel();
    expect(await outcome).toBe('rejected');
    expect(guard.cancelled).toBe(true);
    expect(guard.timedOut).toBe(false);
  });

  it('dispose() desarma o timer e deixa a resposta passar', async () => {
    const guard = new IntrospectGuard(5000);
    const outcome = guard.race(Promise.resolve('dados'));
    expect(await outcome).toBe('dados');
    guard.dispose();
    vi.advanceTimersByTime(10_000);
    expect(guard.signal.aborted).toBe(false);
  });
});

describe('ConnectDbModel.cancel', () => {
  it('volta a idle e permite nova tentativa', () => {
    const model = new ConnectDbModel();
    model.start();
    model.cancel();
    expect(model.status).toBe('idle');
    expect(model.error).toBe('');
  });
});

describe('WebSocketTransport introspecção', () => {
  it('repassa o AbortSignal ao fetch', async () => {
    vi.stubGlobal('WebSocket', class { static OPEN = 1; close() {} });
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ dbml: '', warnings: [], tables: 0 }), { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);
    const transport = new WebSocketTransport('ws://x/ws');
    const controller = new AbortController();
    const req = { dialect: 'sqlite' } as never;
    await transport.introspect(req, controller.signal);
    await transport.testConnection(req, controller.signal);
    expect(fetchMock.mock.calls.map((c) => (c as unknown as [string, RequestInit])[1].signal)).toEqual([
      controller.signal,
      controller.signal,
    ]);
  });
});
