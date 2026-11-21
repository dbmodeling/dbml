import { vi } from 'vitest';
import { mountWorkbench, type Workbench } from '../../src/workbench';
import type {
  ConnectionState,
  DocumentMetadata,
  HostTransport,
  SidecarLayout,
  TextPatch,
  WasmApi,
} from '../../src/types';

/** Wasm mínimo: uma tabela `users` com duas colunas, sem relações. */
export function fakeWasm(): WasmApi {
  const column = (name: string, pk = false) => ({ name, ty: 'int', pk, not_null: pk, unique: false });
  const scene = () => ({
    positions: { users: { x: 0, y: 0 } },
    tables: [{ name: 'users', x: 0, y: 0, w: 220, h: 88, tint: null, fk_columns: [] }],
    groups: [],
    edges: [],
  });
  return {
    parse_and_analyze: () => ({
      schema: { tables: [{ name: 'users', columns: [column('id', true), column('email')] }], refs: [], groups: [] },
      diagnostics: [],
      enums: [],
    }),
    compute_scene: vi.fn(scene),
    compute_layout: () => ({ positions: { users: { x: 0, y: 0 } } }),
    apply_command: () => ({
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
      byte_start: 0,
      byte_end: 0,
      new_text: '',
    }),
    swap_dbml_lines: () => {
      throw new Error('não usado');
    },
    format_dbml: (source) => source,
  } as unknown as WasmApi;
}

export interface FakeTransport extends HostTransport {
  update(source: string, layout?: SidecarLayout, meta?: DocumentMetadata): void;
  setConnection(state: ConnectionState): void;
  patches: TextPatch[];
}

export function fakeTransport(): FakeTransport {
  let onUpdate: ((s: string, l?: SidecarLayout, m?: DocumentMetadata) => void) | undefined;
  let onConnection: ((state: ConnectionState) => void) | undefined;
  const patches: TextPatch[] = [];
  return {
    patches,
    sendPatch: async (patch) => {
      patches.push(patch);
    },
    saveLayout: async () => {},
    onDocumentUpdated: (cb) => {
      onUpdate = cb;
    },
    onConnectionChange: (cb) => {
      onConnection = cb;
    },
    update: (source, layout, meta) => onUpdate?.(source, layout, meta),
    setConnection: (state) => onConnection?.(state),
  };
}

/** jsdom não traz `ResizeObserver`; o workbench só precisa dele para reagir ao tamanho. */
export function installDomStubs(): void {
  class NoopResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', NoopResizeObserver);
  // jsdom não implementa a Web Animations API que os efeitos do workbench consultam
  Element.prototype.getAnimations = () => [];
  Element.prototype.animate = (() => ({ cancel() {}, onfinish: null })) as unknown as Element['animate'];
}

export function mount(transport: HostTransport = fakeTransport(), wasm: WasmApi = fakeWasm()): {
  root: HTMLElement;
  workbench: Workbench;
} {
  const root = document.createElement('div');
  document.body.appendChild(root);
  const workbench = mountWorkbench(root, transport, wasm);
  return { root, workbench };
}

/** Deixa o `requestAnimationFrame` do workbench rodar. */
export const frame = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 40));
