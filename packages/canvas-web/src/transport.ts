/**
 * Implementações da camada de transporte para comunicação desacoplada do Canvas
 * com a Webview do VS Code e com o servidor Axum CLI via WebSocket.
 */

import { GIT_BASE_ERROR, TRANSPORT_COMMAND, TRANSPORT_ERROR, TRANSPORT_EVENT } from './constants';
import {
  HostTransport,
  DocumentMetadata,
  TextPatch,
  SidecarLayout,
  Diagnostic,
  WorkspaceFileEntry,
  SessionSnapshot,
  IntrospectRequest,
  IntrospectResult,
  ConnectionState,
  TestConnectionResult,
  GitBaseErrorCode,
  GitBaseResult,
} from './types';

declare function acquireVsCodeApi(): {
  postMessage(message: unknown): void;
  getState(): unknown;
  setState(state: unknown): void;
};

let cachedVsCodeApi: ReturnType<typeof acquireVsCodeApi> | null = null;
function getVsCodeApi(): ReturnType<typeof acquireVsCodeApi> {
  if (cachedVsCodeApi) return cachedVsCodeApi;
  cachedVsCodeApi =
    typeof acquireVsCodeApi === 'function'
      ? acquireVsCodeApi()
      : {
          postMessage: (msg: unknown) => console.log('[Mock VS Code postMessage]', msg),
          getState: () => null,
          setState: () => {},
        };
  return cachedVsCodeApi;
}

/**
 * Última versão do documento conhecida pelo transporte (#51). Patches e salvamentos a citam como
 * `base_version`; o host recusa o que foi calculado sobre outra versão. Só avança (mensagens fora de
 * ordem não a fazem recuar); `reset` vale para a primeira mensagem de uma conexão nova.
 */
class DocumentVersion {
  private value: number | null = null;

  public adopt(version: unknown, reset = false): void {
    if (typeof version !== 'number' || !Number.isFinite(version)) return;
    if (reset || this.value === null || version > this.value) this.value = version;
  }

  /** Versão a enviar como `base_version`; falha se ainda nenhum documento chegou do host. */
  public base(): number {
    if (this.value === null) throw new Error('Not synced with the host yet; wait for the document to load');
    return this.value;
  }
}

const GIT_BASE_CODES: readonly string[] = Object.values(GIT_BASE_ERROR);

/**
 * Converte a resposta `git_base_result` (CLI ou VS Code) em `GitBaseResult`. `source` presente é
 * sucesso; senão vale `error_code` (desconhecido vira `other`) com a mensagem do host.
 */
export function parseGitBaseResult(msg: { source?: unknown; error?: unknown; error_code?: unknown }): GitBaseResult {
  if (typeof msg.source === 'string') return { ok: true, source: msg.source };
  const code = typeof msg.error_code === 'string' && GIT_BASE_CODES.includes(msg.error_code)
    ? (msg.error_code as GitBaseErrorCode)
    : GIT_BASE_ERROR.OTHER;
  return { ok: false, code, message: typeof msg.error === 'string' ? msg.error : '' };
}

type GitBaseWaiter = { resolve: (r: GitBaseResult) => void; reject: (e: Error) => void };

/** Transporte para execução dentro da Webview do VS Code */
export class VsCodeTransport implements HostTransport {
  private readonly version = new DocumentVersion();
  private syncedSource: string | null = null;
  private vscode: ReturnType<typeof acquireVsCodeApi>;
  private readonly abort = new AbortController();
  private updateCallback?: (
    source: string,
    layout?: SidecarLayout,
    meta?: DocumentMetadata
  ) => void;
  private workspaceFilesCallback?: (
    files: WorkspaceFileEntry[],
    activeTitle?: string
  ) => void;
  private externalChangeCallback?: (newSource: string, documentId?: string) => void;
  private errorCallback?: (diagnostics: Diagnostic[]) => void;
  private pendingIntrospect = new Map<
    number,
    { resolve: (r: IntrospectResult) => void; reject: (e: Error) => void }
  >();
  private introspectSeq = 0;
  private requestSeq = 0;
  private pending = new Map<number, { resolve: () => void; reject: (e: Error) => void }>();
  private request(command: string, payload: object = {}): Promise<void> {
    const id = ++this.requestSeq;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.vscode.postMessage({ command, id, ...payload });
    });
  }

  private pendingTest = new Map<
    number,
    { resolve: (r: TestConnectionResult) => void; reject: (e: Error) => void }
  >();
  private pendingGitBase = new Map<number, GitBaseWaiter>();

  constructor() {
    this.vscode = getVsCodeApi();

    window.addEventListener('message', (event) => {
      const message = event.data;
      if (!message) return;
      switch (message.type) {
        case 'result': {
          this.version.adopt(message.version);
          const p = this.pending.get(message.id);
          this.pending.delete(message.id);
          if (message.error) p?.reject(new Error(message.error)); else p?.resolve();
          break;
        }
        case 'update':
          // Também é a resposta do host a um patch recusado (versão velha): ressincroniza o estado.
          this.version.adopt(message.version);
          if (typeof message.source === 'string') this.syncedSource = message.source;
          if (this.updateCallback) {
            this.updateCallback(message.source, message.layout, {
              title: message.title,
              files: message.files,
              session: message.session,
              guideState: message.guideState,
              saved: message.saved,
              savedSource: message.savedSource,
              autosave: message.autosave,
              documentId: message.documentId,
              version: message.version,
            });
          }
          if (message.files && this.workspaceFilesCallback) {
            this.workspaceFilesCallback(message.files, message.title);
          }
          break;
        case TRANSPORT_EVENT.DELTA: {
          if (this.syncedSource === null || !Array.isArray(message.changes)) {
            this.ready();
            break;
          }
          let next = this.syncedSource;
          for (const ch of message.changes as Array<{ rangeOffset: number; rangeLength: number; text: string }>) {
            next = next.slice(0, ch.rangeOffset) + ch.text + next.slice(ch.rangeOffset + ch.rangeLength);
          }
          this.syncedSource = next;
          this.version.adopt(message.version);
          if (this.updateCallback) {
            this.updateCallback(next, undefined, {
              title: message.title,
              saved: message.saved,
              documentId: message.documentId,
              version: message.version,
            });
          }
          break;
        }
        case 'workspace_files':
          if (this.workspaceFilesCallback) {
            this.workspaceFilesCallback(message.files || [], message.title);
          }
          break;
        case 'externalChange':
          if (this.externalChangeCallback) {
            this.externalChangeCallback(message.source);
          }
          break;
        case 'diagnostics':
          if (this.errorCallback) {
            this.errorCallback(message.diagnostics);
          }
          break;
        case TRANSPORT_EVENT.GIT_BASE_RESULT: {
          const p = this.pendingGitBase.get(message.id);
          this.pendingGitBase.delete(message.id);
          p?.resolve(parseGitBaseResult(message));
          break;
        }
        case 'test_connection_result': {
          const p = this.pendingTest.get(message.id);
          this.pendingTest.delete(message.id);
          if (!p) break;
          if (message.error) p.reject(new Error(message.error));
          else p.resolve(message.result as TestConnectionResult);
          break;
        }
        case 'introspect_result': {
          const p = this.pendingIntrospect.get(message.id);
          this.pendingIntrospect.delete(message.id);
          if (!p) break;
          if (message.error) p.reject(new Error(message.error));
          else p.resolve(message.result as IntrospectResult);
          break;
        }
      }
    }, { signal: this.abort.signal });
  }

  /** Remove o listener de mensagens e rejeita o que ainda aguarda resposta do host. */
  public close(): void {
    if (this.abort.signal.aborted) return;
    this.abort.abort();
    const error = new Error('Connection closed');
    for (const p of this.pending.values()) p.reject(error);
    for (const p of this.pendingIntrospect.values()) p.reject(error);
    for (const p of this.pendingTest.values()) p.reject(error);
    for (const p of this.pendingGitBase.values()) p.reject(error);
    this.pending.clear();
    this.pendingIntrospect.clear();
    this.pendingTest.clear();
    this.pendingGitBase.clear();
  }

  /** O host da extensão lê o arquivo no `HEAD` (`git cat-file`); a Webview não executa processos. */
  public requestGitBase(): Promise<GitBaseResult> {
    const id = ++this.requestSeq;
    return new Promise((resolve, reject) => {
      this.pendingGitBase.set(id, { resolve, reject });
      this.vscode.postMessage({ command: TRANSPORT_COMMAND.GIT_BASE, id });
    });
  }

  public checkDatabaseSupport(): Promise<void> {
    return this.request('check_cli');
  }

  public testConnection(req: IntrospectRequest, signal?: AbortSignal): Promise<TestConnectionResult> {
    const id = ++this.introspectSeq;
    return new Promise((resolve, reject) => {
      this.pendingTest.set(id, { resolve, reject });
      signal?.addEventListener('abort', () => {
        if (this.pendingTest.delete(id)) reject(new Error('Introspection cancelled'));
      }, { once: true });
      this.vscode.postMessage({ command: 'test_connection', id, request: req });
    });
  }

  /** O host da extensão executa `dbml import --url` (a Webview não abre sockets). */
  public introspect(req: IntrospectRequest, signal?: AbortSignal): Promise<IntrospectResult> {
    const id = ++this.introspectSeq;
    return new Promise((resolve, reject) => {
      this.pendingIntrospect.set(id, { resolve, reject });
      signal?.addEventListener('abort', () => {
        if (this.pendingIntrospect.delete(id)) reject(new Error('Introspection cancelled'));
      }, { once: true });
      this.vscode.postMessage({ command: 'introspect', id, request: req });
    });
  }

  public async sendPatch(patch: TextPatch): Promise<void> {
    await this.request('apply_patch', { patch, base_version: this.version.base() });
  }

  public async saveDocument(expected_source: string): Promise<void> {
    await this.request('save_document', { expected_source, base_version: this.version.base() });
  }

  public saveLayout(layout: SidecarLayout): Promise<void> {
    return this.request('save_layout', { layout });
  }

  public async exportFile(filename: string, content: string, isBinary: boolean): Promise<void> {
    await this.request('export_file', { filename, content, isBinary });
  }

  public async openFile(path: string): Promise<void> {
    await this.request('open_file', { path });
  }

  /** Avisa o host que a Webview está montada e pronta para receber o documento. */
  public ready(): void {
    this.vscode.postMessage({ command: 'ready' });
  }

  /** Publica os diagnósticos do Wasm na aba nativa Problems do VS Code. */
  public sendDiagnostics(diagnostics: Diagnostic[]): void {
    this.vscode.postMessage({ command: 'diagnostics', diagnostics });
  }

  public async createNewFile(): Promise<void> {
    this.vscode.postMessage({
      command: 'new_file',
    });
  }

  public saveGuideState(state: { onboardingDone: boolean; lastSeenVersion: string | null }): void {
    this.vscode.postMessage({ command: TRANSPORT_COMMAND.SAVE_GUIDE_STATE, state });
  }

  public getSession(): SessionSnapshot | null {
    const s = this.vscode.getState();
    return s && typeof s === 'object' ? (s as SessionSnapshot) : null;
  }

  public saveSession(session: SessionSnapshot): void {
    this.vscode.setState(session);
    this.vscode.postMessage({
      command: 'save_session',
      session,
    });
  }

  public onDocumentUpdated(
    callback: (
      source: string,
      layout?: SidecarLayout,
      meta?: DocumentMetadata
    ) => void
  ): void {
    this.updateCallback = callback;
  }

  public onWorkspaceFilesUpdated(
    callback: (files: WorkspaceFileEntry[], activeTitle?: string) => void
  ): void {
    this.workspaceFilesCallback = callback;
  }

  public onFileChangedExternally(callback: (newSource: string, documentId?: string) => void): void {
    this.externalChangeCallback = callback;
  }

  public onError(callback: (diagnostics: Diagnostic[]) => void): void {
    this.errorCallback = callback;
  }
}

export const RECONNECT_BASE_MS = 1000;
export const RECONNECT_MAX_MS = 30000;

/**
 * Espera antes da próxima tentativa de reconexão: backoff exponencial com
 * jitter ("equal jitter": metade fixa + metade aleatória), limitado a 30 s.
 */
export function reconnectDelay(attempt: number, random: () => number = Math.random): number {
  const ceiling = Math.min(RECONNECT_MAX_MS, RECONNECT_BASE_MS * 2 ** Math.max(0, attempt));
  return Math.round(ceiling / 2 + (random() * ceiling) / 2);
}

/** Transporte para execução dentro do navegador conectado ao servidor Axum CLI */
export class WebSocketTransport implements HostTransport {
  private ws: WebSocket | null = null;
  private url: string;
  private updateCallback?: (
    source: string,
    layout?: SidecarLayout,
    meta?: DocumentMetadata
  ) => void;
  private externalChangeCallback?: (newSource: string, documentId?: string) => void;
  private connectionState: ConnectionState = 'connecting';
  private connectionCallback?: (state: ConnectionState) => void;
  private reconnectTimer: number | null = null;
  private reconnectAttempts = 0;
  private closed = false;
  private readonly abort = new AbortController();
  private readonly version = new DocumentVersion();
  private hasSynced = false;
  /** Edições feitas enquanto o socket estava fora do ar, aguardando reconexão (#75). */
  private offlinePatches: Array<{ patch: TextPatch; resolve: () => void; reject: (e: Error) => void }> = [];
  private pendingLayout: { layout: SidecarLayout; expected_file?: string } | null = null;
  /** Aba oculta: nenhuma tentativa agendada até `visibilitychange`. */
  private waitingVisible = false;
  private requestSeq = 0;
  private pendingGitBase = new Map<number, GitBaseWaiter>();
  private pending = new Map<number, { resolve: () => void; reject: (e: Error) => void }>();
  private request(type: string, payload: unknown): Promise<void> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return Promise.reject(new Error('Disconnected; reconnect and save again'));
    const id = ++this.requestSeq;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      try { this.ws!.send(JSON.stringify({ type, payload, id })); }
      catch (error) { this.pending.delete(id); reject(error); }
    });
  }


  constructor(url = `ws://${window.location.host}/ws`) {
    this.url = url;
    document.addEventListener('visibilitychange', () => this.onVisibility(), { signal: this.abort.signal });
    window.addEventListener(
      'beforeunload',
      (e) => {
        if (this.offlinePatches.length > 0 || this.pendingLayout !== null) {
          e.preventDefault();
          e.returnValue = '';
        }
      },
      { signal: this.abort.signal }
    );
    this.connect();
  }

  /** Encerra o socket e impede novas reconexões (idempotente). */
  public close(): void {
    if (this.closed) return;
    this.closed = true;
    this.abort.abort();
    if (this.reconnectTimer !== null) window.clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    const ws = this.ws;
    this.ws = null;
    if (ws) {
      ws.onopen = ws.onmessage = ws.onclose = null;
      ws.close();
    }
    for (const p of this.pending.values()) p.reject(new Error(TRANSPORT_ERROR.CONNECTION_CLOSED));
    this.pending.clear();
    for (const p of this.pendingGitBase.values()) p.reject(new Error(TRANSPORT_ERROR.CONNECTION_CLOSED));
    this.pendingGitBase.clear();
    for (const q of this.offlinePatches.splice(0)) q.reject(new Error(TRANSPORT_ERROR.CONNECTION_CLOSED));
    this.pendingLayout = null;
    this.connectionCallback = undefined;
  }

  private onVisibility(): void {
    if (this.closed || document.hidden || !this.waitingVisible) return;
    this.waitingVisible = false;
    this.reconnectAttempts = 0;
    this.connect();
  }

  private scheduleReconnect(): void {
    if (this.closed || this.reconnectTimer !== null) return;
    if (document.hidden) {
      this.waitingVisible = true;
      return;
    }
    const delay = reconnectDelay(this.reconnectAttempts++);
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, delay);
  }

  private setConnectionState(state: ConnectionState): void {
    if (state === this.connectionState) return;
    this.connectionState = state;
    this.connectionCallback?.(state);
  }

  public onConnectionChange(callback: (state: ConnectionState) => void): void {
    this.connectionCallback = callback;
    callback(this.connectionState);
  }

  private connect(): void {
    this.setConnectionState('connecting');
    try {
      this.ws = new WebSocket(this.url);

      this.ws.onopen = () => {
        this.reconnectAttempts = 0;
        this.setConnectionState('online');
      };

      this.ws.onmessage = (event) => {
        try {
          const msg = JSON.parse(event.data);
          if (msg.type === 'result') {
            this.version.adopt(msg.version);
            const p = this.pending.get(msg.id);
            this.pending.delete(msg.id);
            if (msg.error) p?.reject(new Error(msg.error)); else p?.resolve();
          } else if (msg.type === TRANSPORT_EVENT.GIT_BASE_RESULT) {
            const p = this.pendingGitBase.get(msg.id);
            this.pendingGitBase.delete(msg.id);
            p?.resolve(parseGitBaseResult(msg));
          } else if (msg.type === 'init' || msg.type === 'update' || msg.type === 'patch_rejected') {
            // `init` abre uma conexão nova (o servidor pode ter reiniciado): a versão é a dele.
            // `patch_rejected` traz o estado atual do documento para o cliente ressincronizar; o erro
            // do pedido chega em seguida no `result` (e é ele que avisa o usuário).
            this.version.adopt(msg.version, msg.type === 'init');
            if (msg.type === TRANSPORT_EVENT.INIT) {
              this.hasSynced = true;
              if (this.offlinePatches.length > 0) {
                const queued = this.offlinePatches.splice(0);
                if (queued[0].patch.expected_source === msg.source) {
                  void this.flushOfflinePatches(queued);
                  this.flushPendingLayout();
                  return;
                }
                for (const q of queued) {
                  q.reject(new Error(TRANSPORT_ERROR.OFFLINE_CONFLICT));
                }
              }
              this.flushPendingLayout();
            }
            if (this.updateCallback) {
              this.updateCallback(msg.source, msg.layout, {
                title: msg.title,
                files: msg.files,
                session: msg.session,
                saved: msg.saved,
                documentId: msg.documentId,
                version: msg.version,
              });
            }
          } else if (msg.type === 'external_change') {
            this.version.adopt(msg.version);
            if (this.externalChangeCallback) {
              this.externalChangeCallback(msg.source, msg.documentId);
            }
          }
        } catch (e) {
          console.error('Erro ao processar mensagem WebSocket:', e);
        }
      };

      this.ws.onclose = () => {
        for (const p of this.pending.values()) p.reject(new Error('Connection lost; reconnect and save again'));
        this.pending.clear();
        for (const p of this.pendingGitBase.values()) p.reject(new Error('Connection lost; reconnect and compare again'));
        this.pendingGitBase.clear();
        this.setConnectionState('offline');
        this.scheduleReconnect();
      };
    } catch (e) {
      console.warn('Falha ao conectar WebSocket:', e);
      this.setConnectionState('offline');
      this.scheduleReconnect();
    }
  }

  private async flushOfflinePatches(
    queued: Array<{ patch: TextPatch; resolve: () => void; reject: (e: Error) => void }>
  ): Promise<void> {
    for (let i = 0; i < queued.length; i++) {
      const item = queued[i];
      try {
        const { byte_start, byte_end, new_text, expected_source, expected_file } = item.patch;
        const base_version = this.version.base();
        await this.request(TRANSPORT_COMMAND.APPLY_PATCH, { base_version, byte_start, byte_end, new_text, expected_source, expected_file });
        item.resolve();
      } catch (err) {
        const e = err instanceof Error ? err : new Error(String(err));
        item.reject(e);
        for (let j = i + 1; j < queued.length; j++) queued[j].reject(e);
        return;
      }
    }
  }

  private flushPendingLayout(): void {
    const pending = this.pendingLayout;
    if (!pending || !this.ws || this.ws.readyState !== WebSocket.OPEN) return;
    this.pendingLayout = null;
    void this.request(TRANSPORT_COMMAND.SAVE_LAYOUT, pending).catch(() => {});
  }

  public async sendPatch(patch: TextPatch): Promise<void> {
    const { byte_start, byte_end, new_text, expected_source, expected_file } = patch;
    const base_version = this.version.base();
    if (this.hasSynced && (!this.ws || this.ws.readyState !== WebSocket.OPEN)) {
      return new Promise((resolve, reject) => {
        this.offlinePatches.push({ patch, resolve, reject });
      });
    }
    await this.request('apply_patch', { base_version, byte_start, byte_end, new_text, expected_source, expected_file });
  }

  public async saveDocument(expected_source: string, expected_file?: string): Promise<void> {
    const base_version = this.version.base();
    await this.request('save_document', { base_version, expected_source, expected_file });
  }

  /** Pede ao servidor local (`dbml run`) o arquivo ativo no `HEAD` do git; somente leitura. */
  public requestGitBase(documentId?: string): Promise<GitBaseResult> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return Promise.reject(new Error('Disconnected; reconnect and compare again'));
    const id = ++this.requestSeq;
    return new Promise((resolve, reject) => {
      this.pendingGitBase.set(id, { resolve, reject });
      try { this.ws!.send(JSON.stringify({ type: TRANSPORT_COMMAND.GIT_BASE, payload: { expected_file: documentId ?? '' }, id })); }
      catch (error) { this.pendingGitBase.delete(id); reject(error); }
    });
  }

  public async testConnection(req: IntrospectRequest, signal?: AbortSignal): Promise<TestConnectionResult> {
    const res = await fetch('/api/introspect/test', {
      signal,
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(body.error || `Request failed (${res.status})`);
    return body as TestConnectionResult;
  }

  /** POST no servidor local (`dbml run`), que conecta ao banco; a senha só vai no corpo. */
  public async introspect(req: IntrospectRequest, signal?: AbortSignal): Promise<IntrospectResult> {
    const res = await fetch('/api/introspect', {
      signal,
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(body.error || `Request failed (${res.status})`);
    return body as IntrospectResult;
  }

  public saveLayout(layout: SidecarLayout, expected_file?: string): Promise<void> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      this.pendingLayout = { layout, expected_file };
      return Promise.resolve();
    }
    return this.request('save_layout', { layout, expected_file });
  }

  public async exportFile(filename: string, content: string, isBinary: boolean): Promise<void> {
    let blob: Blob;
    if (isBinary) {
      const binaryString = atob(content);
      const bytes = new Uint8Array(binaryString.length);
      for (let i = 0; i < binaryString.length; i++) {
        bytes[i] = binaryString.charCodeAt(i);
      }
      const mime =
        filename.endsWith('.jpg') || filename.endsWith('.jpeg') ? 'image/jpeg' : 'image/png';
      blob = new Blob([bytes], { type: mime });
    } else {
      const mime = filename.endsWith('.svg')
        ? 'image/svg+xml;charset=utf-8'
        : 'text/plain;charset=utf-8';
      blob = new Blob([content], { type: mime });
    }
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  public openFile(path: string): Promise<void> {
    return this.request('open_file', path);
  }

  public onDocumentUpdated(
    callback: (
      source: string,
      layout?: SidecarLayout,
      meta?: DocumentMetadata
    ) => void
  ): void {
    this.updateCallback = callback;
  }

  public onFileChangedExternally(callback: (newSource: string, documentId?: string) => void): void {
    this.externalChangeCallback = callback;
  }
}
