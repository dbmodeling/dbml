import * as vscode from 'vscode';

export const DEFAULT_CLI_PATH = 'dbml';
const APPROVED_KEY = 'dbmodeling.approvedCliPaths';
const ALLOW = 'Allow';

/**
 * Lê `dbmodeling.cliPath` ignorando valores de workspace/pasta: um repositório não pode
 * escolher qual binário a extensão executa (o processo recebe a senha do banco no stdin).
 */
export function readCliPathSetting(): string {
  const info = vscode.workspace.getConfiguration('dbmodeling').inspect<string>('cliPath');
  const user = typeof info?.globalValue === 'string' ? info.globalValue.trim() : '';
  if (user) return user;
  const fallback = typeof info?.defaultValue === 'string' ? info.defaultValue.trim() : '';
  return fallback || DEFAULT_CLI_PATH;
}

/**
 * Devolve o binário da CLI que pode ser executado agora:
 * exige workspace confiável e confirma, uma vez por caminho, qualquer binário diferente do padrão.
 */
export async function getTrustedCliPath(context: vscode.ExtensionContext): Promise<string> {
  if (!vscode.workspace.isTrusted) {
    throw new Error('Database connection is disabled in untrusted workspaces. Trust this workspace to use it.');
  }
  const path = readCliPathSetting();
  if (path === DEFAULT_CLI_PATH) return path;

  const approved = context.globalState.get<string[]>(APPROVED_KEY, []);
  if (approved.includes(path)) return path;

  const answer = await vscode.window.showWarningMessage(
    `DB Modeling will run "${path}" (dbmodeling.cliPath) and send it your database password. Allow?`,
    { modal: true },
    ALLOW
  );
  if (answer !== ALLOW) throw new Error(`Running "${path}" is not allowed. Change dbmodeling.cliPath or confirm the prompt.`);
  await context.globalState.update(APPROVED_KEY, [...approved, path]);
  return path;
}
