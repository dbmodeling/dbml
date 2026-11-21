// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import axe from 'axe-core';
import { APP_VERSION, PRODUCT_GUIDE, decideStartupGuide, pickText } from '../src/guide';
import { computeTourCardPosition } from '../src/tour';
import { Workbench } from '../src/workbench';
import type { HostTransport, WasmApi } from '../src/types';

if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
}

function makeTransport(savedStates: Array<{ onboardingDone: boolean; lastSeenVersion: string | null }> = []) {
  let onUpdate: Parameters<HostTransport['onDocumentUpdated']>[0] | null = null;
  const transport: HostTransport = {
    sendPatch: async () => {},
    saveLayout: async () => {},
    onDocumentUpdated(cb) { onUpdate = cb; },
    saveGuideState(state) { savedStates.push(state); },
  };
  return { transport, emit: (src: string, meta?: Parameters<NonNullable<typeof onUpdate>>[2]) => onUpdate?.(src, undefined, meta) };
}

const minimalWasm: WasmApi = {
  parse_and_analyze: () => ({ schema: { tables: [], refs: [], groups: [] }, diagnostics: [], enums: [] }),
  compute_scene: () => ({ tables: [], edges: [], groups: [], enums: [] }),
  compute_layout: () => ({ positions: {} }),
  apply_command: () => ({ range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }, byte_start: 0, byte_end: 0, new_text: '' }),
  swap_dbml_lines: () => ({ range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }, byte_start: 0, byte_end: 0, new_text: '' }),
  format_dbml: (s) => s,
};

afterEach(() => {
  localStorage.clear();
  document.body.innerHTML = '';
});

describe('product-guide & startup decision', () => {
  it('loads tour steps and releases from canonical JSON', () => {
    expect(PRODUCT_GUIDE.tour.length).toBeGreaterThanOrEqual(4);
    expect(PRODUCT_GUIDE.releases.length).toBeGreaterThanOrEqual(1);
    expect(PRODUCT_GUIDE.releases[0].version).toBe(APP_VERSION);
    expect(pickText(PRODUCT_GUIDE.tour[0].title, 'pt-BR')).toBeTruthy();
    expect(pickText(PRODUCT_GUIDE.tour[0].title, 'en')).toBeTruthy();
  });

  it('starts with tour on first run (no previous version and onboarding not done)', () => {
    expect(decideStartupGuide('0.2.0', false, null)).toBe('tour');
    expect(decideStartupGuide('0.2.0', false, undefined)).toBe('tour');
    expect(decideStartupGuide('0.2.0', false, '')).toBe('tour');
  });

  it('shows whats-new once when upgrading from an older version', () => {
    expect(decideStartupGuide('0.2.0', true, '0.1.4')).toBe('whats-new');
    expect(decideStartupGuide('0.2.0', false, '0.1.4')).toBe('whats-new');
  });

  it('shows nothing when already on current version', () => {
    expect(decideStartupGuide('0.2.0', true, '0.2.0')).toBe('none');
    expect(decideStartupGuide('0.2.0', false, '0.2.0')).toBe('none');
  });
});

describe('tour positioning & flip', () => {
  it('centers card with no arrow when target is absent or center placement', () => {
    const pos = computeTourCardPosition(null, 'bottom', 1000, 700, 320, 180);
    expect(pos.arrowSide).toBe('none');
    expect(pos.left).toBe(340);
    expect(pos.top).toBe(260);
  });

  it('flips from bottom to top when target is near the bottom edge', () => {
    const pos = computeTourCardPosition({ left: 400, top: 620, width: 100, height: 40 }, 'bottom', 1000, 700, 320, 180);
    expect(pos.arrowSide).toBe('bottom');
    expect(pos.top).toBeLessThan(620);
  });
});

describe('Workbench tour & whats-new integration', () => {
  it('opens tour automatically on first run, navigates via keyboard, and persists state on finish', async () => {
    const root = document.createElement('div');
    document.body.appendChild(root);
    const saved: Array<{ onboardingDone: boolean; lastSeenVersion: string | null }> = [];
    const { transport, emit } = makeTransport(saved);
    const wb = new Workbench(root, transport, minimalWasm, { autoGuide: true });

    emit('', { title: 'a.dbml' });
    expect(root.querySelector('.wb-tour-layer')).not.toBeNull();
    expect(root.querySelector('#wb-tour-title')?.textContent).toBeTruthy();

    const axeRes = await axe.run(root.querySelector('.wb-tour-card')!, { rules: { 'color-contrast': { enabled: false } } });
    expect(axeRes.violations).toEqual([]);

    // Avança um passo via teclado e pula com Escape
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    expect(root.querySelector('.wb-tour-step')?.textContent).toContain('2');

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(root.querySelector('.wb-tour-layer')).toBeNull();
    expect(saved.at(-1)).toEqual({ onboardingDone: true, lastSeenVersion: APP_VERSION });
    expect(localStorage.getItem('dbmodeling.onboardingDone')).toBe('true');
    expect(localStorage.getItem('dbmodeling.lastSeenVersion')).toBe(APP_VERSION);

    wb.destroy();
  });

  it('opens whats-new automatically on upgrade and via clickable statusbar version button', async () => {
    const root = document.createElement('div');
    document.body.appendChild(root);
    const saved: Array<{ onboardingDone: boolean; lastSeenVersion: string | null }> = [];
    const { transport, emit } = makeTransport(saved);
    const wb = new Workbench(root, transport, minimalWasm, { autoGuide: true });

    emit('', { title: 'a.dbml', guideState: { onboardingDone: true, lastSeenVersion: '0.1.4' } });
    const dlg = root.querySelector('#wn-title');
    expect(dlg?.textContent).toContain(APP_VERSION);
    expect(saved.at(-1)).toEqual({ onboardingDone: true, lastSeenVersion: APP_VERSION });

    const axeRes = await axe.run(root.querySelector('[role="dialog"]')!, { rules: { 'color-contrast': { enabled: false } } });
    expect(axeRes.violations).toEqual([]);

    (root.querySelector('#wn-ok') as HTMLButtonElement).click();
    // Reabre manualmente clicando na versão na statusbar
    const verBtn = root.querySelector('.ver-btn') as HTMLButtonElement;
    expect(verBtn).not.toBeNull();
    expect(verBtn.textContent).toBe(`v${APP_VERSION}`);
    verBtn.click();
    expect(root.querySelector('#wn-title')?.textContent).toContain(APP_VERSION);

    wb.destroy();
  });

  it('falls back gracefully when localStorage throws', () => {
    const spy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('quota'); });
    const root = document.createElement('div');
    document.body.appendChild(root);
    const { transport, emit } = makeTransport();
    const wb = new Workbench(root, transport, minimalWasm, { autoGuide: false });
    emit('', { title: 'a.dbml' });
    expect(() => wb.openWhatsNew()).not.toThrow();
    spy.mockRestore();
    wb.destroy();
  });
});
