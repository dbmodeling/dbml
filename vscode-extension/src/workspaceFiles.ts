import * as vscode from 'vscode';

/** Caminho legível para tooltip/lista; evita `fsPath` em esquemas virtuais (vscode-vfs etc.). */
export function displayPath(uri: vscode.Uri): string {
  return uri.scheme === 'file' ? uri.fsPath : uri.toString(true);
}

/** Ordenação estável por caminho, sem depender de `fsPath`. */
export function byPath(a: vscode.Uri, b: vscode.Uri): number {
  return a.path.localeCompare(b.path);
}

/** Teto de resultados para não varrer monorepos gigantes inteiros. */
export const MAX_DBML_FILES = 1000;

/**
 * Lista os `.dbml` do workspace respeitando `files.exclude` do usuário
 * (`exclude` indefinido = exclusões padrão do VS Code; um glob fixo as substituiria).
 */
export function findDbmlFiles(max: number = MAX_DBML_FILES): Thenable<vscode.Uri[]> {
  return vscode.workspace.findFiles('**/*.dbml', undefined, max);
}
