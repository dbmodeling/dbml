import type * as vscode from 'vscode';
import type { CliHost } from '../cliHost.js';
import { nodeCliHost } from '../cliHost.node.js';
import { mockState, Uri } from './vscode-mock.js';

export interface Harness {
  provider: import('../customEditor.js').DbmlCustomEditorProvider;
  document: FakeDocument;
  /** Mensagens que a extensão enviou para a Webview. */
  posted: any[];
  workspaceState: Map<string, unknown>;
  globalState: Map<string, unknown>;
  /** Simula uma mensagem vinda da Webview e espera o handler terminar. */
  send(message: unknown): Promise<void>;
  disposePanel(): void;
  panel: { visible: boolean };
  /** Chamadas a `DiagnosticCollection.set` feitas pela extensão. */
  diagnosticSets: Array<{ uri: Uri; list: unknown[] }>;
}

export interface FakeDocument {
  uri: Uri;
  text: string;
  isDirty: boolean;
  isUntitled: boolean;
  /** Como `TextDocument.version`: começa em 1 e sobe a cada alteração de texto. */
  version: number;
  getText(): string;
  save(): Promise<boolean>;
}

export function fakeDocument(uri: Uri, text: string): FakeDocument {
  return {
    uri,
    text,
    isDirty: false,
    isUntitled: false,
    version: 1,
    getText() {
      return this.text;
    },
    async save() {
      this.isDirty = false;
      return true;
    },
  };
}

export function fakeContext(shared?: { workspaceState?: Map<string, unknown>; globalState?: Map<string, unknown> }): {
  context: vscode.ExtensionContext;
  workspaceState: Map<string, unknown>;
  globalState: Map<string, unknown>;
} {
  const workspaceState = shared?.workspaceState ?? new Map<string, unknown>();
  const globalState = shared?.globalState ?? new Map<string, unknown>();
  const store = (m: Map<string, unknown>) => ({
    get: (k: string, d?: unknown) => (m.has(k) ? m.get(k) : d),
    update: async (k: string, v: unknown) => void (v === undefined ? m.delete(k) : m.set(k, v)),
    keys: () => [...m.keys()],
  });
  const context = {
    extensionUri: Uri.file('/ext'),
    subscriptions: [] as Array<{ dispose(): void }>,
    workspaceState: store(workspaceState),
    globalState: store(globalState),
  } as unknown as vscode.ExtensionContext;
  return { context, workspaceState, globalState };
}

/** Monta provider + documento + painel falsos e já resolve o editor (equivale a abrir o `.dbml`). */
export async function openEditor(
  Provider: typeof import('../customEditor.js').DbmlCustomEditorProvider,
  opts: { text?: string; uri?: Uri; cli?: CliHost; reuse?: Harness } = {}
): Promise<Harness> {
  // `reuse` simula recarregar a janela: o estado do workspace sobrevive, o resto é novo.
  const { context, workspaceState, globalState } = fakeContext(opts.reuse);
  const document = fakeDocument(opts.uri ?? Uri.file('/ws/schema.dbml'), opts.text ?? 'Table a {\n  id int\n}\n');
  mockState.afterApplyEdit = () => void document.version++;
  const posted: any[] = [];
  let receive: (m: unknown) => void = () => {};
  let onDispose: () => void = () => {};
  const panel = {
    visible: true,
    webview: {
      options: {} as unknown,
      html: '',
      postMessage: async (m: unknown) => {
        posted.push(m);
        return true;
      },
      asWebviewUri: (u: Uri) => u,
      onDidReceiveMessage: (cb: (m: unknown) => void) => {
        receive = cb;
        return { dispose() {} };
      },
    },
    onDidDispose: (cb: () => void) => {
      onDispose = cb;
      return { dispose() {} };
    },
  };
  const provider = new (Provider as any)(context, opts.cli ?? nodeCliHost);
  const diagnosticSets: Array<{ uri: Uri; list: unknown[] }> = [];
  provider.diagnostics = { set: (uri: Uri, list: unknown[]) => diagnosticSets.push({ uri, list }), delete() {} };
  await provider.resolveCustomTextEditor(document, panel, {} as vscode.CancellationToken);
  return {
    provider,
    document,
    posted,
    workspaceState,
    globalState,
    panel,
    diagnosticSets,
    async send(message: unknown) {
      receive(message);
      // deixa a cadeia de promises do handler (inclui a fila de edições) esvaziar
      for (let i = 0; i < 20; i++) await new Promise((r) => setImmediate(r));
    },
    disposePanel: () => onDispose(),
  };
}
