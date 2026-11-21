import { afterEach, describe, expect, it } from 'vitest';
import { summarizeTest } from '../src/connect-db';
import { connectionHint, diagnosticMessage, setLanguage, systemLanguage, t, translations } from '../src/i18n';

afterEach(() => setLanguage('pt-BR'));

describe('i18n', () => {
  it('has unique keys and nonempty translations', () => {
    expect(new Set(translations.map(([key]) => key)).size).toBe(translations.length);
    expect(translations.every(([key, value]) => key.length > 0 && value.trim().length > 0)).toBe(true);
  });

  it('switches five texts and uses Portuguese as the fallback', () => {
    const keys = ['Arquivo', 'Salvar', 'Nova tabela', 'Translúcido', 'Atalhos de teclado'];
    setLanguage('pt-BR');
    expect(keys.map((key) => t(key))).toEqual(keys);
    setLanguage('en');
    expect(keys.map((key) => t(key))).toEqual(['File', 'Save', 'New table', 'Translucent', 'Keyboard shortcuts']);
    expect(t('Texto desconhecido')).toBe('Texto desconhecido');
    expect(t('Adicionar predefinição de coluna: {}', 'UUID')).toBe('Add column preset: UUID');
  });

  it('translates interpolated connection summaries', () => {
    setLanguage('en');
    expect(summarizeTest('db', ['public'])).toBe('Connected to db. Schemas: public.');
    expect(summarizeTest('db', Array(9).fill('s'))).toContain('(+1 more)');
  });

  it('localizes common diagnostics without altering schema identifiers', () => {
    setLanguage('pt-BR');
    expect(diagnosticMessage('Table without a name')).toBe('Tabela sem nome');
    expect(diagnosticMessage('Duplicate table `sales.users`')).toBe('Tabela duplicada `sales.users`');
    expect(diagnosticMessage('unknown diagnostic text')).toBe('unknown diagnostic text');
    setLanguage('en');
    expect(diagnosticMessage('Table without a name')).toBe('Table without a name');
  });

  it('offers recovery without showing driver details', () => {
    setLanguage('pt-BR');
    expect(connectionHint('error 28P01', false)).toContain('Confira o usuário e a senha');
    expect(connectionHint('connection refused', false)).toContain('Confira o host, a porta');
    expect(connectionHint('unable to open database file', true)).toContain('caminho e as permissões');
    expect(connectionHint('could not read the catalog: driver detail', false)).toContain('permissões de leitura');
  });

  it('chooses Portuguese for Portuguese system locales', () => {
    expect(systemLanguage('pt-PT')).toBe('pt-BR');
    expect(systemLanguage('en-US')).toBe('en');
  });
});
