// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { isFatalWasmError } from '../src/workbench';
import { fakeTransport, fakeWasm, frame, installDomStubs, mount } from './helpers/harness';

beforeEach(() => {
  installDomStubs();
  document.body.innerHTML = '';
  localStorage.setItem('dbmodeling.prefs', JSON.stringify({ lang: 'pt-BR' }));
  vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('falha fatal do Wasm (#76)', () => {
  it('reconhece trap do Wasm e panics, mas não erros comuns', () => {
    expect(isFatalWasmError(new WebAssembly.RuntimeError('unreachable'))).toBe(true);
    expect(isFatalWasmError(new Error('unreachable'))).toBe(true);
    expect(isFatalWasmError(new Error('memory access out of bounds'))).toBe(true);
    expect(isFatalWasmError(new Error('invalid DBML: x'))).toBe(false);
    expect(isFatalWasmError('texto')).toBe(false);
  });

  it('avisa uma única vez que o motor caiu e pede para recarregar', async () => {
    const wasm = fakeWasm();
    wasm.parse_and_analyze = () => {
      throw new WebAssembly.RuntimeError('unreachable');
    };
    const transport = fakeTransport();
    const { root } = mount(transport, wasm);
    transport.update('Table a { id int }', undefined, { saved: true });
    transport.update('Table a { id int }\nTable b { id int }', undefined, { saved: true });
    await frame();
    const toast = root.querySelector('#toast') as HTMLElement;
    expect(toast.textContent).toContain('recarregue a página');
    expect(root.querySelector('[data-engine-failed]')).not.toBeNull();
    expect(root.querySelectorAll('[data-engine-failed]')).toHaveLength(1);
  });

  it('reinstancia o módulo Wasm automaticamente quando reinit está disponível', async () => {
    const fresh = fakeWasm();
    const broken = fakeWasm();
    broken.parse_and_analyze = () => {
      throw new WebAssembly.RuntimeError('unreachable');
    };
    broken.reinit = vi.fn(async () => fresh);
    const transport = fakeTransport();
    const { root } = mount(transport, broken);
    transport.update('Table a { id int }', undefined, { saved: true });
    await frame();
    await Promise.resolve();
    await frame();
    expect(broken.reinit).toHaveBeenCalledTimes(1);
    expect(root.querySelector('[data-engine-failed]')).toBeNull();
    expect(root.querySelectorAll('.card')).toHaveLength(1);
  });
});
