/**
 * Compatibilidade: `DbModelingCanvas` agora monta o Workbench completo
 * (shell + canvas + inspector), idêntico ao app desktop. A implementação antiga
 * em Canvas 2D foi substituída por `workbench.ts`.
 */

import { HostTransport, WasmApi } from './types';
import { Workbench, mountWorkbench } from './workbench';

const NOOP_TRANSPORT: HostTransport = {
  sendPatch: async () => {},
  saveLayout: async () => {},
  onDocumentUpdated: () => {},
};

export class DbModelingCanvas {
  public readonly workbench: Workbench;

  constructor(container: HTMLElement, transport: HostTransport = NOOP_TRANSPORT, wasm?: WasmApi) {
    this.workbench = mountWorkbench(container, transport, wasm);
  }

  /** Desmonta o workbench (listeners, timers, DOM). Quem criou o transporte chama `transport.close()`. */
  public destroy(): void {
    this.workbench.destroy();
  }
}
