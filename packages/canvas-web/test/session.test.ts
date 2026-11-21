// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { WasmApi, WasmSession } from '../src/types';
import { fakeTransport, fakeWasm, frame, installDomStubs, mount } from './helpers/harness';

beforeEach(() => {
  installDomStubs();
  document.body.innerHTML = '';
  localStorage.setItem('dbmodeling.prefs', JSON.stringify({ lang: 'pt-BR' }));
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

type FakeSessionState = WasmSession & { calls: Record<string, number>; freed: boolean };

/** Wasm com `Session` falsa que conta as chamadas (uma análise por versão, route_edges por quadro). */
function wasmWithSession() {
  const base = fakeWasm();
  const sessions: FakeSessionState[] = [];
  class FakeSession {
    calls: Record<string, number> = { analyze: 0, compute_scene: 0, route_edges: 0 };
    freed = false;
    analysis_count = 0;
    constructor() {
      sessions.push(this as unknown as FakeSessionState);
    }
    analyze(source: string) {
      this.calls.analyze++;
      return base.parse_and_analyze(source);
    }
    compute_scene(source: string, positions: Record<string, { x: number; y: number }>) {
      this.calls.compute_scene++;
      return base.compute_scene(source, positions);
    }
    route_edges() {
      this.calls.route_edges++;
      return [];
    }
    free() {
      this.freed = true;
    }
  }
  const wasm = { ...base, Session: FakeSession } as unknown as WasmApi;
  return { wasm, base, sessions };
}

const card = (root: HTMLElement) => root.querySelector('.card') as HTMLElement;

async function dragCard(root: HTMLElement, moves: number) {
  card(root).dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: 100, clientY: 100 }));
  for (let i = 1; i <= moves; i++) {
    window.dispatchEvent(new MouseEvent('mousemove', { clientX: 100 + i * 5, clientY: 100 + i * 3 }));
  }
  await frame();
  window.dispatchEvent(new MouseEvent('mouseup'));
  await frame();
}

describe('Session Wasm (#107, #110)', () => {
  it('analisa uma única vez por edição de texto', async () => {
    const { wasm, sessions } = wasmWithSession();
    const transport = fakeTransport();
    mount(transport, wasm);
    transport.update('Table users { id int }', undefined, { saved: true });
    await frame();
    expect(sessions).toHaveLength(1);
    expect(sessions[0].calls).toMatchObject({ analyze: 1, compute_scene: 1 });
    transport.update('Table users { id int\n name text }', undefined, { saved: true });
    await frame();
    expect(sessions[0].calls).toMatchObject({ analyze: 2, compute_scene: 2 });
  });

  it('não chama as funções soltas que reanalisam quando há Session', async () => {
    const { wasm } = wasmWithSession();
    // a Session falsa delega em `base`; o workbench recebe espiãs no lugar das funções soltas
    const spyWasm = { ...wasm, parse_and_analyze: vi.fn(), compute_scene: vi.fn() } as unknown as WasmApi;
    const transport = fakeTransport();
    const { root } = mount(transport, spyWasm);
    transport.update('Table users { id int }', undefined, { saved: true });
    await frame();
    await dragCard(root, 4);
    expect(spyWasm.parse_and_analyze).not.toHaveBeenCalled();
    expect(spyWasm.compute_scene).not.toHaveBeenCalled();
  });

  it('arrasto roteia arestas pela Session, uma vez por quadro e sem reanalisar', async () => {
    const { wasm, sessions } = wasmWithSession();
    const transport = fakeTransport();
    const { root } = mount(transport, wasm);
    transport.update('Table users { id int }', undefined, { saved: true });
    await frame();
    const before = { ...sessions[0].calls };
    await dragCard(root, 8);
    const after = sessions[0].calls;
    expect(after.route_edges).toBeGreaterThanOrEqual(1);
    expect(after.route_edges).toBeLessThan(8); // 8 mousemove no mesmo quadro viram um reroteamento
    expect(after.analyze).toBe(before.analyze);
    expect(after.compute_scene).toBe(before.compute_scene);
  });

  it('free() da Session em destroy()', async () => {
    const { wasm, sessions } = wasmWithSession();
    const transport = fakeTransport();
    const { workbench } = mount(transport, wasm);
    transport.update('Table users { id int }', undefined, { saved: true });
    await frame();
    expect(sessions[0].freed).toBe(false);
    workbench.destroy();
    expect(sessions[0].freed).toBe(true);
    expect(() => workbench.destroy()).not.toThrow();
  });

  it('sem Session no host (VS Code antigo) cai nas funções soltas', async () => {
    const wasm = fakeWasm();
    const transport = fakeTransport();
    const { root } = mount(transport, wasm);
    transport.update('Table users { id int }', undefined, { saved: true });
    await frame();
    const scene = wasm.compute_scene as unknown as ReturnType<typeof vi.fn>;
    const before = scene.mock.calls.length;
    await dragCard(root, 3);
    expect(scene.mock.calls.length).toBeGreaterThan(before);
  });
});
