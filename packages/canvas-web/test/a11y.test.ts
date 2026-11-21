// @vitest-environment jsdom
import axe from 'axe-core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fakeTransport, frame, installDomStubs, mount } from './helpers/harness';

beforeEach(() => {
  installDomStubs();
  document.body.innerHTML = '';
  localStorage.setItem('dbmodeling.prefs', JSON.stringify({ lang: 'pt-BR' }));
});
afterEach(() => vi.unstubAllGlobals());

async function loaded() {
  const transport = fakeTransport();
  const mounted = mount(transport);
  transport.update('Table users { id int [pk]\n email varchar }', undefined, { saved: true });
  await frame();
  return { ...mounted, transport };
}

const card = (root: HTMLElement) => root.querySelector('.card') as HTMLElement;

describe('acessibilidade do canvas', () => {
  it('cartões, colunas e painéis têm papel e nome acessível', async () => {
    const { root } = await loaded();
    const table = card(root);
    expect(table.getAttribute('role')).toBe('group');
    expect(table.getAttribute('aria-label')).toBe('Tabela users, 2 colunas');
    expect(table.getAttribute('tabindex')).toBe('0');
    const rows = table.querySelectorAll('[role="listitem"]');
    expect(rows).toHaveLength(2);
    expect(rows[0].getAttribute('aria-label')).toBe('id: int, chave primária, obrigatória');
    expect(rows[1].getAttribute('aria-label')).toBe('email: int');
    expect(table.querySelector('[role="list"]')).not.toBeNull();
    expect(root.querySelector('svg.edges')?.getAttribute('aria-hidden')).toBe('true');
    expect(root.querySelector('#surface')?.getAttribute('role')).toBe('region');
    expect(root.querySelector('#surface')?.getAttribute('aria-label')).toBe('Diagrama');
    for (const id of ['titlebar', 'rail-left', 'tools']) {
      expect(root.querySelector(`#${id}`)?.getAttribute('role')).toBe('toolbar');
      expect(root.querySelector(`#${id}`)?.getAttribute('aria-label')).toBeTruthy();
    }
  });

  it('anuncia a seleção numa região aria-live', async () => {
    const { root } = await loaded();
    const live = root.querySelector('#live') as HTMLElement;
    expect(live.getAttribute('aria-live')).toBe('polite');
    card(root).dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));
    window.dispatchEvent(new MouseEvent('mouseup'));
    await frame();
    expect(live.textContent).toContain('users');
  });

  it('move a tabela focada com as setas e mantém o foco', async () => {
    const { root, transport } = await loaded();
    const before = card(root).style.left;
    card(root).focus();
    card(root).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await frame();
    expect(card(root).getAttribute('aria-label')).toContain('selecionada');
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight' }));
    await frame();
    expect(card(root).style.left).toBe(`${parseFloat(before) + 8}px`);
    expect(document.activeElement).toBe(card(root));
    expect(transport.patches).toHaveLength(0);
  });

  it('não tem violações do axe-core (cor fica de fora: jsdom não calcula estilos)', async () => {
    const { root } = await loaded();
    const results = await axe.run(root, {
      rules: { 'color-contrast': { enabled: false }, region: { enabled: false } },
    });
    expect(results.violations.map((v) => `${v.id}: ${v.nodes.length}`)).toEqual([]);
  });

  it('também passa no axe-core com uma tabela selecionada (inspetor aberto)', async () => {
    const { root } = await loaded();
    card(root).focus();
    card(root).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await frame();
    const results = await axe.run(root, {
      rules: { 'color-contrast': { enabled: false }, region: { enabled: false } },
    });
    expect(results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(' | ')}`)).toEqual([]);
  });
});
