/**
 * Mock mínimo da API `vscode` para testes headless (vitest).
 * Cobre só o que `customEditor.ts` / `extension.ts` usam; o estado fica em `mockState`
 * e é zerado com `resetMock()` entre os testes.
 */

type Listener<T> = (e: T) => unknown;

export class Uri {
  private constructor(
    readonly scheme: string,
    readonly authority: string,
    readonly path: string,
    readonly query: string = '',
    readonly fragment: string = ''
  ) {}

  static file(path: string): Uri {
    return new Uri('file', '', path.replace(/\\/g, '/'));
  }

  static parse(value: string): Uri {
    const m = /^([a-zA-Z][\w+.-]*):(?:\/\/([^/?#]*))?([^?#]*)(?:\?([^#]*))?(?:#(.*))?$/.exec(value);
    if (!m) throw new Error(`Invalid URI: ${value}`);
    return new Uri(m[1], m[2] ?? '', decodeURIComponent(m[3]), m[4] ?? '', m[5] ?? '');
  }

  static joinPath(base: Uri, ...parts: string[]): Uri {
    const segments = base.path.split('/');
    for (const part of parts) {
      for (const seg of part.split('/')) {
        if (seg === '..') segments.pop();
        else if (seg !== '.' && seg !== '') segments.push(seg);
      }
    }
    return base.with({ path: segments.join('/') || '/' });
  }

  get fsPath(): string {
    return this.path;
  }

  with(change: { scheme?: string; authority?: string; path?: string; query?: string; fragment?: string }): Uri {
    return new Uri(
      change.scheme ?? this.scheme,
      change.authority ?? this.authority,
      change.path ?? this.path,
      change.query ?? this.query,
      change.fragment ?? this.fragment
    );
  }

  toString(): string {
    return `${this.scheme}://${this.authority}${this.path}${this.query ? `?${this.query}` : ''}`;
  }
}

export class Position {
  constructor(readonly line: number, readonly character: number) {}
}

export class Range {
  readonly start: Position;
  readonly end: Position;
  constructor(a: Position | number, b: Position | number, c?: number, d?: number) {
    if (typeof a === 'number') {
      this.start = new Position(a, b as number);
      this.end = new Position(c as number, d as number);
    } else {
      this.start = a;
      this.end = b as Position;
    }
  }
}

export class WorkspaceEdit {
  readonly replacements: Array<{ uri: Uri; range: Range; text: string }> = [];
  replace(uri: Uri, range: Range, text: string): void {
    this.replacements.push({ uri, range, text });
  }
}

export enum DiagnosticSeverity {
  Error = 0,
  Warning = 1,
  Information = 2,
  Hint = 3,
}

export class Diagnostic {
  constructor(readonly range: Range, readonly message: string, readonly severity: DiagnosticSeverity = DiagnosticSeverity.Error) {}
}

export enum ConfigurationTarget {
  Global = 1,
  Workspace = 2,
  WorkspaceFolder = 3,
}

export interface MockState {
  files: Map<string, Uint8Array>;
  workspaceFolders: Array<{ uri: Uri; name: string; index: number }> | undefined;
  isTrusted: boolean;
  config: Record<string, { globalValue?: unknown; workspaceValue?: unknown; workspaceFolderValue?: unknown; defaultValue?: unknown }>;
  configUpdates: Array<{ key: string; value: unknown; target: unknown }>;
  findFilesCalls: Array<{ include: unknown; exclude: unknown; max: unknown }>;
  executed: Array<{ command: string; args: unknown[] }>;
  applyEditResult: boolean;
  /** Chamado quando `applyEdit` tem sucesso (o harness usa para subir `document.version`). */
  afterApplyEdit: (() => void) | null;
  textListeners: Array<Listener<{ document: { uri: Uri } }>>;
  messages: { warnings: Array<{ message: string; items: string[] }>; infos: string[]; errors: string[] };
  /** Respostas (em ordem) para `showWarningMessage` com botões; padrão: `undefined` (descartado). */
  warningAnswers: Array<string | undefined>;
  saveDialogResult: Uri | undefined;
  writeError: Error | undefined;
  registeredEditors: Array<{ viewType: string; provider: unknown; options: any }>;
}

function fresh(): MockState {
  return {
    files: new Map(),
    workspaceFolders: undefined,
    isTrusted: true,
    config: {},
    configUpdates: [],
    findFilesCalls: [],
    executed: [],
    applyEditResult: true,
    afterApplyEdit: null,
    textListeners: [],
    messages: { warnings: [], infos: [], errors: [] },
    warningAnswers: [],
    saveDialogResult: undefined,
    writeError: undefined,
    registeredEditors: [],
  };
}

export const mockState: MockState = fresh();

export function resetMock(): void {
  Object.assign(mockState, fresh());
}

function emitter<T>() {
  const listeners: Array<Listener<T>> = [];
  const event = (l: Listener<T>) => {
    listeners.push(l);
    return { dispose: () => listeners.splice(listeners.indexOf(l), 1) };
  };
  return { event, fire: (e: T) => listeners.forEach((l) => l(e)), listeners };
}

export class EventEmitter<T> {
  private readonly inner = emitter<T>();
  readonly event = this.inner.event;
  fire(e: T): void {
    this.inner.fire(e);
  }
}

const noopEvent = () => () => ({ dispose() {} });

export const workspace = {
  get workspaceFolders() {
    return mockState.workspaceFolders;
  },
  get isTrusted() {
    return mockState.isTrusted;
  },
  asRelativePath(uri: Uri | string): string {
    const path = typeof uri === 'string' ? uri : uri.path;
    return path.replace(/^.*\//, '');
  },
  async findFiles(include: unknown, exclude?: unknown, max?: unknown): Promise<Uri[]> {
    mockState.findFilesCalls.push({ include, exclude, max });
    return [...mockState.files.keys()].filter((k) => k.endsWith('.dbml')).map((k) => Uri.parse(k));
  },
  getConfiguration(section?: string) {
    const full = (key: string) => (section ? `${section}.${key}` : key);
    return {
      get: <T>(key: string, fallback?: T): T | undefined => {
        const c = mockState.config[full(key)];
        return (c?.workspaceFolderValue ?? c?.workspaceValue ?? c?.globalValue ?? c?.defaultValue ?? fallback) as T | undefined;
      },
      inspect: (key: string) => mockState.config[full(key)],
      update: async (key: string, value: unknown, target?: unknown) => {
        mockState.configUpdates.push({ key: full(key), value, target });
        const entry = (mockState.config[full(key)] ??= {});
        if (target === ConfigurationTarget.Global) entry.globalValue = value;
      },
    };
  },
  fs: {
    async readFile(uri: Uri): Promise<Uint8Array> {
      const data = mockState.files.get(uri.toString());
      if (!data) throw new Error(`ENOENT: ${uri.toString()}`);
      return data;
    },
    async writeFile(uri: Uri, content: Uint8Array): Promise<void> {
      if (mockState.writeError) throw mockState.writeError;
      mockState.files.set(uri.toString(), content);
    },
    async rename(from: Uri, to: Uri): Promise<void> {
      const data = mockState.files.get(from.toString());
      if (!data) throw new Error('ENOENT');
      mockState.files.delete(from.toString());
      mockState.files.set(to.toString(), data);
    },
    async delete(uri: Uri): Promise<void> {
      mockState.files.delete(uri.toString());
    },
    async stat(uri: Uri): Promise<{ type: number }> {
      if (!mockState.files.has(uri.toString())) throw new Error('ENOENT');
      return { type: 1 };
    },
  },
  async applyEdit(_edit: WorkspaceEdit): Promise<boolean> {
    if (mockState.applyEditResult) mockState.afterApplyEdit?.();
    return mockState.applyEditResult;
  },
  onDidChangeTextDocument(l: Listener<{ document: { uri: Uri } }>) {
    mockState.textListeners.push(l);
    return { dispose: () => mockState.textListeners.splice(mockState.textListeners.indexOf(l), 1) };
  },
  onDidSaveTextDocument: noopEvent(),
  onDidCloseTextDocument: noopEvent(),
  onDidCreateFiles: noopEvent(),
  onDidDeleteFiles: noopEvent(),
  onDidRenameFiles: noopEvent(),
  onDidChangeConfiguration: noopEvent(),
  onDidGrantWorkspaceTrust: noopEvent(),
  createFileSystemWatcher: () => ({ onDidCreate() {}, onDidDelete() {}, onDidChange() {}, dispose() {} }),
};

export const window = {
  registerCustomEditorProvider(viewType: string, provider: unknown, options: unknown) {
    mockState.registeredEditors.push({ viewType, provider, options });
    return { dispose() {} };
  },
  async showSaveDialog(): Promise<Uri | undefined> {
    return mockState.saveDialogResult;
  },
  async showInformationMessage(message: string): Promise<undefined> {
    mockState.messages.infos.push(message);
    return undefined;
  },
  async showWarningMessage(message: string, ...rest: unknown[]): Promise<string | undefined> {
    const items = rest.filter((r): r is string => typeof r === 'string');
    mockState.messages.warnings.push({ message, items });
    return mockState.warningAnswers.shift();
  },
  async showErrorMessage(message: string): Promise<undefined> {
    mockState.messages.errors.push(message);
    return undefined;
  },
};

export const commands = {
  async executeCommand(command: string, ...args: unknown[]): Promise<undefined> {
    mockState.executed.push({ command, args });
    return undefined;
  },
};
