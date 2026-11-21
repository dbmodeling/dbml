import * as vscode from 'vscode';

const VIEW_TYPE = 'dbmodeling.editor';
const PATTERN = '*.dbml';

export type DefaultEditor = 'text' | 'canvas';

/**
 * Nova `workbench.editorAssociations` para o modo escolhido, ou `undefined` se nada precisa mudar.
 * Só toca na associação `*.dbml` do próprio DB Modeling; o resto das associações do usuário é preservado.
 */
export function associationsFor(current: unknown, mode: DefaultEditor): Record<string, string> | undefined {
  if (current !== undefined && current !== null && (typeof current !== 'object' || Array.isArray(current))) return undefined;
  const map = { ...((current as Record<string, string> | undefined) ?? {}) };
  if (mode === 'canvas') {
    if (map[PATTERN] === VIEW_TYPE) return undefined;
    map[PATTERN] = VIEW_TYPE;
    return map;
  }
  if (map[PATTERN] !== VIEW_TYPE) return undefined;
  delete map[PATTERN];
  return map;
}

function readMode(): DefaultEditor {
  const info = vscode.workspace.getConfiguration('dbmodeling').inspect<string>('defaultEditor');
  return info?.globalValue === 'canvas' ? 'canvas' : 'text';
}

/** Reflete `dbmodeling.defaultEditor` em `workbench.editorAssociations` (usuário). */
export async function syncDefaultEditor(): Promise<void> {
  const workbench = vscode.workspace.getConfiguration('workbench');
  const current = workbench.inspect<unknown>('editorAssociations')?.globalValue;
  const next = associationsFor(current, readMode());
  if (next) await workbench.update('editorAssociations', next, vscode.ConfigurationTarget.Global);
}
