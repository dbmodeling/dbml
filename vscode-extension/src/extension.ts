import * as vscode from 'vscode';
import { DbmlCustomEditorProvider } from './customEditor.js';
import { syncDefaultEditor } from './editorAssociation.js';
import { findDbmlFiles } from './workspaceFiles.js';

/** Árvore de arquivos `.dbml` do workspace exibida na Activity Bar. */
class DbmlWorkspaceTreeProvider implements vscode.TreeDataProvider<vscode.Uri> {
  private readonly emitter = new vscode.EventEmitter<void>();
  public readonly onDidChangeTreeData = this.emitter.event;

  refresh(): void {
    this.emitter.fire();
  }

  getTreeItem(uri: vscode.Uri): vscode.TreeItem {
    const item = new vscode.TreeItem(vscode.workspace.asRelativePath(uri, (vscode.workspace.workspaceFolders?.length ?? 0) > 1));
    item.resourceUri = uri;
    item.iconPath = new vscode.ThemeIcon('database');
    item.tooltip = uri.fsPath;
    item.command = {
      command: 'dbmodeling.openCanvas',
      title: 'Abrir no DB Modeling',
      arguments: [uri],
    };
    return item;
  }

  async getChildren(): Promise<vscode.Uri[]> {
    const uris = await findDbmlFiles();
    return uris.sort((a, b) => a.fsPath.localeCompare(b.fsPath));
  }
}

async function openInCanvas(uri: vscode.Uri, column?: vscode.ViewColumn): Promise<void> {
  await vscode.commands.executeCommand('vscode.openWith', uri, DbmlCustomEditorProvider.viewType, column);
}

/** Abre o Workbench: arquivo ativo → primeiro `.dbml` → escolha do usuário. */
async function openWorkbench(context: vscode.ExtensionContext): Promise<void> {
  const active = vscode.window.activeTextEditor?.document.uri;
  if (active && active.path.toLowerCase().endsWith('.dbml')) {
    return openInCanvas(active);
  }
  const files = await findDbmlFiles(1);
  if (files.length > 0) {
    return openInCanvas(files[0]);
  }
  const choice = await vscode.window.showQuickPick(['Criar novo esquema', 'Abrir exemplo'], {
    placeHolder: 'Nenhum esquema DBML encontrado',
  });
  if (choice === 'Criar novo esquema') return newFile();
  if (choice !== 'Abrir exemplo') return;
  const bytes = await vscode.workspace.fs.readFile(vscode.Uri.joinPath(context.extensionUri, 'media', 'sample.dbml'));
  const sample = new TextDecoder().decode(bytes);
  const doc = await vscode.workspace.openTextDocument({ language: 'dbml', content: sample });
  return openInCanvas(doc.uri);
}

async function newFile(): Promise<void> {
  const content = 'Table example {\n  id int [pk]\n  name varchar(255)\n}\n';
  const folder = vscode.workspace.workspaceFolders?.[0]?.uri;
  if (!folder) {
    const doc = await vscode.workspace.openTextDocument({ language: 'dbml', content });
    return openInCanvas(doc.uri);
  }
  let n = 0;
  let target: vscode.Uri;
  for (;;) {
    target = vscode.Uri.joinPath(folder, n === 0 ? 'new-schema.dbml' : `new-schema-${n}.dbml`);
    try {
      await vscode.workspace.fs.stat(target);
      n++;
    } catch {
      break;
    }
  }
  await vscode.workspace.fs.writeFile(target, new TextEncoder().encode(content));
  await openInCanvas(target);
}

export function activate(context: vscode.ExtensionContext): void {
  const diagnostics = vscode.languages.createDiagnosticCollection('dbml');
  context.subscriptions.push(diagnostics);
  context.subscriptions.push(DbmlCustomEditorProvider.register(context, diagnostics));

  // `dbmodeling.defaultEditor` → workbench.editorAssociations (o editor customizado tem priority "option")
  void syncDefaultEditor();
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('dbmodeling.defaultEditor')) void syncDefaultEditor();
    })
  );

  // Árvore na Activity Bar (ícone do DB Modeling na barra lateral esquerda)
  const tree = new DbmlWorkspaceTreeProvider();
  context.subscriptions.push(vscode.window.registerTreeDataProvider('dbmodeling.workspaceView', tree));
  const watcher = vscode.workspace.createFileSystemWatcher('**/*.dbml');
  watcher.onDidCreate(() => tree.refresh());
  watcher.onDidDelete(() => tree.refresh());
  context.subscriptions.push(watcher);

  context.subscriptions.push(
    vscode.commands.registerCommand('dbmodeling.openWorkbench', () => openWorkbench(context)),
    vscode.commands.registerCommand('dbmodeling.newFile', async () => {
      await newFile();
      tree.refresh();
    }),
    vscode.commands.registerCommand('dbmodeling.refresh', () => tree.refresh()),
    vscode.commands.registerCommand('dbmodeling.openCanvas', async (uri?: vscode.Uri) => {
      const target = uri || vscode.window.activeTextEditor?.document.uri;
      if (target && target.path.toLowerCase().endsWith('.dbml')) {
        await openInCanvas(target);
      } else {
        await openWorkbench(context);
      }
    })
  );

  // Botão sempre visível na barra de status do VS Code
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  status.text = '$(database) DB Modeling';
  status.tooltip = 'Abrir DB Modeling';
  status.command = 'dbmodeling.openWorkbench';
  status.show();
  context.subscriptions.push(status);
}

export function deactivate(): void {}
