import * as vscode from 'vscode';

/** Teto de resultados para não varrer monorepos gigantes inteiros. */
export const MAX_DBML_FILES = 1000;

/**
 * Lista os `.dbml` do workspace respeitando `files.exclude` do usuário
 * (`exclude` indefinido = exclusões padrão do VS Code; um glob fixo as substituiria).
 */
export function findDbmlFiles(max: number = MAX_DBML_FILES): Thenable<vscode.Uri[]> {
  return vscode.workspace.findFiles('**/*.dbml', undefined, max);
}
