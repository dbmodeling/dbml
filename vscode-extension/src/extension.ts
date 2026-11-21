import type * as vscode from 'vscode';
import { activateExtension } from './activation.js';
import { nodeCliHost } from './cliHost.node.js';

/** Entrada do extension host Node (desktop/remoto): inclui a execução da CLI dbml. */
export function activate(context: vscode.ExtensionContext): void {
  activateExtension(context, nodeCliHost);
}

export function deactivate(): void {}
