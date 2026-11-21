import type * as vscode from 'vscode';
import { activateExtension } from './activation.js';
import { webCliHost } from './cliHost.web.js';

/** Entrada do extension host web (vscode.dev): canvas e edição funcionam; conexão com banco não. */
export function activate(context: vscode.ExtensionContext): void {
  activateExtension(context, webCliHost);
}

export function deactivate(): void {}
