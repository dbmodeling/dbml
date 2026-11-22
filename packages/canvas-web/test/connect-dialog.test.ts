// @vitest-environment jsdom
import axe from 'axe-core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { catalog } from '../src/actions';
import {
  CONNECT_DIALOG,
  CONNECT_URL_ERROR,
  COPY_FEEDBACK_MS,
  ENGINE_COMING_SOON,
  TLS_SELECT_ID,
  URL_PARSE_DEBOUNCE_MS,
  URL_PASTE_SETTLE_MS,
} from '../src/constants';
import { setLanguage } from '../src/i18n';
import type { IntrospectResult, WasmApi } from '../src/types';
import { fakeTransport, frame, installDomStubs, mount, fakeWasm } from './helpers/harness';
import { fakeParser } from './helpers/url-parser';

const ids = CONNECT_DIALOG;
const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

beforeEach(() => {
  installDomStubs();
  Element.prototype.scrollIntoView = () => {};
  document.body.innerHTML = '';
  localStorage.setItem('dbmodeling.prefs', JSON.stringify({ lang: 'pt-BR' }));
  setLanguage('pt-BR');
});
afterEach(() => vi.unstubAllGlobals());

async function opened(parser = true) {
  const introspect = vi.fn(async (): Promise<IntrospectResult> => ({ dbml: 'Table a {\n  id int\n}\n', warnings: [], tables: 1 }));
  const transport = { ...fakeTransport(), introspect };
  const wasm = { ...fakeWasm(), ...(parser ? { parse_connection_url: fakeParser } : {}) } as unknown as WasmApi;
  const { root, workbench } = mount(transport, wasm);
  const wb = workbench as unknown as {
    runAction(id: string): void;
    paletteItems(): Array<{ id: string; label: string }>;
  };
  transport.update('Table users { id int }', undefined, { saved: true });
  await frame();
  wb.runAction('ConnectDatabase');
  const $ = <T extends HTMLElement>(sel: string) => root.querySelector(sel) as T;
  return { root, wb, introspect, $ };
}

const type = (el: HTMLInputElement, value: string) => {
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
};
const key = (el: HTMLElement, k: string) => el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }));

describe('dropdown do banco', () => {
  it('troca as abas por um combobox com listbox e Oracle desabilitado', async () => {
    const { root, $ } = await opened();
    expect(root.querySelector('[data-engine]')).toBeNull();
    const trigger = $<HTMLButtonElement>(`#${ids.ENGINE_TRIGGER_ID}`);
    expect(trigger.getAttribute('role')).toBe('combobox');
    expect(trigger.getAttribute('aria-haspopup')).toBe('listbox');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(trigger.getAttribute('aria-controls')).toBe(ids.ENGINE_LIST_ID);
    expect(trigger.textContent).toContain('PostgreSQL');
    const list = $(`#${ids.ENGINE_LIST_ID}`);
    expect(list.getAttribute('role')).toBe('listbox');
    expect(list.hidden).toBe(true);
    const options = [...list.querySelectorAll('[role="option"]')];
    expect(options.map((o) => o.textContent?.replace('em breve', '').trim())).toEqual(['PostgreSQL', 'MySQL', 'SQL Server', 'SQLite', 'Oracle']);
    const oracle = options.at(-1)!;
    expect(oracle.getAttribute('aria-disabled')).toBe('true');
    expect(oracle.textContent).toContain('em breve');
    expect(options[0].getAttribute('aria-selected')).toBe('true');
  });

  it('Enter, Espaço e seta para baixo abrem; setas navegam; Enter escolhe; o foco volta ao botão', async () => {
    const { $ } = await opened();
    const trigger = $<HTMLButtonElement>(`#${ids.ENGINE_TRIGGER_ID}`);
    for (const k of ['Enter', ' ', 'ArrowDown']) {
      trigger.focus();
      key(trigger, k);
      expect(trigger.getAttribute('aria-expanded'), k).toBe('true');
      expect($(`#${ids.ENGINE_LIST_ID}`).hidden).toBe(false);
      key(trigger, 'Escape');
      expect(trigger.getAttribute('aria-expanded')).toBe('false');
    }
    key(trigger, 'ArrowDown'); // abre na opção atual
    key(trigger, 'ArrowDown');
    key(trigger, 'ArrowDown');
    expect(trigger.getAttribute('aria-activedescendant')).toBe(`${ids.ENGINE_OPTION_ID_PREFIX}sqlserver`);
    key(trigger, 'Enter');
    expect(trigger.textContent).toContain('SQL Server');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(document.activeElement).toBe(trigger);
  });

  it('Esc fecha só a lista e devolve o foco; um segundo Esc fecha o diálogo', async () => {
    const { root, $ } = await opened();
    const trigger = $<HTMLButtonElement>(`#${ids.ENGINE_TRIGGER_ID}`);
    trigger.focus();
    key(trigger, 'ArrowDown');
    key(trigger, 'Escape');
    expect(document.activeElement).toBe(trigger);
    expect(root.querySelector('[role="dialog"]')).not.toBeNull();
    key(trigger, 'Escape');
    await wait(250);
    expect(root.querySelector('[role="dialog"]')).toBeNull();
  });

  it('Oracle não pode ser escolhido nem por teclado nem por clique', async () => {
    const { $ } = await opened();
    const trigger = $<HTMLButtonElement>(`#${ids.ENGINE_TRIGGER_ID}`);
    key(trigger, 'ArrowDown');
    for (let i = 0; i < 8; i++) key(trigger, 'ArrowDown');
    expect(trigger.getAttribute('aria-activedescendant')).toBe(`${ids.ENGINE_OPTION_ID_PREFIX}sqlite`);
    $(`#${ids.ENGINE_OPTION_ID_PREFIX}${ENGINE_COMING_SOON}`).click();
    expect(trigger.textContent).toContain('PostgreSQL');
  });

  it('clicar numa opção troca o banco e preserva os dados digitados', async () => {
    const { $ } = await opened();
    type($('#db-host'), 'h');
    type($('#db-user'), 'u');
    type($('#db-database'), 'db');
    type($(`#${ids.PASSWORD_ID}`), 'segredo');
    $<HTMLButtonElement>(`#${ids.ENGINE_TRIGGER_ID}`).click();
    $(`#${ids.ENGINE_OPTION_ID_PREFIX}mysql`).click();
    expect($<HTMLInputElement>('#db-host').value).toBe('h');
    expect($<HTMLInputElement>('#db-user').value).toBe('u');
    expect($<HTMLInputElement>('#db-database').value).toBe('db');
    expect($<HTMLInputElement>(`#${ids.PASSWORD_ID}`).value).toBe('segredo');
    expect($<HTMLInputElement>('#db-port').placeholder).toBe('3306');
    expect($<HTMLInputElement>(`#${ids.URL_ID}`).value).toBe('mysql://u:***@h/db');
  });

  it('SQLite mostra só o arquivo', async () => {
    const { $ } = await opened();
    $<HTMLButtonElement>(`#${ids.ENGINE_TRIGGER_ID}`).click();
    $(`#${ids.ENGINE_OPTION_ID_PREFIX}sqlite`).click();
    const visible = (sel: string) => [...document.querySelectorAll<HTMLElement>(sel)].every((n) => n.style.display !== 'none');
    expect(visible('[data-row="path"]')).toBe(true);
    for (const row of ['host', 'user', 'database', 'schemas', 'tls']) expect(visible(`[data-row="${row}"]`), row).toBe(false);
    expect(visible('[data-row="url"]')).toBe(true);
  });
});

describe('connection string', () => {
  it('colar uma string completa preenche os campos, o banco e o TLS', async () => {
    const { $ } = await opened();
    const url = $<HTMLInputElement>(`#${ids.URL_ID}`);
    type(url, 'mysql://u:p%40ss@h:5433/db?sslmode=require');
    await wait(URL_PARSE_DEBOUNCE_MS + 30);
    expect($<HTMLInputElement>('#db-host').value).toBe('h');
    expect($<HTMLInputElement>('#db-port').value).toBe('5433');
    expect($<HTMLInputElement>('#db-database').value).toBe('db');
    expect($<HTMLInputElement>('#db-user').value).toBe('u');
    expect($<HTMLInputElement>(`#${ids.PASSWORD_ID}`).value).toBe('p@ss');
    expect($<HTMLSelectElement>(`#${TLS_SELECT_ID}`).value).toBe('require');
    expect($(`#${ids.ENGINE_TRIGGER_ID}`).textContent).toContain('MySQL');
  });

  it('o texto colado é mascarado assim que entra', async () => {
    const { $ } = await opened();
    const url = $<HTMLInputElement>(`#${ids.URL_ID}`);
    url.value = 'postgres://u:segredo@h/db';
    url.dispatchEvent(new Event('paste', { bubbles: true }));
    await wait(URL_PASTE_SETTLE_MS + 30);
    expect(url.value).toBe('postgres://u:***@h/db');
    expect($<HTMLInputElement>(`#${ids.PASSWORD_ID}`).value).toBe('segredo');
  });

  it('editar um campo reescreve a string mascarada', async () => {
    const { $ } = await opened();
    type($('#db-host'), 'h');
    type($('#db-user'), 'u');
    type($(`#${ids.PASSWORD_ID}`), 'p@ss');
    expect($<HTMLInputElement>(`#${ids.URL_ID}`).value).toBe('postgres://u:***@h');
  });

  it('string inválida: erro inline associado, campos mantidos, sem toast, e some ao corrigir', async () => {
    const { root, $ } = await opened();
    type($('#db-host'), 'h');
    const url = $<HTMLInputElement>(`#${ids.URL_ID}`);
    type(url, 'ftp://x');
    await wait(URL_PARSE_DEBOUNCE_MS + 30);
    const err = $(`#${ids.URL_ERROR_ID}`);
    expect(err.textContent).toContain('Esquema desconhecido');
    expect(url.getAttribute('aria-invalid')).toBe('true');
    expect(url.getAttribute('aria-describedby')).toBe(ids.URL_ERROR_ID);
    expect($<HTMLInputElement>('#db-host').value).toBe('h');
    expect(root.querySelector('#toast')?.textContent).toBe('');
    expect(root.querySelectorAll('[data-modal]')).toHaveLength(1);
    type(url, 'postgres://u@h2/db');
    await wait(URL_PARSE_DEBOUNCE_MS + 30);
    expect(err.textContent).toBe('');
    expect(url.getAttribute('aria-invalid')).toBe('false');
    expect(url.hasAttribute('aria-describedby')).toBe(false);
    expect($<HTMLInputElement>('#db-host').value).toBe('h2');
  });

  it('a mensagem sai em inglês com o idioma en', async () => {
    localStorage.setItem('dbmodeling.prefs', JSON.stringify({ lang: 'en' }));
    const { $ } = await opened();
    type($(`#${ids.URL_ID}`), 'ftp://x');
    await wait(URL_PARSE_DEBOUNCE_MS + 30);
    expect($(`#${ids.URL_ERROR_ID}`).textContent).toContain('Unknown scheme');
  });

  it('sem o parser Wasm o campo da string é somente leitura', async () => {
    const { $ } = await opened(false);
    expect($<HTMLInputElement>(`#${ids.URL_ID}`).readOnly).toBe(true);
  });

  it('o código de erro mapeia para texto em todos os casos', () => {
    expect(Object.values(CONNECT_URL_ERROR)).toHaveLength(6);
  });
});

describe('senha', () => {
  it('começa oculta no campo e na string; o olho revela os dois e indica pressionado', async () => {
    const { $ } = await opened();
    type($('#db-host'), 'h');
    type($('#db-user'), 'u');
    type($(`#${ids.PASSWORD_ID}`), 'p@ss');
    const pwd = $<HTMLInputElement>(`#${ids.PASSWORD_ID}`);
    const toggle = $<HTMLButtonElement>(`#${ids.PASSWORD_TOGGLE_ID}`);
    const url = $<HTMLInputElement>(`#${ids.URL_ID}`);
    expect(pwd.type).toBe('password');
    expect(url.value).toBe('postgres://u:***@h');
    expect(toggle.getAttribute('aria-pressed')).toBe('false');
    expect(toggle.getAttribute('aria-label')).toBe('Mostrar senha');
    toggle.click();
    expect(pwd.type).toBe('text');
    expect(url.value).toBe('postgres://u:p%40ss@h');
    expect(toggle.getAttribute('aria-pressed')).toBe('true');
    expect(toggle.title).toBe('Ocultar senha');
    toggle.click();
    expect(pwd.type).toBe('password');
    expect(url.value).toBe('postgres://u:***@h');
  });

  it('volta a oculto ao trocar de banco', async () => {
    const { $ } = await opened();
    type($('#db-host'), 'h');
    type($('#db-user'), 'u');
    type($(`#${ids.PASSWORD_ID}`), 'p@ss');
    $<HTMLButtonElement>(`#${ids.PASSWORD_TOGGLE_ID}`).click();
    $<HTMLButtonElement>(`#${ids.ENGINE_TRIGGER_ID}`).click();
    $(`#${ids.ENGINE_OPTION_ID_PREFIX}mysql`).click();
    expect($<HTMLInputElement>(`#${ids.PASSWORD_ID}`).type).toBe('password');
    expect($<HTMLButtonElement>(`#${ids.PASSWORD_TOGGLE_ID}`).getAttribute('aria-pressed')).toBe('false');
    expect($<HTMLInputElement>(`#${ids.URL_ID}`).value).toContain(':***@');
  });

  it('volta a oculto ao conectar', async () => {
    const { $, introspect } = await opened();
    type($('#db-host'), 'h');
    type($('#db-user'), 'u');
    type($(`#${ids.PASSWORD_ID}`), 'p@ss');
    $<HTMLButtonElement>(`#${ids.PASSWORD_TOGGLE_ID}`).click();
    $<HTMLButtonElement>('#db-connect').click();
    await frame();
    expect(introspect).toHaveBeenCalledTimes(1);
    expect(introspect.mock.calls[0]).toBeDefined();
    expect($<HTMLInputElement>(`#${ids.PASSWORD_ID}`).type).toBe('password');
  });

  it('o estado de revelação não é persistido: reabrir o diálogo começa oculto', async () => {
    const { root, wb, $ } = await opened();
    $<HTMLButtonElement>(`#${ids.PASSWORD_TOGGLE_ID}`).click();
    key($('#db-form'), 'Escape'); // Esc fora do dropdown fecha o diálogo
    await wait(250);
    wb.runAction('ConnectDatabase');
    expect(root.querySelector<HTMLInputElement>(`#${ids.PASSWORD_ID}`)!.type).toBe('password');
    expect(JSON.stringify(Object.entries(localStorage))).not.toContain('Revealed');
  });
});

describe('copiar', () => {
  const stubClipboard = () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    return writeText;
  };

  it('copia a string mascarada com a senha oculta e a completa com a senha revelada', async () => {
    const writeText = stubClipboard();
    const { $ } = await opened();
    type($('#db-host'), 'h');
    type($('#db-user'), 'u');
    type($(`#${ids.PASSWORD_ID}`), 'p@ss');
    const copy = $<HTMLButtonElement>(`#${ids.URL_COPY_ID}`);
    expect(copy.title).toContain('mascarada');
    copy.click();
    await wait(10);
    expect(writeText).toHaveBeenLastCalledWith('postgres://u:***@h');
    expect($(`#${ids.URL_COPY_LABEL_ID}`).textContent).toBe('Copiado');
    $<HTMLButtonElement>(`#${ids.PASSWORD_TOGGLE_ID}`).click();
    expect(copy.title).toContain('completa');
    copy.click();
    await wait(10);
    expect(writeText).toHaveBeenLastCalledWith('postgres://u:p%40ss@h');
    await wait(COPY_FEEDBACK_MS + 50);
    expect($(`#${ids.URL_COPY_LABEL_ID}`).textContent).toBe('Copiar string');
  });
});

describe('command palette', () => {
  it('a ação de mostrar/ocultar senha só existe com o diálogo aberto', async () => {
    const { wb, $ } = await opened();
    expect(wb.paletteItems().some((a) => a.id === 'ToggleConnectPassword')).toBe(true);
    wb.runAction('OpenPalette');
    // a paleta abre por cima: o diálogo continua montado
    expect(document.querySelectorAll('[data-modal]')).toHaveLength(2);
    expect($(`#${ids.PASSWORD_TOGGLE_ID}`)).not.toBeNull();
    const row = [...document.querySelectorAll<HTMLElement>('#pal-list .it')].find((n) => n.textContent?.includes('Mostrar/ocultar senha'));
    expect(row).toBeDefined();
    row!.click();
    expect($<HTMLButtonElement>(`#${ids.PASSWORD_TOGGLE_ID}`).getAttribute('aria-pressed')).toBe('true');
    expect(document.activeElement).toBe($(`#${ids.PASSWORD_TOGGLE_ID}`));
    await wait(250);
    expect(document.querySelectorAll('[data-modal]')).toHaveLength(1);
    wb.runAction('ToggleConnectPassword');
    expect($<HTMLButtonElement>(`#${ids.PASSWORD_TOGGLE_ID}`).getAttribute('aria-pressed')).toBe('false');
  });

  it('sem o diálogo a ação some da paleta e não faz nada', async () => {
    const { wb, $ } = await opened();
    key($('#db-form'), 'Escape');
    await wait(250);
    expect(wb.paletteItems().some((a) => a.id === 'ToggleConnectPassword')).toBe(false);
    expect(() => wb.runAction('ToggleConnectPassword')).not.toThrow();
  });

  it('o catálogo traz a ação no grupo Arquivo, sem atalho, em pt-BR e en', () => {
    const entry = catalog().find((a) => a.id === 'ToggleConnectPassword');
    expect(entry).toMatchObject({ group: 'Arquivo', label: 'Mostrar/ocultar senha da conexão', key: '' });
    setLanguage('en');
    expect(catalog().find((a) => a.id === 'ToggleConnectPassword')?.label).toBe('Show/hide connection password');
  });
});

describe('acessibilidade do diálogo', () => {
  it('não tem violações do axe-core, com a lista aberta e com erro de string', async () => {
    const { root, $ } = await opened();
    const trigger = $<HTMLButtonElement>(`#${ids.ENGINE_TRIGGER_ID}`);
    key(trigger, 'ArrowDown');
    type($(`#${ids.URL_ID}`), 'ftp://x');
    await wait(URL_PARSE_DEBOUNCE_MS + 30);
    const results = await axe.run(root, { rules: { 'color-contrast': { enabled: false }, region: { enabled: false } } });
    expect(results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(' | ')}`)).toEqual([]);
  });

  it('todo controle do diálogo tem nome acessível e o foco inicial vai para a string', async () => {
    const { root, $ } = await opened();
    expect(document.activeElement).toBe($(`#${ids.URL_ID}`));
    const dialog = root.querySelector('[role="dialog"]')!;
    for (const el of dialog.querySelectorAll<HTMLElement>('input, button, select')) {
      const name = el.getAttribute('aria-label') || el.getAttribute('aria-labelledby') || el.textContent?.trim()
        || (el.id && dialog.querySelector(`label[for="${el.id}"]`)?.textContent) || el.closest('label')?.textContent || el.title;
      expect(name, el.id || el.outerHTML.slice(0, 60)).toBeTruthy();
    }
  });
});
