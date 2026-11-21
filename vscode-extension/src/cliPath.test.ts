import { beforeEach, describe, expect, it } from 'vitest';
import { DEFAULT_CLI_PATH, getTrustedCliPath, readCliPathSetting } from './cliPath.js';
import { fakeContext } from './test/harness.js';
import { mockState, resetMock } from './test/vscode-mock.js';

beforeEach(() => resetMock());

describe('readCliPathSetting', () => {
  it('ignora o valor do workspace e da pasta (repositório não define o binário)', () => {
    mockState.config['dbmodeling.cliPath'] = {
      defaultValue: 'dbml',
      globalValue: undefined,
      workspaceValue: '/tmp/evil',
      workspaceFolderValue: '/tmp/evil2',
    };
    expect(readCliPathSetting()).toBe(DEFAULT_CLI_PATH);
  });

  it('usa o valor do usuário quando existe', () => {
    mockState.config['dbmodeling.cliPath'] = { defaultValue: 'dbml', globalValue: ' /opt/dbml ', workspaceValue: '/tmp/evil' };
    expect(readCliPathSetting()).toBe('/opt/dbml');
  });

  it('cai no padrão se vazio ou sem configuração', () => {
    mockState.config['dbmodeling.cliPath'] = { defaultValue: 'dbml', globalValue: '  ' };
    expect(readCliPathSetting()).toBe('dbml');
    mockState.config = {};
    expect(readCliPathSetting()).toBe('dbml');
  });
});

describe('getTrustedCliPath', () => {
  it('workspace não confiável bloqueia a introspecção', async () => {
    mockState.isTrusted = false;
    const { context } = fakeContext();
    await expect(getTrustedCliPath(context)).rejects.toThrow(/trust/i);
  });

  it('binário padrão não pede confirmação', async () => {
    const { context } = fakeContext();
    await expect(getTrustedCliPath(context)).resolves.toBe('dbml');
    expect(mockState.messages.warnings).toHaveLength(0);
  });

  it('caminho personalizado pede confirmação uma única vez', async () => {
    mockState.config['dbmodeling.cliPath'] = { defaultValue: 'dbml', globalValue: '/opt/dbml' };
    mockState.warningAnswers = ['Allow'];
    const { context } = fakeContext();
    await expect(getTrustedCliPath(context)).resolves.toBe('/opt/dbml');
    expect(mockState.messages.warnings).toHaveLength(1);
    expect(mockState.messages.warnings[0].message).toContain('/opt/dbml');
    // segunda vez: já aprovado
    await expect(getTrustedCliPath(context)).resolves.toBe('/opt/dbml');
    expect(mockState.messages.warnings).toHaveLength(1);
  });

  it('recusar a confirmação bloqueia e não memoriza', async () => {
    mockState.config['dbmodeling.cliPath'] = { defaultValue: 'dbml', globalValue: '/opt/dbml' };
    mockState.warningAnswers = [undefined, 'Allow'];
    const { context } = fakeContext();
    await expect(getTrustedCliPath(context)).rejects.toThrow(/not allowed/i);
    await expect(getTrustedCliPath(context)).resolves.toBe('/opt/dbml');
  });

  it('mudar o caminho exige nova confirmação', async () => {
    mockState.config['dbmodeling.cliPath'] = { defaultValue: 'dbml', globalValue: '/opt/a' };
    mockState.warningAnswers = ['Allow', 'Allow'];
    const { context } = fakeContext();
    await getTrustedCliPath(context);
    mockState.config['dbmodeling.cliPath'] = { defaultValue: 'dbml', globalValue: '/opt/b' };
    await getTrustedCliPath(context);
    expect(mockState.messages.warnings).toHaveLength(2);
  });
});
