import * as vscode from 'vscode';
import { base64ToBytes } from './base64.js';
import type { CliHost } from './cliHost.js';
import { getTrustedCliPath } from './cliPath.js';
import { parseWebviewMessage, type WebviewMessage } from './messages.js';
import { sanitizeSession } from './session.js';
import { isAllowedDbmlUri } from './uriPolicy.js';
import { byPath, displayPath, findDbmlFiles } from './workspaceFiles.js';

/** Lista os `.dbml` do workspace no formato esperado pela aba Arquivos do Workbench. */
async function listDbmlFiles(active?: vscode.Uri): Promise<Array<{ name: string; path: string; active: boolean }>> {
  const uris = await findDbmlFiles();
  return uris
    .sort(byPath)
    .map((u) => ({
      name: vscode.workspace.asRelativePath(u, (vscode.workspace.workspaceFolders?.length ?? 0) > 1),
      path: u.toString(),
      active: !!active && u.toString() === active.toString(),
    }));
}

export class DbmlCustomEditorProvider implements vscode.CustomTextEditorProvider {
  public static readonly viewType = 'dbmodeling.editor';

  private readonly verifiedClis = new Set<string>();

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly cli: CliHost
  ) {}

  /** Caminho confiável da CLI, já verificado (presença + versão mínima) — a checagem é feita uma vez por caminho. */
  private async ensureCli(): Promise<string> {
    if (this.cli.unavailableReason) throw new Error(this.cli.unavailableReason);
    const path = await getTrustedCliPath(this.context);
    if (!this.verifiedClis.has(path)) {
      await this.cli.checkCli(path);
      this.verifiedClis.add(path);
    }
    return path;
  }

  public static register(
    context: vscode.ExtensionContext,
    diagnostics: vscode.DiagnosticCollection,
    cli: CliHost
  ): vscode.Disposable {
    const provider = new DbmlCustomEditorProvider(context, cli);
    provider.diagnostics = diagnostics;
    context.subscriptions.push(vscode.workspace.onDidCloseTextDocument((document) => diagnostics.delete(document.uri)));
    return vscode.window.registerCustomEditorProvider(DbmlCustomEditorProvider.viewType, provider, {
      // Webview oculta é descartada (libera DOM, canvas e instância Wasm); ao voltar, 'ready' reenvia texto, layout e sessão.
      webviewOptions: { retainContextWhenHidden: false },
      supportsMultipleEditorsPerDocument: true,
    });
  }

  private diagnostics?: vscode.DiagnosticCollection;
  private readonly edits = new WeakMap<vscode.TextDocument, Promise<void>>();

  public async resolveCustomTextEditor(
    document: vscode.TextDocument,
    webviewPanel: vscode.WebviewPanel,
    _token: vscode.CancellationToken
  ): Promise<void> {
    const webview = webviewPanel.webview;
    webview.options = {
      enableScripts: true,
      localResourceRoots: [vscode.Uri.joinPath(this.context.extensionUri, 'dist')],
    };
    webview.html = this.getHtmlForWebview(webview);

    const sidecarUri = this.getSidecarUri(document.uri);
    // Sidecar em `file:` e em workspaces virtuais (mesmo esquema de uma pasta aberta, via workspace.fs); `untitled:`, `git:` etc. ficam sem.
    const canSidecar =
      document.uri.scheme === 'file' ||
      (vscode.workspace.workspaceFolders ?? []).some((f) => f.uri.scheme === document.uri.scheme);
    let layoutData: unknown = null;
    if (canSidecar) {
      try {
        const bytes = await vscode.workspace.fs.readFile(sidecarUri);
        layoutData = JSON.parse(new TextDecoder().decode(bytes));
      } catch {
        // Sidecar ainda não existe; será criado quando o usuário mover tabelas
      }
    }

    const sessionKey = `dbmodeling.docSession:${document.uri.toString()}`;
    const documentId = document.uri.toString();
    const title = () => vscode.workspace.asRelativePath(document.uri, (vscode.workspace.workspaceFolders?.length ?? 0) > 1);
    const sendUpdate = async (withLayout: boolean) => {
      const files = await listDbmlFiles(document.uri);
      let savedSource: string | undefined;
      if (withLayout && document.isDirty) {
        try {
          savedSource = canSidecar
            ? new TextDecoder().decode(await vscode.workspace.fs.readFile(document.uri))
            : '';
        } catch { /* The host still reports its dirty state. */ }
      }
      lastPosted = { source: document.getText(), saved: !document.isDirty };
      webview.postMessage({
        type: 'update',
        documentId,
        saved: !document.isDirty,
        savedSource,
        autosave: !document.isUntitled,
        source: document.getText(),
        layout: withLayout ? layoutData : undefined,
        title: title(),
        files,
        session: withLayout ? sanitizeSession(this.context.workspaceState.get(sessionKey)) : undefined,
      });
    };

    // Alterações textuais (digitação no editor nativo, Ctrl+Z, outros editores): reenviadas com debounce.
    // O Workbench ignora o eco quando o texto já é idêntico ao que ele aplicou. Atualizações levam só texto/estado
    // (a lista de arquivos só é recalculada em criar/excluir/renomear) e são puladas quando nada mudou ou a
    // aba está oculta (ao reaparecer, o 'ready' reenvia tudo).
    let lastPosted: { source: string; saved: boolean } | null = null;
    const postTextUpdate = (extra: Record<string, unknown> = {}) => {
      if (!webviewPanel.visible) return;
      const source = document.getText();
      const saved = !document.isDirty;
      if (lastPosted && lastPosted.source === source && lastPosted.saved === saved) return;
      lastPosted = { source, saved };
      void webview.postMessage({ type: 'update', source, documentId, saved, ...extra });
    };
    let changeTimer: ReturnType<typeof setTimeout> | null = null;
    const changeSub = vscode.workspace.onDidChangeTextDocument((e) => {
      if (e.document.uri.toString() !== document.uri.toString()) return;
      if (changeTimer) clearTimeout(changeTimer);
      changeTimer = setTimeout(() => postTextUpdate({ title: title() }), 80);
    });
    const saveSub = vscode.workspace.onDidSaveTextDocument((savedDocument) => {
      if (savedDocument.uri.toString() !== documentId) return;
      postTextUpdate();
    });
    const filesSub = [
      vscode.workspace.onDidCreateFiles(() => void refreshFiles()),
      vscode.workspace.onDidDeleteFiles(() => void refreshFiles()),
      vscode.workspace.onDidRenameFiles(() => void refreshFiles()),
    ];
    const refreshFiles = async () => {
      webview.postMessage({
        type: 'workspace_files',
        files: await listDbmlFiles(document.uri),
        title: title(),
      });
    };

    // Falhas de gravação aparecem uma vez por arquivo e tipo (não a cada arraste de tabela).
    const warned = new Set<string>();
    const warnOnce = (kind: 'sidecar' | 'edit', text: string) => {
      if (warned.has(kind)) return;
      warned.add(kind);
      void vscode.window.showWarningMessage(text);
    };

    const handleMessage = async (message: WebviewMessage) => {
      try {
        switch (message.command) {
          case 'apply_patch': {
            const patch = message.patch;
            if (patch.expected_source !== document.getText()) throw new Error('Document changed outside this editor; reload or reconcile changes');
            const range = new vscode.Range(
              new vscode.Position(patch.range.start.line, patch.range.start.character),
              new vscode.Position(patch.range.end.line, patch.range.end.character)
            );
            const edit = new vscode.WorkspaceEdit();
            edit.replace(document.uri, range, patch.new_text);
            // Integra com o histórico nativo (Ctrl+Z / Ctrl+Y)
            if (!await vscode.workspace.applyEdit(edit)) {
              warnOnce('edit', `Could not apply the change to ${title()}: the editor rejected the edit (is the file read-only?).`);
              throw new Error('Could not apply edit');
            }
            break;
          }
          case 'save_document': {
            if (message.expected_source !== document.getText()) throw new Error('Document changed outside this editor; reload or reconcile changes');
            if (!await document.save()) throw new Error('Could not save document');
            break;
          }
          case 'save_layout': {
            if (!canSidecar) {
              layoutData = message.layout;
              break;
            }
            const temp = sidecarUri.with({ path: `${sidecarUri.path}.tmp-${Date.now()}-${Math.random().toString(36).slice(2)}` });
            try {
              const json = JSON.stringify(message.layout, (_key, value) =>
                value && typeof value === 'object' && !Array.isArray(value)
                  ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, value[key]]))
                  : value, 2);
              await vscode.workspace.fs.writeFile(temp, new TextEncoder().encode(json));
              await vscode.workspace.fs.rename(temp, sidecarUri, { overwrite: true });
              layoutData = message.layout;
            } catch (err) {
              try { await vscode.workspace.fs.delete(temp); } catch { /* temporary file may not exist */ }
              const cause = err instanceof Error ? err.message : String(err);
              warnOnce('sidecar', `Could not save the layout of ${title()} (${cause}). Table positions will not be kept.`);
              throw err;
            }
            break;
          }
          case 'diagnostics': {
            const list = message.diagnostics.map((d) => {
              const sev =
                d.severity === 'error'
                  ? vscode.DiagnosticSeverity.Error
                  : d.severity === 'warning'
                    ? vscode.DiagnosticSeverity.Warning
                    : vscode.DiagnosticSeverity.Information;
              return new vscode.Diagnostic(
                new vscode.Range(d.range.start.line, d.range.start.character, d.range.end.line, d.range.end.character),
                d.message,
                sev
              );
            });
            this.diagnostics?.set(document.uri, list);
            break;
          }
          case 'export_file': {
            const { filename, content, isBinary } = message;
            const dir = document.uri.scheme === 'file' ? vscode.Uri.joinPath(document.uri, '..') : undefined;
            const saveUri = await vscode.window.showSaveDialog({
              defaultUri: dir ? vscode.Uri.joinPath(dir, filename) : undefined,
              saveLabel: 'Save export',
            });
            if (saveUri) {
              const data = isBinary ? base64ToBytes(content) : new TextEncoder().encode(content);
              await vscode.workspace.fs.writeFile(saveUri, data);
              vscode.window.showInformationMessage(`Arquivo exportado: ${displayPath(saveUri)}`);
            }
            break;
          }
          case 'check_cli': {
            await this.ensureCli();
            break;
          }
          case 'test_connection': {
            try {
              const result = await this.cli.testConnection(await this.ensureCli(), message.request);
              webview.postMessage({ type: 'test_connection_result', id: message.id, result });
            } catch (e) {
              webview.postMessage({
                type: 'test_connection_result',
                id: message.id,
                error: e instanceof Error ? e.message : String(e),
              });
            }
            break;
          }
          case 'introspect': {
            try {
              const result = await this.cli.introspect(await this.ensureCli(), message.request);
              webview.postMessage({ type: 'introspect_result', id: message.id, result });
            } catch (e) {
              webview.postMessage({
                type: 'introspect_result',
                id: message.id,
                error: e instanceof Error ? e.message : String(e),
              });
            }
            break;
          }
          case 'open_file': {
            let target: vscode.Uri;
            try {
              target = vscode.Uri.parse(message.path, true);
            } catch {
              throw new Error('Invalid file path');
            }
            const isCurrent = target.toString() === documentId;
            if (!isCurrent && !isAllowedDbmlUri(target, vscode.workspace.workspaceFolders?.map((f) => f.uri) ?? [])) {
              throw new Error('Only .dbml files inside the workspace can be opened');
            }
            await vscode.commands.executeCommand('vscode.openWith', target, DbmlCustomEditorProvider.viewType);
            break;
          }
          case 'new_file': {
            await vscode.commands.executeCommand('dbmodeling.newFile');
            break;
          }
          case 'save_session': {
            await this.context.workspaceState.update(sessionKey, sanitizeSession(message.session));
            break;
          }
          case 'ready': {
            await sendUpdate(true);
            break;
          }
        }
        if (message.id != null && ['apply_patch', 'save_document', 'save_layout', 'open_file', 'check_cli', 'export_file'].includes(message.command)) {
          await webview.postMessage({ type: 'result', id: message.id });
        }
      } catch (err) {
        if (message.id != null) await webview.postMessage({ type: 'result', id: message.id, error: err instanceof Error ? err.message : String(err) });
        else console.error(err);
      }
    };
    webview.onDidReceiveMessage((raw: unknown) => {
      const parsed = parseWebviewMessage(raw);
      if (!parsed.ok) {
        console.warn(`[dbmodeling] mensagem da Webview descartada: ${parsed.reason}`);
        if (parsed.id != null) {
          const type = parsed.command === 'introspect' || parsed.command === 'test_connection' ? `${parsed.command}_result` : 'result';
          void webview.postMessage({ type, id: parsed.id, error: `Invalid message (${parsed.reason})` });
        }
        return;
      }
      const message = parsed.message;
      if (['apply_patch', 'save_document', 'save_layout'].includes(message.command)) {
        const previous = this.edits.get(document) ?? Promise.resolve();
        const next = previous.then(() => handleMessage(message));
        this.edits.set(document, next.catch((error) => console.error(error)));
      } else {
        void handleMessage(message).catch((error) => console.error(error));
      }
    });

    webviewPanel.onDidDispose(() => {
      if (changeTimer) clearTimeout(changeTimer);
      changeSub.dispose();
      saveSub.dispose();
      filesSub.forEach((s) => s.dispose());
    });
  }

  private getSidecarUri(documentUri: vscode.Uri): vscode.Uri {
    return documentUri.with({ path: `${documentUri.path}.layout.json` });
  }

  private getHtmlForWebview(webview: vscode.Webview): string {
    const wasmScriptUri = webview.asWebviewUri(
      vscode.Uri.joinPath(this.context.extensionUri, 'dist', 'dbml-wasm.bundle.js')
    );
    const canvasScriptUri = webview.asWebviewUri(
      vscode.Uri.joinPath(this.context.extensionUri, 'dist', 'canvas-web.js')
    );

    return `<!DOCTYPE html>
<html lang="pt-BR">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>DB Modeling</title>
  <style>
    html, body, #app { margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; background: #0a0b0c; }
  </style>
</head>
<body>
  <div id="app"></div>
  <script src="${wasmScriptUri}"></script>
  <script src="${canvasScriptUri}"></script>
  <script>
    (function () {
      const transport = new DbModelingCanvasLib.VsCodeTransport();
      DbModelingCanvasLib.mountWorkbench(document.getElementById('app'), transport, window.DbModelingParser);
      transport.ready();
    })();
  </script>
</body>
</html>`;
  }
}
