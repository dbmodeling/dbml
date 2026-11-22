/**
 * Workbench completo do DB Modeling na Web: reproduz o shell (`crates/app/src/shell/mod.rs` + `panels.rs`)
 * e o canvas (`crates/app/src/canvas.rs`) do app desktop. Toda a geometria (layout, rotas,
 * grupos) vem do núcleo Rust/Wasm (`compute_scene`); aqui só há DOM, eventos e estado de UI.
 */

import {
  CONNECT_NETWORK_ROW_IDS,
  DIFF_ACT,
  DIFF_EDGE_WIDTH,
  DIFF_PANE_ID,
  DIFF_STATUS_COLOR_VAR,
  GUIDE_STORAGE_KEY,
  LOCAL_SESSION_PREFIX,
  MAX_LOCAL_SESSIONS,
  MAX_WASM_RECOVERY_ATTEMPTS,
  TLS_FIELD_LABEL,
  TLS_ROW_ID,
  TLS_SELECT_ID,
  TLS_SELECT_OPTIONS,
} from './constants';
import {
  Command,
  Diagnostic,
  HostTransport,
  DocumentMetadata,
  Schema,
  SceneGeometry,
  SidecarLayout,
  ConnectionState,
  IntrospectRequest,
  WasmApi,
  WasmSession,
  EdgeStyle,
  edgeKey,
  TextPatch,
  WorkspaceFileEntry,
  SessionSnapshot,
  qualifiedEndpointTable,
  qualifiedTableName,
} from './types';
import {
  WHITE, WHITE_30, WHITE_20, WHITE_75, BLACK_20, BLACK_30,
  CARD_W,
  HEADER_H,
  ROW_H,
  MIN_ZOOM,
  MAX_ZOOM,
  TABLE_COLORS,
  STANDARD_TYPES,
  cardH,
  getGlass,
  UI_BASE,
  hexWithAlpha,
  Mode,
} from './glass';
import { iconSvg, IconName } from './icons';
import { ActionId, COLUMN_PRESET_ACTIONS, GROUP_EXPORT_CODE, GROUP_EXPORT_IMAGE, catalog } from './actions';
import { MINIMAP_H, MINIMAP_PAD, MINIMAP_W, MinimapTransform, computeMinimapTransform, panForMapPoint, worldToMap } from './minimap';
import { Language, connectionHint, diagnosticMessage, setLanguage, systemLanguage, t, t as translate } from './i18n';
import { WORKBENCH_CSS } from './workbench-css';
import { tokenizeLine, tableLineRange } from './dbml-highlight';
import { curvedEdge, edgeDrawing } from './edge-path';
import { MOTION, actSelector, cssQ, easeOut, lerp, noMotion } from './motion';
import { exportToMermaid, exportToSvg, exportSvgStringToRasterBlob } from './export';
import { canvasGridStep, canvasGroups } from './scene-style';
import { ConnectDbModel, DB_ENGINES, DbEngine, INTROSPECT_TIMEOUT_MS, IntrospectGuard, buildRequest } from './connect-db';
import { IMPORT_DIALECTS, ImportDialect, ImportSqlModel, ImportSqlResult } from './import-sql';
import { APP_VERSION, GuideState, PRODUCT_GUIDE, TourTarget, decideStartupGuide, pickText } from './guide';
import { TourController } from './tour';
import { DIFF_OFF, diffErrorMessageKey, loadDiff, refreshDiff, type DiffState } from './diff/state';
import { cameraFor, focusRect, type FocusContext } from './diff/focus';
import { occupiedRects, placeGhosts, removedEdges, diffCardHeight, type Rect } from './diff/layout';
import type { DiffView } from './diff/model';
import {
  columnDiffTitle,
  diffMark,
  diffStatusClass,
  diffStatusLabel,
  ghostSlots,
  renderDiffPanel,
  renderEnumGhostValues,
  renderGhostEdges,
  renderGhostEnum,
  renderGhostRows,
  renderGhostTable,
} from './diff/render';

type Pt = { x: number; y: number };
type Pane = 'files' | 'schema' | 'problems' | typeof DIFF_PANE_ID;
type Drag =
  | { kind: 'edge'; last: Pt; snapshot: ReturnType<Workbench['captureSnapshot']>; moved: boolean }
  | { kind: 'pan'; lx: number; ly: number }
  | { kind: 'cards'; last: Pt }
  | { kind: 'enum'; name: string; last: Pt }
  | { kind: 'group'; name: string; last: Pt }
  | { kind: 'marquee'; start: Pt; now: Pt; base: string[] }
  | { kind: 'link'; table: string; col: string; now: Pt };

const esc = (s: string): string =>
  String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
/** Trap do Wasm (`panic = "abort"` vira `RuntimeError: unreachable`): a instância não é mais confiável. */
export function isFatalWasmError(e: unknown): boolean {
  if (typeof WebAssembly !== 'undefined' && e instanceof WebAssembly.RuntimeError) return true;
  const message = e instanceof Error ? e.message : '';
  return /^(unreachable|memory access out of bounds|recursive use of an object)/i.test(message);
}
const errorMessage = (e: unknown): string => (e instanceof Error ? e.message : String(e));
const ICON = (n: IconName, s = 16, c = 'currentColor'): string => iconSvg(n, s, c);
const IDENT = /^[\p{L}\p{N}_]+$/u;
const UI_STEPS = [1, 1.15, 1.3];

export function diagnosticLabels(diags: Diagnostic[]): { status: string; footer: string } {
  const errors = diags.filter((d) => d.severity === 'error').length;
  const warnings = diags.filter((d) => d.severity === 'warning').length;
  const total = errors + warnings;
  return {
    status: `${total} ${t(total === 1 ? 'problema' : 'problemas')}`,
    footer: errors ? `${errors} ${t(errors === 1 ? 'problema' : 'problemas')}` : warnings ? `${warnings} ${t(warnings === 1 ? 'aviso' : 'avisos')}` : t('Modelo válido'),
  };
}

const ACTION_ICON: Partial<Record<ActionId, IconName>> = {
  NewFile: 'FilePlus', SaveFile: 'Save', ExportDbml: 'FileDown', ImportSqlDdl: 'FileInput', ConnectDatabase: 'Database',
  Undo: 'Undo2', Redo: 'Redo2', FormatDocument: 'WandSparkles', ShowFiles: 'Folder', ShowSchema: 'Table2',
  ShowProblems: 'TriangleAlert', ToggleTheme: 'SunMoon', ToggleEditor: 'Braces', ToggleSidebar: 'PanelLeft',
  ToggleInspector: 'PanelRight', ToggleLegend: 'Info', ToggleMinimap: 'Frame', ToolSelect: 'MousePointer2', ToolHand: 'Hand', FitView: 'Scan',
  Relayout: 'LayoutGrid', ZoomIn: 'ZoomIn', ZoomOut: 'ZoomOut', Deselect: 'SquareDashed', NewTable: 'SquarePlus',
  NewEnum: 'List', DeleteSelection: 'Trash', RenameSelected: 'Pencil', GroupSelection: 'Group', UngroupSelection: 'Group',
  ExportSqlPostgres: 'Database', ExportSqlMysql: 'Database', ExportSqlSqlite: 'Database', ExportSqlSqlServer: 'Database',
  ExportSqlOracle: 'Database', ExportMermaid: 'GitFork', ExportDocsHtml: 'FileCode', ExportDocsMarkdown: 'FileCode',
  ExportPng: 'Image', ExportJpeg: 'Image', ExportSvg: 'PenTool',
  OpenPalette: 'Command', QuickSearch: 'Search', ToggleFocusMode: 'Scan', ToggleDiff: 'GitCompare', CycleFocusDepth: 'Layers',
  ShowShortcuts: 'Keyboard', ShowTour: 'Info', ShowWhatsNew: 'WandSparkles', AddPresetUuidPk: 'Fingerprint', AddPresetBigIntPk: 'KeyRound',
  AddPresetTimestamps: 'Clock', AddPresetSoftDelete: 'Archive',
};

/** Submenus do menu principal (hambúrguer); a paleta continua listando todo o catálogo. */
const MAIN_MENU: ReadonlyArray<readonly [string, IconName, ActionId[]]> = [
  ['Arquivo', 'File', ['NewFile', 'SaveFile', 'ExportDbml', 'ImportSqlDdl', 'ConnectDatabase']],
  ['Editar', 'Pencil', ['Undo', 'Redo', 'FormatDocument']],
  ['Exibir', 'PanelsTopLeft', ['ShowFiles', 'ShowSchema', 'ShowProblems', 'ToggleDiff', 'ToggleTheme', 'ToggleEditor', 'ToggleSidebar', 'ToggleInspector', 'ToggleLegend']],
  ['Canvas', 'Frame', ['ToolSelect', 'ToolHand', 'FitView', 'Relayout', 'ZoomIn', 'ZoomOut', 'Deselect']],
  ['Tabelas', 'Table2', ['NewTable', 'NewEnum', 'DeleteSelection', 'RenameSelected', 'GroupSelection', 'UngroupSelection']],
];

const EXPORT_CODE_ROWS: ReadonlyArray<readonly [ActionId, string, string]> = [
  ['ExportSqlPostgres', 'PostgreSQL', '.sql'], ['ExportSqlMysql', 'MySQL', '.sql'], ['ExportSqlSqlite', 'SQLite', '.sql'],
  ['ExportSqlSqlServer', 'SQL Server', '.sql'], ['ExportSqlOracle', 'Oracle', '.sql'], ['ExportMermaid', 'Mermaid erDiagram', '.mmd'],
  ['ExportDocsHtml', 'Documentation (HTML)', '.html'], ['ExportDocsMarkdown', 'Documentation (Markdown)', '.md'],
];
const EXPORT_IMAGE_ROWS: ReadonlyArray<readonly [ActionId, string, string, string]> = [
  ['ExportPng', 'PNG', 'Raster · resolução 2×', '.png'], ['ExportJpeg', 'JPEG', 'Raster · fundo sólido', '.jpg'],
  ['ExportSvg', 'SVG vetorial', 'Escalável, editável', '.svg'],
];

interface Prefs {
  mode: Mode;
  lang: Language;
  uiScale: number;
  translucent: boolean;
  autoHide: boolean;
  inspector: boolean;
  sidebar: boolean;
  legend: boolean;
  minimap: boolean;
}

function loadPrefs(): Prefs {
  const d: Prefs = {
    mode: 'dark',
    lang: systemLanguage(typeof navigator === 'undefined' ? 'en' : navigator.language),
    uiScale: 1,
    translucent: false,
    autoHide: true,
    inspector: true,
    sidebar: true,
    legend: false,
    minimap: true,
  };
  try {
    const { glass: _legacyGlass, glassLevel: _legacyLevel, translucent_cards: _legacyCards, ...saved } = JSON.parse(localStorage.getItem('dbmodeling.prefs') || '{}');
    // transparência desativada (decisão de produto, 2026-11-16): ignora preferência antiga
    return { ...d, ...saved, translucent: false };
  } catch {
    return d;
  }
}

/** Máscara de regiões sujas para evitar reconstruir todo o DOM a cada quadro (#108). */
export const Dirty = {
  Camera: 1 << 0,
  World: 1 << 1,
  Titlebar: 1 << 2,
  Rails: 1 << 3,
  Sidebar: 1 << 4,
  Editor: 1 << 5,
  Inspector: 1 << 6,
  Islands: 1 << 7,
  Status: 1 << 8,
  Legend: 1 << 9,
  Theme: 1 << 10,
  Chrome:
    (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5) | (1 << 6) | (1 << 7) | (1 << 8) | (1 << 9) | (1 << 10),
  All: (1 << 11) - 1,
} as const;

export class Workbench {
  private root: HTMLElement;
  private wb!: HTMLElement;
  private transport: HostTransport;
  private wasm: WasmApi | undefined;
  /** Documento analisado por versão de texto (#107/#110); ausente em hosts sem `Session`. */
  private session: WasmSession | null = null;
  /** Posições mudaram no arrasto: as arestas são reroteadas uma vez por quadro, não por mousemove. */
  private edgesDirty = false;

  private source = '';
  private savedSource = '';
  private canDiscard = true;
  private hostSource = '';
  private documentId?: string;
  private patchQueue: Promise<void> = Promise.resolve();
  private patchError: Error | null = null;
  private autosave = true;
  private connection: ConnectionState = 'online';
  private autosaveTimer?: number;
  private title = 'example.dbml';
  private files: WorkspaceFileEntry[] = [];
  private schema: Schema = { tables: [], refs: [], groups: [] };
  private lastValid: Schema | null = null;
  private diags: Diagnostic[] = [];
  private enums: string[] = [];
  private scene: SceneGeometry | null = null;
  private positions: Record<string, Pt> = {};
  private extraLayout: Partial<SidecarLayout> = {};
  private undoStack: Array<{
    source: string;
    positions: Record<string, Pt>;
    selection: string[];
    selectedEnum: string | null;
    selectedRef: Workbench['selectedRef'];
    edges: Record<string, EdgeStyle>;
  }> = [];
  private redoStack: Array<{
    source: string;
    positions: Record<string, Pt>;
    selection: string[];
    selectedEnum: string | null;
    selectedRef: Workbench['selectedRef'];
    edges: Record<string, EdgeStyle>;
  }> = [];
  private edges: Record<string, EdgeStyle> = {};
  private dragStartPositions: Record<string, Pt> | null = null;

  private selection: string[] = [];
  private selectedEnum: string | null = null;
  private selectedRef: { from_table: string; from_col: string; to_table: string; to_col: string } | null = null;
  private top: string | null = null;
  private tool: 'select' | 'hand' = 'select';
  private space = false;
  private zoom = 1;
  private pan: Pt = { x: 60, y: 60 };
  private drag: Drag | null = null;
  private needsFit = true;
  private viewport: Pt | null = null;

  private prefs: Prefs = loadPrefs();
  private pane: Pane = 'schema';
  private editorOpen = false;
  private paletteOpen = false;
  private quickSearchOpen = false;
  private shortcutsOpen = false;
  private whatsNewOpen = false;
  private tour: TourController | null = null;
  private guideChecked = false;
  private guideState: GuideState = { onboardingDone: false, lastSeenVersion: null };
  private readonly autoGuide: boolean;
  private paletteQuery = '';
  private paletteIdx = 0;
  private quickSearchQuery = '';
  private quickSearchIdx = 0;
  private focusMode = false;
  private focusDepth: 1 | 2 = 1;
  /** Modo Comparar (somente leitura): base = git HEAD do arquivo aberto; nunca toca o buffer nem o histórico. */
  private diff: DiffState = DIFF_OFF;
  /** Texto do documento contra o qual `diff` foi calculado. */
  private diffHead = '';
  /** Descarta respostas atrasadas de um pedido de base já substituído ou cancelado. */
  private diffSeq = 0;
  private diffPrevPane: Exclude<Pane, typeof DIFF_PANE_ID> = 'schema';
  private diffGhostPositions: Record<string, Pt> = {};
  private highlightedColumn: [string, string] | null = null;
  private minimapXform: MinimapTransform | null = null;
  private minimapVpRect: { x: number; y: number; w: number; h: number } | null = null;
  private minimapDrag: { grabX: number; grabY: number } | null = null;
  private connectEngine: DbEngine = 'postgres';
  private importDialect: ImportDialect = 'postgres';
  private focusCol: [string, string] | null = null;
  private openCol: string | null = null;
  private openColTable: string | null = null;
  private schemaFilterOpen = false;
  private schemaFilter = '';
  private pendingNew: string | null = null;
  private sessionTimer: number | null = null;
  private collapsed = new Set<string>();
  /** Efeitos em curso sobre elementos que o render recria: reaplicados com atraso negativo (retomam do ponto). */
  private fx: Array<{ sel: string; cls: string; t0: number; dur: number }> = [];
  /** Cartões excluídos que ainda estão sumindo. */
  private ghosts: Array<{ node: HTMLElement; t0: number }> = [];
  private shown = new WeakMap<HTMLElement, boolean>();
  private tweenRaf: number | null = null;
  private toastTimer = 0;
  /** Estado do último render, para animar só o que mudou. */
  private seen: { title: string; tables: Set<string>; edges: Set<string>; sel: Set<string>; insp: string; pane: string } | null = null;
  private edKey = '';
  private menuSub = 'Arquivo';
  private saveTimer: number | null = null;
  private raf: number | null = null;
  /** Cancela todos os listeners globais (window) no `destroy()`. */
  private readonly abort = new AbortController();
  private destroyed = false;
  /** Regiões pendentes para o próximo quadro (`Dirty.*`). */
  private dirtyMask = 0;
  private announcedSelection = '';
  /** O Wasm sofreu um trap: aguarda reinstanciação ou recarga da página. */
  private engineFailed = false;
  private wasmRecoveryCount = 0;
  private wasmRecovering = false;
  private styleEl: HTMLStyleElement | null = null;
  private resizeObserver: ResizeObserver | null = null;

  private els: Record<string, HTMLElement> = {};

  constructor(root: HTMLElement, transport: HostTransport, wasm?: WasmApi, options?: { autoGuide?: boolean }) {
    setLanguage(this.prefs.lang);
    this.root = root;
    this.transport = transport;
    this.wasm = wasm || (window as unknown as { DbModelingParser?: WasmApi }).DbModelingParser;
    const isJsdom = typeof navigator !== 'undefined' && /jsdom/i.test(navigator.userAgent);
    this.autoGuide = options?.autoGuide ?? !isJsdom;
    this.guideState = this.loadLocalGuideState();
    this.build();
    this.bind();
    transport.onDocumentUpdated((src, layout, meta) => { if (!this.destroyed) this.setDocument(src, layout, meta); });
    // edição fora do app (outro editor, git checkout): a CLI avisa via `external_change`
    transport.onFileChangedExternally?.((src, documentId) => {
      if (this.destroyed) return;
      if (documentId && documentId !== this.documentId) return;
      this.setDocument(src, undefined, { saved: true, documentId });
    });
    transport.onConnectionChange?.((state) => {
      if (this.destroyed) return;
      this.connection = state;
      this.renderStatus();
    });
    transport.onWorkspaceFilesUpdated?.((files, title) => {
      if (this.destroyed) return;
      this.files = files;
      if (title) this.title = title;
      this.renderChrome();
    });
  }

  // ───────────────────────── construção do DOM ─────────────────────────

  private build(): void {
    const style = document.createElement('style');
    style.textContent = WORKBENCH_CSS;
    document.head.appendChild(style);
    this.styleEl = style;
    this.root.innerHTML = '';
    this.wb = document.createElement('div');
    this.wb.className = 'wb';
    this.wb.innerHTML = `
      <div class="titlebar ui" id="titlebar" role="toolbar" aria-label="${t('Barra de título')}"></div>
      <div class="main">
        <div class="rail left ui" id="rail-left" role="toolbar" aria-orientation="vertical" aria-label="${t('Painéis')}"></div>
        <div class="sidebar ui" id="sidebar" data-tour="sidebar" role="region" aria-label="${t('Painel lateral')}"></div>
        <div class="editor ui" id="editor" role="region" aria-label="${t('Editor DBML')}">
          <div class="eh" id="ed-head"></div>
          <div class="ebody"><div class="ebd" id="ebd"><div class="ebd-in" id="ebd-in"></div></div><textarea class="code" id="code" aria-label="DBML" spellcheck="false" wrap="off"></textarea></div>
        </div>
        <button type="button" class="panel-resize" data-resize="left"></button>
        <div class="surface" id="surface" data-tour="canvas" tabindex="0" role="region" aria-label="${t('Diagrama')}">
          <div class="world" id="world"></div>
          <div class="marquee" id="marquee"></div>
          <div class="empty" id="empty">
            <div class="in">
              <div class="etext">
                <h3><span data-i18n="Nenhuma tabela ainda">${t('Nenhuma tabela ainda')}</span></h3>
                <p><span data-i18n="Escreva seu esquema DBML no editor ou crie uma nova tabela.">${t('Escreva seu esquema DBML no editor ou crie uma nova tabela.')}</span></p>
              </div>
              <div class="acts">
                <button class="eact" data-act="ToggleEditor"><span style="color:var(--accent);display:flex">${ICON('Braces', 16)}</span><span data-i18n="Abrir editor DBML">${t('Abrir editor DBML')}</span><span class="k">Ctrl E</span></button>
                <button class="eact" data-act="NewTable"><span style="color:var(--accent);display:flex">${ICON('SquarePlus', 16)}</span><span data-i18n="Nova tabela">${t('Nova tabela')}</span><span class="k">T</span></button>
                <button class="eact" data-act="ImportSqlDdl"><span style="color:var(--accent);display:flex">${ICON('FileInput', 16)}</span><span data-i18n="Importar SQL DDL">${t('Importar SQL DDL')}</span></button>
                <button class="eact" data-act="ConnectDatabase"><span style="color:var(--accent);display:flex">${ICON('Database', 16)}</span><span data-i18n="Conectar ao banco">${t('Conectar ao banco')}</span></button>
              </div>
            </div>
          </div>
          <div class="island tools ui" id="tools" role="toolbar" aria-label="${t('Ferramentas')}"></div>
          <div class="island zoomi ui" id="zoomi"></div>
          <div class="minimap ui" id="minimap" data-tour="minimap" role="region" aria-label="${t('Minimapa')}"></div>
          <div class="legend ui" id="legend"></div>
        </div>
        <button type="button" class="panel-resize" data-resize="right"></button>
        <div class="inspector ui" id="inspector" data-tour="inspector" role="region" aria-label="${t('Inspetor')}"></div>
      </div>
      <div class="status ui" id="status"></div>
      <div class="toast" id="toast" role="status"></div>
      <div class="sr-only" id="live" role="status" aria-live="polite" aria-atomic="true"></div>`;
    this.root.appendChild(this.wb);
    for (const id of [
      'titlebar', 'rail-left', 'sidebar', 'editor', 'surface', 'world', 'marquee', 'empty', 'tools', 'zoomi',
      'minimap', 'legend', 'inspector', 'status', 'toast', 'live', 'code', 'ed-head', 'ebd', 'ebd-in',
    ]) {
      this.els[id] = this.wb.querySelector('#' + id) as HTMLElement;
    }
    this.applyTheme();
  }

  private applyTheme(): void {
    const g = getGlass(this.prefs.mode, this.prefs.uiScale * UI_BASE, this.prefs.translucent);
    const v: Record<string, string> = {
      white: WHITE, 'white-30': WHITE_30, 'white-20': WHITE_20, 'white-75': WHITE_75,
      'black-20': BLACK_20, 'black-30': BLACK_30,
      wall: g.wall, blur: g.blur, chrome: g.chrome, sidebar: g.sidebar, surface: g.surface, grid: g.grid, card: g.card,
      'card-translucent': g.cardTranslucent, island: g.island, edge: g.edge, 'edge-soft': g.edgeSoft,
      hover: g.hover, active: g.active, selected: g.selected, text: g.text, muted: g.textMuted,
      faint: g.textFaint, accent: g.accent, danger: g.danger, warning: g.warning, ok: g.ok, err: g.danger,
      'card-shadow': g.cardShadow, 'card-shadow-sel': g.cardShadowSelected, 'island-shadow': g.islandShadow,
      rail: g.rail, editor: g.editor, gutter: g.gutter, lnum: g.lnum, sel: g.sel, 'sel-shadow': g.selShadow,
      input: g.input, 'input-solid': g.inputSolid, 'edge-strong': g.edgeStrong, pop: g.pop, modal: g.modal,
      scrim: g.scrim, 'pop-shadow': g.popShadow, 'syn-kw': g.synKw, 'syn-type': g.synType, 'syn-attr': g.synAttr,
      'syn-num': g.synNum, 'syn-comment': g.synComment, 'syn-punct': g.synPunct,
      'rel-line': g.relLine, 'rel-stroke': g.relStroke, 'rel-label': g.relLabel,
      link: g.link, idx: g.idx, 'swatch-ring': g.swatchRing,
      ui: String(g.uiScale),
    };
    for (const k of Object.keys(v)) this.wb.style.setProperty('--' + k, v[k]);
    this.wb.classList.toggle('glass', this.prefs.translucent);
    this.wb.classList.toggle('translucent', this.prefs.translucent);
  }

  private savePrefs(): void {
    try {
      localStorage.setItem('dbmodeling.prefs', JSON.stringify(this.prefs));
    } catch {
      /* ignorado */
    }
  }

  private sessionKey(): string {
    return `dbmodeling.session:${this.title || 'default'}`;
  }

  private loadDocSession(): SessionSnapshot | null {
    const hostSession = this.transport.getSession?.();
    if (hostSession) return hostSession;
    try {
      const raw = localStorage.getItem(this.sessionKey());
      return raw ? (JSON.parse(raw) as SessionSnapshot) : null;
    } catch {
      return null;
    }
  }

  private saveDocSession(): void {
    const snap: SessionSnapshot = {
      // o modo Comparar não é persistido: a sessão guarda o painel que ele substituiu
      pane: this.pane === DIFF_PANE_ID ? this.diffPrevPane : this.pane,
      editorOpen: this.editorOpen,
      zoom: this.zoom,
      pan: { x: this.pan.x, y: this.pan.y },
      selection: [...this.selection],
      selectedEnum: this.selectedEnum,
      savedAt: Date.now(),
    };
    try {
      localStorage.setItem(this.sessionKey(), JSON.stringify(snap));
      this.pruneLocalSessions();
    } catch {
      /* ignorado */
    }
    this.transport.saveSession?.(snap);
  }

  /** Mantém no máximo `MAX_LOCAL_SESSIONS` entradas `dbmodeling.session:*` no `localStorage` (#128). */
  private pruneLocalSessions(): void {
    const prefix = LOCAL_SESSION_PREFIX;
    const currentKey = this.sessionKey();
    const entries: Array<{ key: string; savedAt: number }> = [];
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (!key || !key.startsWith(prefix)) continue;
      let savedAt = 0;
      try {
        const parsed = JSON.parse(localStorage.getItem(key) || '{}') as SessionSnapshot;
        if (typeof parsed.savedAt === 'number' && Number.isFinite(parsed.savedAt)) savedAt = parsed.savedAt;
      } catch {
        savedAt = 0;
      }
      entries.push({ key, savedAt });
    }
    if (entries.length <= MAX_LOCAL_SESSIONS) return;
    entries.sort((a, b) => a.savedAt - b.savedAt);
    let toRemove = entries.length - MAX_LOCAL_SESSIONS;
    for (const entry of entries) {
      if (toRemove <= 0) break;
      if (entry.key === currentKey) continue;
      localStorage.removeItem(entry.key);
      toRemove--;
    }
  }

  private scheduleSessionSave(): void {
    if (this.sessionTimer !== null) window.clearTimeout(this.sessionTimer);
    this.sessionTimer = window.setTimeout(() => {
      this.sessionTimer = null;
      this.saveDocSession();
    }, 150);
  }

  // ───────────────────────── documento / análise ─────────────────────────

  private setDocument(
    source: string,
    layout?: SidecarLayout,
    meta?: DocumentMetadata
  ): void {
    const documentChanged = meta?.documentId
      ? meta.documentId !== this.documentId
      : !!(meta?.title && meta.title !== this.title);
    const pending = this.source !== this.hostSource || this.patchError !== null;
    if (pending && (documentChanged || source !== this.source)) {
      if (!documentChanged && source === this.hostSource && meta?.saved && this.patchError) {
        this.scheduleAutosave();
      }
      if (documentChanged || source !== this.hostSource) {
        this.toast(t('O documento mudou fora desta tela. Exporte suas alterações antes de recarregar.'));
      }
      return;
    }
    if (!documentChanged && source === this.source && meta?.saved) this.patchError = null;
    if (meta?.documentId) this.documentId = meta.documentId;
    if (meta?.autosave !== undefined) this.autosave = meta.autosave;
    if (meta?.saved) {
      this.savedSource = source;
      this.canDiscard = true;
    }
    if (meta?.title) this.title = meta.title;
    if (meta?.files) this.files = meta.files;
    const first = (this.source === '' && !this.scene) || documentChanged;
    if (documentChanged) {
      window.clearTimeout(this.autosaveTimer);
      if (this.saveTimer) window.clearTimeout(this.saveTimer);
      this.saveTimer = null;
      this.positions = {};
      this.edges = {};
      this.extraLayout = {};
      this.needsFit = true;
    }
    if (layout) {
      this.extraLayout = layout;
      this.edges = structuredClone(layout.edges || {});
      if (layout.tables && (first || Object.keys(this.positions).length === 0)) {
        this.positions = { ...layout.tables } as Record<string, Pt>;
      }
      if (layout.viewport && first) {
        this.zoom = layout.viewport.zoom;
        this.pan = { x: layout.viewport.pan_x, y: layout.viewport.pan_y };
        this.needsFit = false;
      }
    }
    if (first) {
      this.canDiscard = meta?.saved !== false || meta.savedSource !== undefined;
      this.savedSource = meta?.savedSource ?? source;
      this.hostSource = source;
      this.undoStack = [];
      this.redoStack = [];
      const session = meta?.session ?? this.loadDocSession();
      if (session) {
        if (session.pane === 'files' || session.pane === 'schema' || session.pane === 'problems') {
          this.pane = session.pane;
        }
        if (typeof session.editorOpen === 'boolean') {
          this.editorOpen = session.editorOpen;
        }
        if (
          typeof session.zoom === 'number' &&
          Number.isFinite(session.zoom) &&
          session.zoom > 0 &&
          session.pan &&
          Number.isFinite(session.pan.x) &&
          Number.isFinite(session.pan.y)
        ) {
          this.zoom = session.zoom;
          this.pan = { x: session.pan.x, y: session.pan.y };
          this.needsFit = false;
        }
        if (Array.isArray(session.selection)) {
          this.selection = session.selection.filter((s): s is string => typeof s === 'string');
        }
        if (typeof session.selectedEnum === 'string' || session.selectedEnum === null) {
          this.selectedEnum = session.selectedEnum;
        }
      }
    }
    this.hostSource = source;
    if (source === this.source && !first) {
      this.renderChrome();
      this.scheduleRender();
      return;
    }
    this.source = source;
    this.syncEditorValue();
    this.reanalyze();
    // outro arquivo: a base do git é a do arquivo novo
    if (documentChanged && this.diff.status !== 'off') this.startDiff();
    this.renderAll();
    this.maybeStartGuide(meta?.guideState);
  }

  /** Registra a falha de uma chamada ao Wasm; se for fatal, tenta reinstanciar o módulo ou avisa o usuário (#76). */
  private wasmFailed(context: string, e: unknown): boolean {
    console.error(`Error in ${context}:`, e);
    if (!isFatalWasmError(e)) return false;
    this.session = null;
    if (!this.engineFailed) {
      this.engineFailed = true;
      if (this.wasm?.reinit && this.wasmRecoveryCount < MAX_WASM_RECOVERY_ATTEMPTS && !this.wasmRecovering) {
        this.wasmRecovering = true;
        this.wasmRecoveryCount++;
        this.toast(t('O motor de análise falhou; reiniciando…'));
        this.renderStatus();
        void this.wasm
          .reinit()
          .then((fresh) => {
            if (this.destroyed) return;
            this.wasm = fresh;
            this.engineFailed = false;
            this.wasmRecovering = false;
            this.reanalyze();
            this.renderAll();
            this.toast(t('Motor de análise recuperado.'));
          })
          .catch((reinitErr) => {
            console.error('Failed to re-instantiate Wasm:', reinitErr);
            this.wasmRecovering = false;
            this.toast(t('O motor de análise falhou. Exporte o DBML e recarregue a página para continuar.'));
            this.renderStatus();
          });
      } else {
        this.toast(t('O motor de análise falhou. Exporte o DBML e recarregue a página para continuar.'));
        this.renderStatus();
      }
    }
    return true;
  }

  private reanalyze(): void {
    if (!this.wasm || this.engineFailed) return;
    try {
      const r = this.analysisSession()?.analyze(this.source) ?? this.wasm.parse_and_analyze(this.source);
      const hasError = (r.diagnostics as Diagnostic[]).some((d) => d.severity === 'error');
      this.diags = r.diagnostics;
      this.transport.sendDiagnostics?.(this.diags);
      this.enums = r.enums || [];
      // mantém o último esquema válido durante a digitação para não piscar o Empty State
      if (hasError && r.schema.tables.length === 0 && this.lastValid) {
        this.schema = this.lastValid;
      } else {
        this.schema = r.schema;
        if (!hasError || r.schema.tables.length > 0) this.lastValid = r.schema;
      }
    } catch (e) {
      this.wasmFailed('analyzing DBML via Wasm', e);
    }
    this.selection = this.selection.filter((n) => this.schema.tables.some((t) => qualifiedTableName(t) === n));
    this.computeScene();
    this.syncDiff();
  }

  /** Sessão Wasm (criada sob demanda); `null` se o host não a exporta ou já foi liberada. */
  private analysisSession(): WasmSession | null {
    if (this.session) return this.session;
    if (this.destroyed || !this.wasm?.Session) return null;
    this.session = new this.wasm.Session();
    return this.session;
  }

  private computeScene(): void {
    if (!this.wasm || this.engineFailed) return;
    const known: Record<string, Pt> = {};
    for (const t of this.schema.tables) {
      const qname = qualifiedTableName(t);
      if (this.positions[qname]) known[qname] = this.positions[qname];
    }
    if (this.pendingNew && !known[this.pendingNew]) {
      const c = this.viewCenter();
      known[this.pendingNew] = { x: Math.round(c.x - CARD_W / 2), y: Math.round(c.y - 60) };
      this.positions[this.pendingNew] = known[this.pendingNew];
    }
    try {
      this.scene = this.analysisSession()?.compute_scene(this.source, known) ?? this.wasm.compute_scene(this.source, known);
      this.edgesDirty = false;
      for (const k of Object.keys(this.scene!.positions)) {
        if (!this.positions[k]) this.positions[k] = this.scene!.positions[k];
      }
    } catch (e) {
      this.wasmFailed('compute_scene', e);
    }
    this.pendingNew = null;
  }

  private viewCenter(): Pt {
    const r = this.els['surface'].getBoundingClientRect();
    return { x: (r.width / 2 - this.pan.x) / this.zoom, y: (r.height / 2 - this.pan.y) / this.zoom };
  }

  // ───────────────────────── comandos / patches ─────────────────────────

  private isDirty(): boolean {
    return !this.canDiscard || this.source !== this.savedSource || this.source !== this.hostSource || this.patchError !== null;
  }

  private clonePositions(pos: Record<string, Pt> = this.positions): Record<string, Pt> {
    const out: Record<string, Pt> = {};
    for (const k of Object.keys(pos)) {
      out[k] = { x: pos[k].x, y: pos[k].y };
    }
    return out;
  }

  private positionsEqual(a: Record<string, Pt>, b: Record<string, Pt>): boolean {
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (const k of ka) {
      const pa = a[k];
      const pb = b[k];
      if (!pb || pa.x !== pb.x || pa.y !== pb.y) return false;
    }
    return true;
  }

  private captureSnapshot() {
    return {
      source: this.source,
      positions: this.clonePositions(),
      selection: [...this.selection],
      selectedEnum: this.selectedEnum,
      selectedRef: this.selectedRef ? { ...this.selectedRef } : null,
      edges: structuredClone(this.edges),
    };
  }

  private pushHistory(snap = this.captureSnapshot()): void {
    const last = this.undoStack[this.undoStack.length - 1];
    if (last && last.source === snap.source && this.positionsEqual(last.positions, snap.positions) && JSON.stringify(last.edges) === JSON.stringify(snap.edges)) {
      return;
    }
    if (this.undoStack.length >= 100) this.undoStack.shift();
    this.undoStack.push(snap);
    this.redoStack = [];
  }

  private restoreSnapshot(snap: {
    source: string;
    positions: Record<string, Pt>;
    selection: string[];
    selectedEnum: string | null;
    selectedRef: Workbench['selectedRef'];
    edges: Record<string, EdgeStyle>;
  }): void {
    const sourceChanged = snap.source !== this.source;
    const from = this.clonePositions();
    this.positions = this.clonePositions(snap.positions);
    this.edges = structuredClone(snap.edges);
    this.selection = [...snap.selection];
    this.selectedEnum = snap.selectedEnum;
    this.selectedRef = snap.selectedRef ? { ...snap.selectedRef } : null;
    if (sourceChanged) {
      const previous = this.source;
      const patch = this.fullReplace(snap.source);
      this.source = snap.source;
      this.sendPatch(patch, previous);
      this.syncEditorValue();
    }
    this.reanalyze();
    this.queueSaveLayout();
    // desfazer/refazer um movimento: os cartões deslizam de volta
    const to = this.positions;
    this.positions = from;
    this.glide(() => { this.positions = to; });
  }

  private undo(): void {
    const prev = this.undoStack.pop();
    if (!prev) return;
    if (this.redoStack.length >= 100) this.redoStack.shift();
    this.redoStack.push(this.captureSnapshot());
    this.restoreSnapshot(prev);
  }

  private redo(): void {
    const next = this.redoStack.pop();
    if (!next) return;
    if (this.undoStack.length >= 100) this.undoStack.shift();
    this.undoStack.push(this.captureSnapshot());
    this.restoreSnapshot(next);
  }

  private fullReplace(updated: string): TextPatch {
    const old = this.source;
    const lines = old.split('\n');
    return {
      range: {
        start: { line: 0, character: 0 },
        end: { line: lines.length - 1, character: lines[lines.length - 1].length },
      },
      byte_start: 0,
      byte_end: new TextEncoder().encode(old).length,
      new_text: updated,
      updated_source: updated,
    };
  }

  private applyPatch(patch: TextPatch): void {
    if (!patch.new_text && patch.byte_start === patch.byte_end) return;
    const previous = this.source;
    this.source = patch.updated_source ?? this.source;
    this.sendPatch(patch, previous);
    this.syncEditorValue();
    this.reanalyze();
  }

  private sendPatch(patch: TextPatch, previous: string): void {
    const updated = patch.updated_source ?? this.source;
    const documentId = this.documentId;
    this.patchQueue = this.patchQueue.then(async () => {
      if (this.patchError) return;
      try {
        await this.transport.sendPatch({ ...patch, expected_source: previous, expected_file: documentId });
        this.hostSource = updated;
      } catch (error) {
        this.patchError = error instanceof Error ? error : new Error(String(error));
        console.error('DBML persistence failed:', error);
        this.toast(this.persistenceHint(this.patchError.message));
      }
    });
    this.scheduleAutosave();
  }

  private scheduleAutosave(): void {
    window.clearTimeout(this.autosaveTimer);
    if (!this.autosave || !this.transport.saveDocument) return;
    this.autosaveTimer = window.setTimeout(() => {
      if (this.isDirty()) void this.saveDocument();
    }, 600);
  }

  private saveDocument(): Promise<boolean> {
    window.clearTimeout(this.autosaveTimer);
    const source = this.source;
    const documentId = this.documentId;
    const result = this.patchQueue.then(async () => {
      try {
        if (this.hostSource !== source) {
          const old = this.hostSource;
          const lines = old.split('\n');
          await this.transport.sendPatch({
            range: { start: { line: 0, character: 0 }, end: { line: lines.length - 1, character: lines[lines.length - 1].length } },
            byte_start: 0, byte_end: new TextEncoder().encode(old).length,
            new_text: source, expected_source: old, expected_file: documentId,
          });
          this.hostSource = source;
        }
        await this.transport.saveDocument?.(source, documentId);
        this.patchError = null;
        if (this.hostSource === source) this.savedSource = source;
        this.canDiscard = true;
        this.renderChrome();
        return true;
      } catch (error) {
        this.patchError = error instanceof Error ? error : new Error(String(error));
        console.error('DBML persistence failed:', error);
        this.toast(this.persistenceHint(this.patchError.message));
        return false;
      }
    });
    this.patchQueue = result.then(() => undefined);
    return result;
  }

  private dispatch(cmd: Command): TextPatch | null {
    if (!this.wasm || this.engineFailed) return null;
    try {
      const before = this.captureSnapshot();
      const patch: TextPatch = this.wasm.apply_command(this.source, cmd);
      if (patch.new_text || patch.byte_start !== patch.byte_end) {
        this.pushHistory(before);
      }
      if (patch.renamed_table) {
        const [o, n] = patch.renamed_table;
        if (this.positions[o]) {
          this.positions[n] = this.positions[o];
          delete this.positions[o];
        }
        this.selection = this.selection.map((s) => (s === o ? n : s));
        if (this.top === o) this.top = n;
      }
      if (patch.created_table) {
        this.pendingNew = patch.created_table;
        this.selection = [patch.created_table];
        this.selectedEnum = null;
        this.prefs.inspector = true;
        this.focusCol = null;
      }
      if (patch.renamed_enum) {
        const [o, n] = patch.renamed_enum;
        const ok = `enum:${o}`, nk = `enum:${n}`;
        if (this.positions[ok]) {
          this.positions[nk] = this.positions[ok];
          delete this.positions[ok];
        }
        if (this.selectedEnum === o) this.selectedEnum = n;
      }
      if (patch.created_enum) {
        this.selectedEnum = patch.created_enum;
        this.selection = [];
        this.selectedRef = null;
        this.prefs.inspector = true;
      }
      if (patch.created_column) {
        this.selection = [patch.created_column[0]];
        this.openColTable = patch.created_column[0];
        this.openCol = patch.created_column[1];
        this.focusCol = patch.created_column;
        this.prefs.inspector = true;
      }
      this.applyPatch(patch);
      if (patch.should_relayout) this.relayout(false, false);
      this.scheduleRender();
      return patch;
    } catch (e) {
      if (!this.wasmFailed('apply_command', e)) this.toast(errorMessage(e));
      return null;
    }
  }

  private relayout(fit = true, recordHistory = true): void {
    if (!this.wasm || this.engineFailed) return;
    try {
      if (recordHistory) this.pushHistory();
      const r = this.wasm.compute_layout(this.source, {});
      this.glide(() => {
        this.positions = { ...r.positions };
        this.computeScene();
        if (fit) this.fit();
      });
      this.queueSaveLayout();
    } catch (e) {
      this.wasmFailed('auto-layout', e);
    }
  }

  private persistenceHint(message: string): string {
    return /^(Disconnected;|Connection lost;|Active file changed;|Document changed outside this editor;)/.test(message)
      ? diagnosticMessage(message)
      : t('Suas alterações continuam abertas. Tente salvar novamente ou exporte uma cópia em DBML.');
  }

  private layoutFailed(error: unknown, dbmlSaved = false): void {
    console.error('Diagram layout save failed:', error);
    this.toast(t(dbmlSaved
      ? 'O DBML foi salvo, mas o layout do diagrama não foi gravado. Tente salvar novamente.'
      : 'O layout do diagrama não foi gravado. Tente salvar novamente.'));
  }

  private async openProjectFile(path: string): Promise<void> {
    try {
      await this.transport.openFile?.(path);
    } catch (error) {
      console.error('Project file open failed:', error);
      const message = error instanceof Error ? error.message : String(error);
      this.toast(/not found|no such file|enoent/i.test(message)
        ? t('O arquivo foi movido ou removido. Escolha o arquivo novamente.')
        : /permission|access denied|eacces/i.test(message)
          ? t('Sem acesso ao arquivo. Verifique as permissões ou abra uma cópia.')
          : t('Não foi possível abrir o arquivo. Selecione-o novamente.'));
    }
  }

  private async saveLayout(): Promise<void> {
    if (this.saveTimer) window.clearTimeout(this.saveTimer);
    this.saveTimer = null;
    const layout: SidecarLayout = {
      ...(this.extraLayout as SidecarLayout),
      tables: { ...this.positions },
      edges: Object.fromEntries(Object.entries(this.edges).sort(([a], [b]) => a.localeCompare(b))),
      viewport: { zoom: this.zoom, pan_x: this.pan.x, pan_y: this.pan.y },
    };
    await this.transport.saveLayout(layout, this.documentId);
  }

  private queueSaveLayout(): void {
    if (this.saveTimer) window.clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => {
      void this.saveLayout().catch((error) => this.layoutFailed(error));
    }, 300);
  }

  // ───────────────────────── câmera ─────────────────────────

  private fit(): void {
    const r = this.els['surface'].getBoundingClientRect();
    if (!this.scene || r.width === 0) return;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const t of this.scene.tables) {
      const p = this.positions[t.name] || t;
      x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y);
      x1 = Math.max(x1, p.x + CARD_W); y1 = Math.max(y1, p.y + t.h);
    }
    for (const g of this.scene.groups) {
      x0 = Math.min(x0, g.x); y0 = Math.min(y0, g.y);
      x1 = Math.max(x1, g.x + g.w); y1 = Math.max(y1, g.y + g.h);
    }
    for (const e of this.scene.enums ?? []) {
      const p = this.positions[`enum:${e.name}`] || e;
      x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y);
      x1 = Math.max(x1, p.x + e.w); y1 = Math.max(y1, p.y + e.h);
    }
    if (!Number.isFinite(x0)) return;
    const w = x1 - x0, h = y1 - y0;
    // a ilha de ferramentas flutua à esquerda (cresce com a escala da UI): o diagrama fica à direita dela
    const tr = this.els['tools']?.getBoundingClientRect();
    const left = tr && tr.width ? Math.max(0, tr.right - r.left) + 16 : 60;
    const avail = Math.max(1, r.width - left - 16);
    const z = Math.min(Math.max(Math.min(avail / w, (r.height - 140) / h, 1.2), MIN_ZOOM), MAX_ZOOM);
    this.zoom = z;
    this.pan = { x: left + (avail - w * z) / 2 - x0 * z, y: (r.height - h * z) / 2 - y0 * z + 14 };
    this.needsFit = false;
    this.scheduleSessionSave();
  }

  private zoomAt(cx: number, cy: number, factor: number): void {
    const z = Math.min(Math.max(this.zoom * factor, MIN_ZOOM), MAX_ZOOM);
    const k = z / this.zoom;
    this.pan = { x: cx - (cx - this.pan.x) * k, y: cy - (cy - this.pan.y) * k };
    this.zoom = z;
    this.scheduleSessionSave();
  }

  private toWorld(e: { clientX: number; clientY: number }): Pt {
    const r = this.els['surface'].getBoundingClientRect();
    return { x: (e.clientX - r.left - this.pan.x) / this.zoom, y: (e.clientY - r.top - this.pan.y) / this.zoom };
  }

  private focusTable(name: string, highlightCol: string | null = null): void {
    const p = this.positions[name];
    const t = this.schema.tables.find((x) => qualifiedTableName(x) === name);
    if (!p || !t) return;
    this.selection = [name];
    this.selectedRef = null;
    this.selectedEnum = null;
    this.highlightedColumn = highlightCol ? [name, highlightCol] : null;
    if (highlightCol) {
      this.openColTable = name;
      this.openCol = highlightCol;
    }
    this.top = name;
    this.prefs.inspector = true;
    this.savePrefs();
    this.renderAll(); // seleção e inspector já; a câmera desliza até a tabela
    const r = this.els['surface'].getBoundingClientRect();
    const targetZoom = this.zoom < 0.85 ? 1 : this.zoom;
    this.glide(() => {
      this.zoom = targetZoom;
      this.pan = {
        x: r.width / 2 - (p.x + CARD_W / 2) * targetZoom,
        y: r.height / 2 - (p.y + cardH(t.columns.length) / 2) * targetZoom,
      };
    });
  }

  private focusEnum(name: string): void {
    const ev = this.scene?.enums?.find((e) => e.name === name);
    const p = this.positions[`enum:${name}`] || (ev ? { x: ev.x, y: ev.y } : null);
    if (!ev || !p) return;
    this.selection = [];
    this.selectedRef = null;
    this.selectedEnum = name;
    this.highlightedColumn = null;
    this.prefs.inspector = true;
    this.savePrefs();
    this.renderAll();
    const r = this.els['surface'].getBoundingClientRect();
    const targetZoom = this.zoom < 0.85 ? 1 : this.zoom;
    this.glide(() => {
      this.zoom = targetZoom;
      this.pan = {
        x: r.width / 2 - (p.x + ev.w / 2) * targetZoom,
        y: r.height / 2 - (p.y + ev.h / 2) * targetZoom,
      };
    });
  }

  /** Computes 1-hop or 2-hop FK neighborhood subgraph from current selection (mirrors `Schema::neighborhood`). */
  private focusSubgraph(): { tables: Set<string>; edges: Set<string> } | null {
    if (!this.focusMode || this.selection.length === 0 || !this.scene) return null;
    const visited = new Set<string>(this.selection);
    let frontier = new Set<string>(this.selection);
    const edges = new Set<string>();
    for (let step = 0; step < this.focusDepth; step++) {
      if (frontier.size === 0) break;
      const next = new Set<string>();
      for (const e of this.scene.edges) {
        const tf = frontier.has(e.from_table);
        const tt = frontier.has(e.to_table);
        if (tf || tt) {
          edges.add(edgeKey(e));
          if (!visited.has(e.from_table)) next.add(e.from_table);
          if (!visited.has(e.to_table)) next.add(e.to_table);
        }
      }
      for (const n of next) visited.add(n);
      frontier = next;
    }
    return { tables: visited, edges };
  }


  // ───────────────────────── modo Comparar (diff com o git HEAD) ─────────────────────────

  private toggleDiff(): void {
    if (this.diff.status === 'off') this.startDiff();
    else this.exitDiff();
  }

  /** Liga o modo: abre o painel de resumo, pede a base ao host e calcula o diff (somente leitura). */
  private startDiff(): void {
    if (this.pane !== DIFF_PANE_ID) this.diffPrevPane = this.pane;
    this.pane = DIFF_PANE_ID;
    this.prefs.sidebar = true;
    this.editorOpen = false;
    this.diff = { status: 'loading' };
    const seq = ++this.diffSeq;
    this.diffHead = this.source;
    this.renderAll();
    void loadDiff(this.transport, this.wasm, this.source, this.documentId).then((state) => {
      if (this.destroyed || seq !== this.diffSeq || this.diff.status === 'off') return;
      this.diff = state;
      this.diffHead = this.source;
      // o texto pode ter mudado enquanto a base chegava
      this.syncDiff();
      this.announce(state.status === 'error' ? t(diffErrorMessageKey(state.code)) : t('Comparar com o git HEAD'));
      this.renderAll();
    });
  }

  private exitDiff(): void {
    if (this.diff.status === 'off') return;
    this.diffSeq++;
    this.diff = DIFF_OFF;
    this.diffGhostPositions = {};
    if (this.pane === DIFF_PANE_ID) this.pane = this.diffPrevPane;
    this.renderAll();
  }

  /** Recalcula o diff contra a base guardada quando o texto mudou (sem nova consulta ao git). */
  private syncDiff(): void {
    if (this.diff.status !== 'ready' || this.diffHead === this.source) return;
    this.diffHead = this.source;
    this.diff = refreshDiff(this.diff, this.wasm, this.source);
  }

  /** Posições dos cartões fantasma: espaço livre fora dos cartões e enums do head. */
  private ghostLayout(view: DiffView): Record<string, Pt> {
    const occupied: Rect[] = occupiedRects(this.schema.tables, this.positions, view);
    for (const ev of this.scene?.enums ?? []) {
      const p = this.positions[`enum:${ev.name}`] || { x: ev.x, y: ev.y };
      occupied.push({ x: p.x, y: p.y, w: ev.w, h: ev.h });
    }
    this.diffGhostPositions = placeGhosts(occupied, ghostSlots(view));
    return this.diffGhostPositions;
  }

  /** Clique no resumo: seleciona (quando existe no head) e leva a câmera até o item. */
  private focusDiffItem(id: string): void {
    if (this.diff.status !== 'ready') return;
    const view = this.diff.view;
    const item = view.items.find((i) => i.id === id);
    if (!item) return;
    const enumRects = new Map<string, Rect>();
    for (const ev of this.scene?.enums ?? []) {
      const p = this.positions[`enum:${ev.name}`] || { x: ev.x, y: ev.y };
      enumRects.set(ev.name, { x: p.x, y: p.y, w: ev.w, h: ev.h });
    }
    const ctx: FocusContext = {
      view,
      tables: this.schema.tables,
      positions: this.positions,
      ghostPositions: this.ghostLayout(view),
      enumRects,
    };
    const rect = focusRect(item, ctx);
    if (!rect) return;
    const target = item.target;
    this.selection = target.kind === 'table' && !target.removed ? [target.name] : [];
    this.selectedEnum = target.kind === 'enum' && !target.removed ? target.name : null;
    this.selectedRef = null;
    this.highlightedColumn = null;
    if (this.selection.length) this.top = this.selection[0];
    this.renderAll();
    const r = this.els['surface'].getBoundingClientRect();
    const cam = cameraFor(rect, { x: r.width, y: r.height }, this.zoom);
    this.glide(() => {
      this.zoom = cam.zoom;
      this.pan = cam.pan;
    });
  }

  // ───────────────────────── movimento ─────────────────────────

  private fxStart(sel: string, cls: string, dur: number = MOTION.base): void {
    if (noMotion()) return;
    this.fx = this.fx.filter((f) => f.sel !== sel || f.cls !== cls);
    this.fx.push({ sel, cls, t0: performance.now(), dur });
  }

  /** Reaplica os efeitos ativos nos elementos recém-recriados; os vencidos saem. */
  private applyFx(): void {
    if (!this.fx.length) return;
    const now = performance.now();
    const all = (sel: string) => { try { return Array.from(this.wb.querySelectorAll<HTMLElement>(sel)); } catch { return []; } };
    this.fx = this.fx.filter((f) => {
      const live = now - f.t0 < f.dur;
      for (const el of all(f.sel)) {
        el.classList.toggle(f.cls, live);
        if (live) {
          el.style.setProperty('--fx-dur', `${f.dur}ms`);
          el.style.setProperty('--fx-delay', `-${Math.round(now - f.t0)}ms`);
        }
      }
      return live;
    });
  }

  /** Mostra/oculta um painel deslizando; `side` é a margem que recolhe (painéis) ou nenhum (flutuantes). */
  private setShown(el: HTMLElement, show: boolean, display: string, side: 'Left' | 'Right' | null): void {
    const was = this.shown.get(el);
    this.shown.set(el, show);
    if (was === undefined || noMotion()) { el.style.display = show ? display : 'none'; return; }
    if (was === show) return;
    el.getAnimations().forEach((a) => a.cancel());
    if (show) el.style.display = display;
    const w = parseFloat(getComputedStyle(el).width) || 0;
    const hidden: Keyframe = side ? { [`margin${side}`]: `-${w}px`, opacity: 0 } : { opacity: 0, transform: 'translateY(8px)' };
    const a = el.animate(show ? [hidden, {}] : [{}, hidden], { duration: MOTION.slow, easing: 'cubic-bezier(.2,.8,.2,1)' });
    if (!show) a.onfinish = () => { if (this.shown.get(el) === false) el.style.display = 'none'; };
  }

  private stopTween(): void {
    if (this.tweenRaf !== null) cancelAnimationFrame(this.tweenRaf);
    this.tweenRaf = null;
  }

  /** Interpola no canvas (só mundo e ilhas: o chrome não muda durante o movimento). */
  private tween(step: (k: number) => void, done: () => void): void {
    this.stopTween();
    if (noMotion()) { step(1); done(); this.scheduleRender(); return; }
    const t0 = performance.now();
    const tick = (now: number) => {
      const k = Math.min(1, (now - t0) / MOTION.slow);
      step(easeOut(k));
      if (k < 1) {
        this.renderWorld();
        this.renderIslands();
        this.tweenRaf = requestAnimationFrame(tick);
      } else {
        this.tweenRaf = null;
        done();
        this.scheduleRender();
      }
    };
    this.tweenRaf = requestAnimationFrame(tick);
  }

  /** Aplica `change` (que altera zoom/pan/posições) e desliza do estado anterior até o novo. */
  private glide(change: () => void): void {
    const z0 = this.zoom, p0 = { ...this.pan }, from = this.clonePositions();
    change();
    const z1 = this.zoom, p1 = { ...this.pan }, to = this.positions;
    const moved = Object.keys(to).filter((k) => from[k] && (from[k].x !== to[k].x || from[k].y !== to[k].y));
    if (!moved.length && z0 === z1 && p0.x === p1.x && p0.y === p1.y) { this.scheduleRender(); return; }
    this.tween(
      (k) => {
        this.zoom = lerp(z0, z1, k);
        this.pan = { x: lerp(p0.x, p1.x, k), y: lerp(p0.y, p1.y, k) };
        if (moved.length) {
          const cur = { ...to };
          for (const n of moved) cur[n] = { x: lerp(from[n].x, to[n].x, k), y: lerp(from[n].y, to[n].y, k) };
          this.positions = cur;
          this.recomputeEdges();
        }
      },
      () => {
        this.positions = to;
        if (moved.length) { this.recomputeEdges(); this.queueSaveLayout(); }
        this.scheduleSessionSave();
      }
    );
  }

  /** Compara com o render anterior e dispara os efeitos do que mudou (antes de recriar o DOM). */
  private detectFx(): void {
    const tables = new Set(this.schema.tables.map((t) => qualifiedTableName(t)));
    const edges = new Set((this.scene?.edges || []).map((e) => edgeKey(e)));
    const sel = new Set(this.selection);
    const insp = [...this.selection, this.selectedEnum || '', this.selectedRef ? edgeKey(this.selectedRef) : ''].join('|');
    const pane = String(this.leftPane());
    const prev = this.seen;
    this.seen = { title: this.title, tables, edges, sel, insp, pane };
    if (!prev || noMotion()) return;
    const q = cssQ;
    if (prev.title !== this.title || prev.tables.size === 0) {
      // outro arquivo (ou primeira carga): o diagrama inteiro entra de uma vez
      this.fxStart('#world > *', 'fx-fade', MOTION.slow);
    } else {
      const added = [...tables].filter((n) => !prev.tables.has(n));
      const removed = [...prev.tables].filter((n) => !tables.has(n));
      // 1 saiu + 1 entrou = renomear: não pisca
      if (!(added.length === 1 && removed.length === 1)) {
        for (const n of added) this.fxStart(`.card[data-table="${q(n)}"]`, 'fx-pop');
        for (const n of removed) {
          const old = this.els['world'].querySelector<HTMLElement>(`.card[data-table="${q(n)}"]`);
          if (!old) continue;
          const node = old.cloneNode(true) as HTMLElement;
          node.removeAttribute('data-table');
          node.classList.add('fx-out');
          this.ghosts.push({ node, t0: performance.now() });
          window.setTimeout(() => this.scheduleRender(), MOTION.base + 20); // tira o clone do DOM
        }
      }
      for (const k of edges) if (!prev.edges.has(k)) this.fxStart(`path.edge[data-ek="${q(k)}"]`, 'fx-fade', MOTION.slow);
    }
    let selChanged = sel.size !== prev.sel.size;
    for (const n of sel) {
      if (prev.sel.has(n)) continue;
      selChanged = true;
      this.fxStart(`.card[data-table="${q(n)}"]`, 'fx-sel', MOTION.slow);
      this.fxStart(`.trow.on[data-table="${q(n)}"]`, 'fx-row');
    }
    if (selChanged) this.fxStart('svg.edges path.halo', 'fx-fade');
    if (prev.insp !== insp) this.fxStart('#inspector > *', 'fx-in');
    if (prev.pane !== pane) this.fxStart('#sidebar > *', 'fx-in');
  }

  // ───────────────────────── renderização ─────────────────────────

  /** Marca regiões específicas para reconstrução no próximo quadro (`Dirty.*`, #108). */
  public invalidate(mask: number): void {
    if (this.destroyed) return;
    this.dirtyMask |= mask;
    this.requestFrame();
  }

  /** Pedido de render completo (cromo + mundo); vence qualquer pedido só de câmera do mesmo quadro. */
  private scheduleRender(): void {
    this.invalidate(Dirty.All);
  }

  /** Pan: muda só o transform do mundo e o fundo em grade, sem refazer o DOM (#108). */
  private scheduleCamera(): void {
    this.invalidate(Dirty.Camera);
  }

  private requestFrame(): void {
    if (this.raf !== null) return;
    this.raf = requestAnimationFrame(() => {
      this.raf = null;
      const mask = this.dirtyMask;
      this.dirtyMask = 0;
      if (mask === 0) return;
      if (mask === Dirty.Camera) {
        this.applyCamera();
      } else {
        this.renderAll(mask);
      }
    });
  }

  private applyCamera(): void {
    this.els['world'].style.transform = `translate(${this.pan.x}px, ${this.pan.y}px) scale(${this.zoom})`;
    this.els['surface'].style.backgroundPosition = `${this.pan.x}px ${this.pan.y}px`;
    this.renderMinimap();
  }

  private renderAll(mask: number = Dirty.All): void {
    const wantsWorld = (mask & Dirty.World) !== 0;
    const wantsChrome = (mask & Dirty.Chrome) !== 0;
    if (wantsWorld && this.edgesDirty) this.recomputeEdges();
    if (wantsWorld || wantsChrome) this.detectFx();
    // chrome primeiro: o ajuste à tela depende da largura que os painéis deixam ao canvas
    if (wantsChrome) this.renderChrome(mask);
    if (!wantsWorld) {
      if ((mask & Dirty.Camera) !== 0) this.applyCamera();
      return;
    }
    const { width, height } = this.els['surface'].getBoundingClientRect();
    const previous = this.viewport;
    this.viewport = width > 0 && height > 0 ? { x: width, y: height } : null;
    if (this.needsFit && this.scene) {
      this.fit();
      if (this.els['surface'].getBoundingClientRect().width > 0) this.needsFit = false;
      this.renderIslands();
      this.renderStatus();
    } else if (previous && this.viewport && (width !== previous.x || height !== previous.y)) {
      const dx = (width - previous.x) / 2;
      const dy = (height - previous.y) / 2;
      this.pan.x += dx;
      this.pan.y += dy;
      const selected = this.scene?.tables.find((table) => table.name === this.selection[0]);
      const pos = selected && (this.positions[selected.name] || selected);
      if (selected && pos) {
        const x = this.pan.x + (pos.x + CARD_W / 2) * this.zoom;
        const y = this.pan.y + (pos.y + selected.h / 2) * this.zoom;
        const insetX = Math.min((CARD_W / 2) * this.zoom + 12, width / 2);
        const insetY = Math.min((selected.h / 2) * this.zoom + 12, height / 2);
        this.pan.x += Math.max(insetX, Math.min(x, width - insetX)) - x;
        this.pan.y += Math.max(insetY, Math.min(y, height - insetY)) - y;
      }
      this.scheduleSessionSave();
    }
    this.renderWorld();
  }

  private leftPane(): Pane | 'dbml' | null {
    return this.editorOpen ? 'dbml' : this.prefs.sidebar ? this.pane : null;
  }

  /** Um painel esquerdo por vez: abrir o editor recolhe a barra lateral. */
  private openEditor(): void {
    this.editorOpen = true;
    this.prefs.sidebar = false;
  }

  private inspectorVisible(): boolean {
    const none = this.selection.length === 0 && !this.selectedRef && !this.selectedEnum;
    return this.prefs.inspector && !(this.prefs.autoHide && none);
  }

  private renderChrome(mask: number = Dirty.Chrome): void {
    if (mask & Dirty.Theme) {
      this.applyTheme();
      this.els['empty'].querySelectorAll<HTMLElement>('[data-i18n]').forEach((el) => { el.textContent = t(el.dataset.i18n!); });
    }
    if (mask & Dirty.Titlebar) this.renderTitlebar();
    if (mask & Dirty.Rails) this.renderRails();
    if (mask & Dirty.Sidebar) this.renderSidebar();
    if (mask & Dirty.Editor) this.renderEditor();
    if (mask & Dirty.Inspector) this.renderInspector();
    if (mask & Dirty.Islands) this.renderIslands();
    if (mask & Dirty.Status) this.renderStatus();
    if (mask & (Dirty.Sidebar | Dirty.Editor | Dirty.Inspector | Dirty.Titlebar | Dirty.Theme)) {
      const left = this.leftPane();
      this.setShown(this.els['editor'], left === 'dbml', 'flex', 'Left');
      this.setShown(this.els['sidebar'], !!left && left !== 'dbml', 'flex', 'Left');
      const leftResize = this.wb.querySelector<HTMLButtonElement>('[data-resize=left]')!;
      const rightResize = this.wb.querySelector<HTMLButtonElement>('[data-resize=right]')!;
      leftResize.hidden = !left;
      rightResize.hidden = !this.inspectorVisible();
      leftResize.setAttribute('aria-label', t('Redimensionar painel esquerdo'));
      rightResize.setAttribute('aria-label', t('Redimensionar inspetor'));
      this.els['empty'].style.display = this.schema.tables.length === 0 && this.enums.length === 0 ? 'flex' : 'none';
    }
    if (mask & Dirty.Legend) this.renderLegend();
    if (mask & (Dirty.Sidebar | Dirty.Inspector | Dirty.Titlebar)) this.applyFx();
  }

  private tint(name: string): string | null {
    return this.scene?.tables.find((t) => t.name === name)?.tint || null;
  }

  private renderWorld(): void {
    const sc = this.scene;
    const w = this.els['world'];
    this.applyCamera();
    // grade de pontos adaptativa (24px no zoom 100%)
    const g = getGlass(this.prefs.mode, 1, this.prefs.translucent);
    const dark = this.prefs.mode === 'dark';
    const step = canvasGridStep(this.zoom);
    const s = this.els['surface'];
    s.style.backgroundImage = `radial-gradient(${g.grid} 1px, transparent 1.2px)`;
    s.style.backgroundSize = `${step}px ${step}px`;
    // cursor da ação em curso: vale sobre cartões e linhas também (CSS `data-cursor`)
    const k = this.drag?.kind;
    const cur = k === 'marquee' || k === 'link' ? 'crosshair'
      : k ? 'grabbing'
      : this.tool === 'hand' || this.space ? 'grab' : '';
    if (cur) this.wb.dataset.cursor = cur; else delete this.wb.dataset.cursor;
    if (k) this.wb.dataset.drag = k; else delete this.wb.dataset.drag;
    s.style.cursor = 'default';
    if (!sc) {
      w.innerHTML = '';
      return;
    }
    const sel = new Set(this.selection);
    const focusSub = this.focusSubgraph();
    const FOCUS_DIM = 0.18;
    const dv = this.diff.status === 'ready' ? this.diff.view : null;
    const ghostPos = dv ? this.ghostLayout(dv) : {};
    let svg = '';
    for (const e of sc.edges) {
      const ek = edgeKey(e);
      const isSelRef =
        !!this.selectedRef &&
        this.selectedRef.from_table === e.from_table &&
        this.selectedRef.from_col === e.from_col &&
        this.selectedRef.to_table === e.to_table &&
        this.selectedRef.to_col === e.to_col;
      const lit = isSelRef || sel.has(e.from_table) || sel.has(e.to_table);
      const dim = focusSub && !focusSub.edges.has(ek) ? FOCUS_DIM : 1;
      const refDiff = dv?.refs.get(ek);
      const col = refDiff && refDiff.status !== 'removed' ? DIFF_STATUS_COLOR_VAR[refDiff.status] : lit ? g.accent : g.relStroke;
      const a = this.positions[e.from_table], b = this.positions[e.to_table];
      if (!a || !b) continue;
      const style = this.edges[ek];
      const curve = curvedEdge(a.x, e.ay, b.x, e.by, style);
      const drawn = edgeDrawing(e, curve, style);
      const d = drawn.path;
      const lab = lit ? g.accent : g.textFaint;
      if (dim < 1) svg += `<g opacity="${dim}">`;
      if (lit) svg += `<path class="halo" d="${d}" fill="none" stroke="${g.accent}" stroke-opacity="0.16" stroke-width="8" stroke-linecap="round" stroke-linejoin="round"/>`;
      svg += `<path class="edge" data-ek="${esc(ek)}" d="${d}" fill="none" stroke="${col}" stroke-width="${refDiff ? DIFF_EDGE_WIDTH : lit ? 1.8 : 1.4}" stroke-linejoin="round"/>`;
      svg += `<path class="edge-hit" d="${d}" fill="none" stroke="transparent" stroke-width="${12 / this.zoom}" data-edge="1" data-from-table="${esc(e.from_table)}" data-from-col="${esc(e.from_col)}" data-to-table="${esc(e.to_table)}" data-to-col="${esc(e.to_col)}"/>`;
      if (isSelRef) svg += `<circle class="edge-handle" data-edge-handle="1" cx="${drawn.midpoint.x}" cy="${drawn.midpoint.y}" r="${4 / this.zoom}" fill="${g.accent}" stroke="${g.card}" stroke-width="${1 / this.zoom}"/>`;
      svg += `<circle cx="${curve.x1}" cy="${e.ay}" r="3" fill="${col}"/><circle cx="${curve.x2}" cy="${e.by}" r="3.5" fill="${g.card}" stroke="${col}" stroke-width="1.5"/>`;
      const lf = `font-family="Geist Mono, monospace" font-size="10" font-weight="500" fill="${lab}"`;
      svg += `<text text-anchor="middle" x="${curve.x1 + Math.sign(curve.d1) * 11}" y="${e.ay - 6}" ${lf}>${e.start_label}</text>`;
      svg += `<text text-anchor="middle" x="${curve.x2 + Math.sign(curve.d2) * 11}" y="${e.by - 6}" ${lf}>${e.end_label}</text>`;
      if (dim < 1) svg += '</g>';
    }
    if (this.drag?.kind === 'link') {
      const d = this.drag;
      const p = this.positions[d.table];
      const t = this.schema.tables.find((x) => qualifiedTableName(x) === d.table);
      const i = t ? t.columns.findIndex((c) => c.name === d.col) : -1;
      if (p && i >= 0) {
        const ay = p.y + HEADER_H + 4 + ROW_H * (i + 0.5);
        svg += `<path d="M ${p.x + CARD_W} ${ay} L ${d.now.x} ${d.now.y}" stroke="${g.accent}" stroke-width="2" stroke-dasharray="6 4" fill="none"/><circle cx="${d.now.x}" cy="${d.now.y}" r="5" fill="${g.accent}"/>`;
      }
    }
    if (dv) svg = renderGhostEdges(removedEdges(dv, this.schema.tables, this.positions, ghostPos)) + svg;
    let html = `<svg class="edges" aria-hidden="true">${svg}</svg>`;
    // molduras de grupo
    for (const gr of canvasGroups(sc.groups, this.schema, this.positions)) {
      const c = gr.color;
      const gDim = focusSub && !gr.tables.some((tb) => focusSub.tables.has(tb)) ? `opacity:${FOCUS_DIM};` : '';
      html += `<div class="group" role="group" aria-label="${esc(translate('Grupo {}', gr.name))}" style="${gDim}left:${gr.x}px;top:${gr.y}px;width:${gr.w}px;height:${gr.h}px;border-color:${hexWithAlpha(c, dark ? 0.18 : 0.22)};background:${hexWithAlpha(c, dark ? 0.06 : 0.07)}">
        <div class="gl" data-group="${esc(gr.name)}" style="color:${c}">${ICON('Layers', 13)}${esc(gr.name)}<span class="gpill" style="background:${hexWithAlpha(c, 0.14)}">${gr.count}</span></div></div>`;
    }
    // cartões
    const order = [...sc.tables].sort((a, b) => Number(a.name === this.top) - Number(b.name === this.top));
    for (const tv of order) {
      const t = this.schema.tables.find((x) => qualifiedTableName(x) === tv.name);
      const p = this.positions[tv.name];
      if (!t || !p) continue;
      const qname = tv.name;
      const tint = tv.tint;
      const tDim = focusSub && !focusSub.tables.has(qname) ? `opacity:${FOCUS_DIM};` : '';
      const fk = new Set(tv.fk_columns);
      const tDiff = dv?.tables.get(qname);
      const idxCols = new Set<string>();
      for (const ix of t.indexes || []) {
        for (const ic of ix.columns || []) {
          if (ic.kind === 'column') idxCols.add(ic.value);
        }
      }
      const rows = t.columns
        .map((c) => {
          const isFk = !!c.fk || fk.has(c.name);
          const hasIdx = idxCols.has(c.name);
          const isHi = this.highlightedColumn?.[0] === qname && this.highlightedColumn?.[1] === c.name;
          const colNote = c.note?.trim();
          const noteBadge = colNote
            ? `<span class="note-ic" style="color:var(--muted);display:flex;flex:none;cursor:help" title="${esc(colNote)}">${ICON('Info', 11)}</span>`
            : '';
          const ic = c.pk
            ? `<span style="color:var(--accent)">${ICON('KeyRound', 12)}</span>`
            : isFk
              ? `<span style="color:var(--link)">${ICON('Link', 12)}</span>`
              : `<span style="color:var(--faint)">${ICON('Dot', 12)}</span>`;
          const badges = [
            hasIdx ? `<span class="badge idx" title="${translate('Indexada')}">IDX</span>` : '',
            c.not_null ? '<span class="badge">NN</span>' : '',
            c.unique ? '<span class="badge">UQ</span>' : '',
          ].join('');
          const rowLabel = [`${c.name}: ${c.ty}`, c.pk ? translate('chave primária') : '', isFk ? translate('chave estrangeira') : '', c.not_null ? translate('obrigatória') : '', c.unique ? translate('única') : ''].filter(Boolean).join(', ');
          const cDiff = tDiff?.columns.get(c.name);
          const diffCls = cDiff ? ` ${diffStatusClass(cDiff.status)}` : '';
          const diffTitle = cDiff && columnDiffTitle(cDiff) ? ` title="${esc(columnDiffTitle(cDiff))}"` : '';
          const diffLabel = cDiff ? `, ${diffStatusLabel(cDiff.status)}` : '';
          return `<div class="row${diffCls}" role="listitem" aria-label="${esc(rowLabel + diffLabel)}"${diffTitle} data-col="${esc(c.name)}" data-table="${esc(qname)}"${isHi ? ' style="background:color-mix(in srgb, var(--accent) 14%, transparent)"' : ''}><div class="rail14">${ic}</div>
            <div class="cn ${c.pk ? 'pk' : ''}">${esc(c.name)}</div>${cDiff ? diffMark(cDiff.status) : ''}${noteBadge}${badges}<div class="ty">${esc(c.ty)}</div>
            <div class="port" data-port="1" data-table="${esc(qname)}" data-col="${esc(c.name)}"></div></div>`;
        })
        .join('');
      const schemaBadge = t.schema ? `<span class="sch">${esc(t.schema)}</span>` : '';
      const tblNote = t.note?.trim();
      const tblNoteBadge = tblNote
        ? `<span class="note-ic" style="color:var(--muted);display:flex;flex:none;cursor:help" title="${esc(tblNote)}">${ICON('Info', 12)}</span>`
        : '';
      const cardLabel = translate(t.columns.length === 1 ? 'Tabela {}, 1 coluna' : 'Tabela {}, {} colunas', qname, t.columns.length) + (tDiff ? `, ${diffStatusLabel(tDiff.status)}` : '') + (sel.has(qname) ? `, ${translate('selecionada')}` : '');
      html += `<div class="card ${sel.has(qname) ? 'sel' : ''}${tDiff ? ` ${diffStatusClass(tDiff.status)}` : ''}" role="group" tabindex="0" aria-label="${esc(cardLabel)}" data-table="${esc(qname)}" style="${tDim}left:${p.x}px;top:${p.y}px;height:${dv ? diffCardHeight(t, dv) : cardH(t.columns.length)}px">
        <div class="head"><span style="color:${tint || g.accent};display:flex">${ICON('Table2', 14)}</span>
        <div class="nm">${esc(t.name)}</div>${schemaBadge}${tblNoteBadge}<div class="cnt">${t.columns.length}</div></div>
        <div class="rows" role="list">${rows}${tDiff ? renderGhostRows(tDiff) : ''}</div>
        <div class="plus" data-addcol="${esc(qname)}" title="${translate("Adicionar coluna")}">${ICON('Plus', 14)}</div></div>`;
    }
    // enums
    for (const ev of sc.enums || []) {
      const p = this.positions[`enum:${ev.name}`] || { x: ev.x, y: ev.y };
      const on = this.selectedEnum === ev.name;
      const referencedBySelected = !on && ev.referenced_by.some((tb) => sel.has(tb));
      const eDim = focusSub && !ev.referenced_by.some((tb) => focusSub.tables.has(tb)) ? `opacity:${FOCUS_DIM};` : '';
      const eDiff = dv?.enums.get(ev.name);
      const valRows = (ev.values.length
        ? ev.values.map((v) => {
            const vs = eDiff?.values.get(v);
            return `<div class="ev${vs ? ` ${diffStatusClass(vs)}` : ''}" role="listitem"><span class="ev-ic">${vs ? diffMark(vs) : ICON('Dot', 12)}</span>${esc(v)}</div>`;
          }).join('')
        : `<div class="ev" role="listitem" style="color:var(--faint)">${translate("(vazio)")}</div>`) + (eDiff ? renderEnumGhostValues(eDiff) : '');
      const enumLabel = translate(ev.values.length === 1 ? 'Enum {}, 1 valor' : 'Enum {}, {} valores', ev.name, ev.values.length) + (on ? `, ${translate('selecionada')}` : '');
      html += `<div class="card enum-card ${on ? 'sel' : ''}${eDiff ? ` ${diffStatusClass(eDiff.status)}` : ''}" role="group" tabindex="0" aria-label="${esc(enumLabel)}" data-enum="${esc(ev.name)}" style="${eDim}left:${p.x}px;top:${p.y}px;${referencedBySelected ? 'border-color:var(--accent);' : ''}">
        <div class="head"><span style="color:var(--accent);display:flex">${ICON('List', 14)}</span>
        <div class="nm">${esc(ev.name)}</div><span class="tag">ENUM</span></div>
        <div class="rows" role="list">${valRows}</div></div>`;
    }
    if (dv) {
      for (const tbl of dv.removedTables) {
        const p = ghostPos[qualifiedTableName(tbl)];
        if (p) html += renderGhostTable(tbl, p);
      }
      for (const en of dv.removedEnums) {
        const p = ghostPos[`enum:${en.name}`];
        if (p) html += renderGhostEnum(en, p);
      }
    }
    const focused = document.activeElement instanceof HTMLElement && w.contains(document.activeElement)
      ? document.activeElement.closest<HTMLElement>('.card')
      : null;
    const focusKey = focused?.dataset.table !== undefined
      ? `[data-table="${cssQ(focused.dataset.table)}"]`
      : focused?.dataset.enum !== undefined ? `[data-enum="${cssQ(focused.dataset.enum)}"]` : null;
    w.innerHTML = html;
    if (focusKey) w.querySelector<HTMLElement>(`.card${focusKey}`)?.focus({ preventScroll: true });
    this.announceSelection();
    // cartões excluídos somem aos poucos (clones fora do fluxo de eventos)
    const now = performance.now();
    this.ghosts = this.ghosts.filter((gh) => now - gh.t0 < MOTION.base);
    for (const gh of this.ghosts) {
      gh.node.style.setProperty('--fx-dur', `${MOTION.base}ms`);
      gh.node.style.setProperty('--fx-delay', `-${Math.round(now - gh.t0)}ms`);
      w.appendChild(gh.node);
    }
    this.applyFx();
    this.drawMarquee();
  }

  /** Texto para leitores de tela (região `aria-live` polida); só repete se o texto mudar. */
  private announce(msg: string): void {
    const live = this.els['live'];
    if (live && live.textContent !== msg) live.textContent = msg;
  }

  private announceSelection(): void {
    const names = [...this.selection, ...(this.selectedEnum ? [this.selectedEnum] : [])];
    const signature = names.join('\u0000');
    if (signature === this.announcedSelection) return;
    this.announcedSelection = signature;
    if (!names.length) return;
    this.announce(names.length === 1
      ? translate('{} selecionada', names[0])
      : translate('{} itens selecionados', names.length));
  }

  /** Só o retângulo: no arrasto do marquee não refaz o mundo (custava ~70ms/evento com 150 tabelas). */
  private drawMarquee(): void {
    const m = this.els['marquee'];
    if (this.drag?.kind === 'marquee') {
      const d = this.drag;
      const a = this.worldToScreen(d.start), b = this.worldToScreen(d.now);
      m.style.display = 'block';
      m.style.left = Math.min(a.x, b.x) + 'px';
      m.style.top = Math.min(a.y, b.y) + 'px';
      m.style.width = Math.abs(a.x - b.x) + 'px';
      m.style.height = Math.abs(a.y - b.y) + 'px';
    } else {
      m.style.display = 'none';
    }
  }

  private worldToScreen(p: Pt): Pt {
    return { x: p.x * this.zoom + this.pan.x, y: p.y * this.zoom + this.pan.y };
  }

  private tbtn(act: string, icon: IconName, tip: string, cls = '', size = 15): string {
    return `<button class="ib ${cls}" data-act="${act}" title="${esc(t(tip))}">${ICON(icon, size)}</button>`;
  }

  private projectName(): string {
    if (this.files.length) {
      const dir = this.files[0].path.split(/[\\/]/).slice(-2, -1)[0];
      if (dir) return dir;
    }
    return 'dbm-proj';
  }

  private renderTitlebar(): void {
    const brand = [
      'left:16%;top:18%;width:38%;height:10%;border-radius:1px 1px 0 0',
      'left:16%;top:28%;width:38%;height:20%;opacity:.5',
      'left:46%;top:52%;width:38%;height:10%;border-radius:1px 1px 0 0',
      'left:46%;top:62%;width:38%;height:20%;opacity:.5',
      'left:31%;top:48%;width:6%;height:21%',
      'left:31%;top:63%;width:15%;height:6%',
    ].map((st) => `<span style="${st}"></span>`).join('');
    const dark = this.prefs.mode === 'dark';
    const left = this.leftPane();
    const hasSel = this.selection.length > 0 || !!this.selectedRef || !!this.selectedEnum;
    const insp = this.inspectorVisible();
    this.els['titlebar'].innerHTML = `
      ${this.tbtn('ToggleSidebar', 'PanelLeft', 'Alternar painel lateral · Ctrl+B', left ? 'on' : '', 16)}
      <div class="tb-sep"></div>
      <button class="proj-btn" data-act="menu:main" title="Menu">
        <div class="brand">${brand}</div>
        <span class="pname">${esc(this.projectName())}</span>
        <span style="color:var(--faint);display:flex">${ICON('ChevronDown', 12)}</span>
      </button>
      <div class="tab"><span style="color:var(--accent);display:flex">${ICON('FileCode', 14)}</span><span class="nm">${esc(this.title)}</span>${this.isDirty() ? `<span class="dirty" title="${t('Alterações não salvas')}"></span>` : ''}</div>
      ${this.tbtn('NewFile', 'Plus', 'Novo arquivo · Ctrl+N', 'tab-new', 14)}
      <div class="grow"></div>
      <button class="search wide" data-act="OpenPalette">${ICON('Search', 14)}<span>${t("Buscar tabelas, colunas, comandos…")}</span><span class="grow"></span><span class="kbd">Ctrl K</span></button>
      <button class="search narrow" data-act="OpenPalette" title="${t("Buscar tabelas, colunas, comandos · Ctrl+K")}">${ICON('Search', 15)}</button>
      <div class="tb-sep"></div>
      <div class="tb-views">
        <button class="${left !== 'dbml' ? 'on' : ''}" data-act="ShowDiagramView" title="${t("Diagrama · Ctrl+1")}">${ICON('Workflow', 14)}</button>
        <button class="${left === 'dbml' ? 'on' : ''}" data-act="ShowSplitView" title="${t("Dividido · DBML · Ctrl+2")}">${ICON('Columns2', 14)}</button>
      </div>
      <div class="tb-acts" data-tour="export-btn">
        ${this.tbtn('ExportDbml', 'FileDown', 'Exportar DBML')}
        ${this.tbtn('menu:export-code', 'DatabaseZap', 'Exportar código')}
        ${this.tbtn('menu:export-image', 'ImageDown', 'Exportar imagem')}
        ${this.tbtn('ToggleTheme', dark ? 'Moon' : 'Sun', 'Alternar tema · Ctrl+T')}
      </div>
      ${this.tbtn('menu:overflow', 'Ellipsis', 'Mais ações', 'tb-overflow')}
      <button class="ib ${insp ? 'on' : ''}" style="margin-left:2px" data-act="ToggleInspector" title="${t("Alternar inspector · Ctrl+I")}">${ICON('PanelRight', 16)}${hasSel && !insp ? '<span class="insp-dot"></span>' : ''}</button>`;
  }

  private errorCount(): number {
    return this.diags.filter((d) => d.severity === 'error').length;
  }

  private renderRails(): void {
    const errs = this.errorCount();
    const left = this.leftPane();
    const rb = (act: string, icon: IconName, tip: string, on = false, extra = '') =>
      `<button class="rbtn ${on ? 'on' : ''}" data-act="${act}" title="${esc(t(tip))}">${ICON(icon, 16)}${extra}</button>`;
    this.els['rail-left'].innerHTML = `
      ${rb('ShowFiles', 'Folder', 'Arquivos · Ctrl+Shift+E', left === 'files')}
      ${rb('ShowSchema', 'Table2', 'Tabelas · Ctrl+Shift+D', left === 'schema')}
      ${rb('ShowProblems', 'TriangleAlert', 'Problemas · Ctrl+Shift+M', left === 'problems', errs ? `<span class="rbadge">${errs}</span>` : '')}
      ${rb('ToggleEditor', 'Braces', 'Editor DBML · Ctrl+E', left === 'dbml')}
      ${rb('ToggleDiff', 'GitCompare', 'Comparar com o git HEAD · D', this.diff.status !== 'off')}
      ${rb('ConnectDatabase', 'Database', 'Conectar ao banco')}
      ${rb('ImportSqlDdl', 'FileInput', 'Importar SQL DDL')}
      <div class="grow"></div>
      ${rb('menu:appearance', 'Palette', 'Aparência')}
      ${rb('ShowShortcuts', 'Keyboard', 'Atalhos de teclado · Ctrl+/')}
      ${rb('OpenPalette', 'Command', 'Paleta de comandos · Ctrl+K')}`;
  }

  private renderIslands(): void {
    const tb = (act: string, icon: IconName, tip: string, cls = '') =>
      `<button class="tb ${cls}" data-act="${act}" title="${esc(t(tip))}">${ICON(icon, 16)}</button>`;
    this.els['tools'].innerHTML = `
      ${tb('ToolSelect', 'MousePointer2', 'Selecionar · V', this.tool === 'select' ? 'on' : '')}
      ${tb('ToolHand', 'Hand', 'Mão: mover a vista · H', this.tool === 'hand' ? 'on' : '')}
      <div class="sep"></div>
      <button class="tb" data-act="NewTable" data-tour="topbar-add" title="${esc(t('Nova tabela · T'))}">${ICON('SquarePlus', 16)}</button>
      ${tb('NewEnum', 'ListPlus', 'Novo enum · E')}
      ${tb('GroupSelection', 'Group', 'Agrupar seleção · Ctrl+G')}
      <div class="sep"></div>
      ${tb('Relayout', 'LayoutGrid', 'Auto-layout · Ctrl+L')}
      ${tb('ToggleFocusMode', 'Scan', 'Modo Foco (vizinhança FK) · F', this.focusMode && this.selection.length > 0 ? 'acc' : '')}
      ${tb('ToggleEditor', 'Braces', 'Editor DBML · Ctrl+E', this.editorOpen ? 'acc' : '')}`;
    const existingFocusBar = this.wb.querySelector('#focus-mode-bar');
    if (this.focusMode && this.selection.length > 0) {
      const barHtml = `<span style="font-size:11.5px;font-weight:600;color:var(--accent)">${t('Modo Foco')}</span>
        <button class="ib sm" style="width:auto;padding:0 7px;font-family:var(--mono);font-size:11px;color:var(--accent);background:color-mix(in srgb,var(--accent) 16%,transparent)" data-act="CycleFocusDepth" title="${t('Alternar 1-hop / 2-hop · Shift+F')}">${this.focusDepth}-hop</button>
        <button class="ib sm" style="width:auto;padding:0 6px;font-size:11px;color:var(--muted)" data-act="ToggleFocusMode" title="${t('Sair do Modo Foco · Esc')}">Esc</button>`;
      if (existingFocusBar) {
        existingFocusBar.innerHTML = barHtml;
      } else {
        const bar = document.createElement('div');
        bar.id = 'focus-mode-bar';
        bar.className = 'island';
        bar.style.cssText = 'position:absolute;top:12px;left:50%;transform:translateX(-50%);z-index:20;display:flex;align-items:center;gap:8px;padding:4px 10px';
        bar.innerHTML = barHtml;
        this.els['surface'].appendChild(bar);
      }
    } else if (existingFocusBar) {
      existingFocusBar.remove();
    }
    this.els['zoomi'].innerHTML = `<button class="zbtn" data-act="ZoomOut" title="${t("Diminuir zoom · Ctrl+-")}">${ICON('Minus', 13)}</button>
      <button class="pct" data-act="FitView" title="${t("Ajustar à tela · Shift+1")}">${Math.round(this.zoom * 100)}%</button>
      <button class="zbtn" data-act="ZoomIn" title="${t("Aumentar zoom · Ctrl+=")}">${ICON('Plus', 13)}</button>
      <span class="zsep"></span>
      <button class="zbtn" data-act="FitView" title="${t("Ajustar à tela · Shift+1")}">${ICON('Scan', 13)}</button>
      <span class="zsep"></span>
      <button class="zbtn ${this.prefs.minimap && this.schema.tables.length > 0 ? 'acc' : ''}" data-act="ToggleMinimap" title="${t("Minimapa · M")}">${ICON('Frame', 13)}</button>
      <button class="zbtn ${this.prefs.legend ? 'acc' : ''}" data-act="ToggleLegend" title="${t("Legenda · Ctrl+Shift+L")}">${ICON('Info', 13)}</button>`;
    this.renderMinimap();
  }

  private renderMinimap(): void {
    const el = this.els['minimap'];
    if (!el) return;
    const visible = this.prefs.minimap && this.schema.tables.length > 0 && !!this.scene;
    el.style.display = visible ? 'block' : 'none';
    const leg = this.els['legend'];
    if (leg) leg.style.bottom = visible ? '202px' : '56px';
    if (!visible || !this.scene) {
      this.minimapXform = null;
      this.minimapVpRect = null;
      return;
    }
    const g = getGlass(this.prefs.mode, 1, this.prefs.translucent);
    const dark = this.prefs.mode === 'dark';
    const r = this.els['surface'].getBoundingClientRect();
    const vw = Math.max(1, r.width || this.viewport?.x || 800);
    const vh = Math.max(1, r.height || this.viewport?.y || 600);
    const vpX = -this.pan.x / this.zoom;
    const vpY = -this.pan.y / this.zoom;
    const vpW = vw / this.zoom;
    const vpH = vh / this.zoom;
    let minX = vpX, minY = vpY, maxX = vpX + vpW, maxY = vpY + vpH;
    for (const tv of this.scene.tables) {
      const p = this.positions[tv.name];
      if (!p) continue;
      minX = Math.min(minX, p.x);
      minY = Math.min(minY, p.y);
      maxX = Math.max(maxX, p.x + CARD_W);
      maxY = Math.max(maxY, p.y + tv.h);
    }
    for (const ev of this.scene.enums || []) {
      const p = this.positions[`enum:${ev.name}`] || ev;
      minX = Math.min(minX, p.x);
      minY = Math.min(minY, p.y);
      maxX = Math.max(maxX, p.x + ev.w);
      maxY = Math.max(maxY, p.y + ev.h);
    }
    const margin = 40;
    const xf = computeMinimapTransform(minX - margin, minY - margin, maxX + margin, maxY + margin, MINIMAP_W, MINIMAP_H, MINIMAP_PAD);
    this.minimapXform = xf;
    const sel = new Set(this.selection);
    let svg = '';
    for (const gr of canvasGroups(this.scene.groups, this.schema, this.positions)) {
      const a = worldToMap(xf, gr.x, gr.y);
      const b = worldToMap(xf, gr.x + gr.w, gr.y + gr.h);
      svg += `<rect x="${a.x.toFixed(1)}" y="${a.y.toFixed(1)}" width="${Math.max(2, b.x - a.x).toFixed(1)}" height="${Math.max(2, b.y - a.y).toFixed(1)}" rx="3" fill="${hexWithAlpha(gr.color, dark ? 0.14 : 0.16)}" stroke="${hexWithAlpha(gr.color, 0.35)}" stroke-width="1"/>`;
    }
    for (const ev of this.scene.enums || []) {
      const p = this.positions[`enum:${ev.name}`] || ev;
      const a = worldToMap(xf, p.x, p.y);
      const b = worldToMap(xf, p.x + ev.w, p.y + ev.h);
      svg += `<rect x="${a.x.toFixed(1)}" y="${a.y.toFixed(1)}" width="${Math.max(3, b.x - a.x).toFixed(1)}" height="${Math.max(3, b.y - a.y).toFixed(1)}" rx="2" fill="${g.textFaint}" fill-opacity="0.45"/>`;
    }
    for (const tv of this.scene.tables) {
      const p = this.positions[tv.name];
      if (!p) continue;
      const a = worldToMap(xf, p.x, p.y);
      const b = worldToMap(xf, p.x + CARD_W, p.y + tv.h);
      const isSel = sel.has(tv.name);
      const fill = isSel ? g.accent : (tv.tint || g.textMuted);
      svg += `<rect x="${a.x.toFixed(1)}" y="${a.y.toFixed(1)}" width="${Math.max(3, b.x - a.x).toFixed(1)}" height="${Math.max(3, b.y - a.y).toFixed(1)}" rx="2" fill="${fill}" fill-opacity="${isSel ? 1 : 0.75}"/>`;
    }
    const va = worldToMap(xf, vpX, vpY);
    const vb = worldToMap(xf, vpX + vpW, vpY + vpH);
    const vr = { x: va.x, y: va.y, w: Math.max(8, vb.x - va.x), h: Math.max(6, vb.y - va.y) };
    this.minimapVpRect = vr;
    svg += `<rect x="${vr.x.toFixed(1)}" y="${vr.y.toFixed(1)}" width="${vr.w.toFixed(1)}" height="${vr.h.toFixed(1)}" rx="3" fill="${hexWithAlpha(g.accent, 0.14)}" stroke="${g.accent}" stroke-width="1.5"/>`;
    el.innerHTML = `<svg viewBox="0 0 ${MINIMAP_W} ${MINIMAP_H}" aria-hidden="true">${svg}</svg>`;
  }

  private panFromMinimap(clientX: number, clientY: number, grabX: number, grabY: number): void {
    const el = this.els['minimap'];
    if (!el || !this.minimapXform) return;
    const mr = el.getBoundingClientRect();
    const mx = ((clientX - mr.left) / Math.max(1, mr.width)) * MINIMAP_W - grabX;
    const my = ((clientY - mr.top) / Math.max(1, mr.height)) * MINIMAP_H - grabY;
    const sr = this.els['surface'].getBoundingClientRect();
    this.pan = panForMapPoint(this.minimapXform, mx, my, this.zoom, sr.width || 800, sr.height || 600);
    this.applyCamera();
  }

  private renderLegend(): void {
    const el = this.els['legend'];
    this.setShown(el, this.prefs.legend, 'block', null);
    if (!this.prefs.legend) return;
    const row = (vis: string, label: string) => `<div class="lr"><span class="lv">${vis}</span>${t(label)}</div>`;
    el.innerHTML = `<div class="lh">${t("LEGENDA")}<span class="grow"></span><button class="ib sm" data-act="ToggleLegend" title="${t("Fechar")}">${ICON('X', 13)}</button></div>
      ${row(`<span style="color:var(--accent);display:flex">${ICON('KeyRound', 12)}</span>`, 'Chave primária')}
      ${row(`<span style="color:var(--link);display:flex">${ICON('Link', 12)}</span>`, 'Chave estrangeira')}
      ${row('<span class="mono" style="font:600 9.5px var(--mono);color:var(--faint)">NN</span>', 'Obrigatório (NN) · Único (UQ)')}
      ${row('<span class="ln-s" style="height:1.5px;background:var(--rel-line)"></span>', 'Relação (coluna → coluna)')}
      ${row('<span class="mono" style="font:600 9.5px var(--mono);color:var(--muted)">N·1</span>', 'Cardinalidade: muitos · um')}
      ${row('<span class="ln-s" style="height:2.5px;background:var(--accent)"></span>', 'Tabela selecionada e relações')}`;
  }

  private hint(): string {
    const d = this.drag;
    if (d?.kind === 'cards') return translate('Movendo {}', this.selection.join(', '));
    if (d?.kind === 'group') return translate('Movendo grupo {}', d.name);
    if (d?.kind === 'link') return translate('Relação de {}.{}: solte na tabela de destino', d.table, d.col);
    if (d?.kind === 'marquee') return translate("Selecionando: solte para selecionar as tabelas tocadas");
    if (this.diff.status !== 'off') return translate('Comparando com o git HEAD · somente leitura');
    if (this.selectedEnum) return translate("Enum selecionado · edite os valores no inspetor · Delete exclui o enum · Esc limpa a seleção");
    if (this.selectedRef) return translate("Relação selecionada · Delete remove a linha e a coluna FK · Esc limpa a seleção");
    if (this.selection.length > 1) return translate('{} tabelas selecionadas · Ctrl+G agrupa', this.selection.length);
    if (this.selection.length === 1) return translate("Arraste para mover a tabela · Esc limpa a seleção");
    return translate("Clique em uma tabela para inspecionar · arraste o fundo para selecionar · role para mover · Ctrl+role para zoom");
  }

  private engineHtml(): string {
    if (!this.engineFailed) return '';
    return `<span class="stat" role="status" data-engine-failed style="color:var(--danger)">${ICON('CircleX', 12)}${esc(t('Motor de análise parado. Recarregue a página.'))}</span>`;
  }

  /** Só aparece fora do estado `online`: edições feitas sem canal com o servidor não são gravadas. */
  private connectionHtml(): string {
    if (this.connection === 'online') return '';
    const offline = this.connection === 'offline';
    const label = offline ? t('Sem conexão com o servidor. Edições não serão salvas.') : t('Reconectando ao servidor…');
    return `<span class="stat conn" role="status" data-conn="${this.connection}" style="color:var(--${offline ? 'danger' : 'warning'})">${ICON(offline ? 'CircleX' : 'TriangleAlert', 12)}${esc(label)}</span>`;
  }

  private renderStatus(): void {
    const errs = this.errorCount();
    const dark = this.prefs.mode === 'dark';
    const problems = this.diags.filter((d) => d.severity === 'error' || d.severity === 'warning').length;
    const probTxt = diagnosticLabels(this.diags).status;
    this.els['status'].innerHTML = `
      <span class="stat" data-act="ShowProblems" title="${t("Problemas")}" style="color:${errs ? 'var(--danger)' : problems ? 'var(--warning)' : 'var(--ok)'}">${ICON(problems ? 'CircleX' : 'CircleCheck', 12)}${probTxt}</span>
      <span class="cnts">${ICON('Table2', 12)}${t('{} tabelas · {} relações', this.schema.tables.length, this.schema.refs.length)}</span>
      ${this.connectionHtml()}${this.engineHtml()}
      <span class="hint">${esc(this.hint())}</span>
      <span>${Math.round(this.zoom * 100)}%</span><span>DBML</span><button type="button" class="ver-btn" data-act="ShowWhatsNew" title="${esc(t('Ver novidades desta versão'))}">v${APP_VERSION}</button>
      <span class="theme" data-act="ToggleTheme" title="${t("Alternar tema · Ctrl+T")}">${ICON(dark ? 'Moon' : 'Sun', 12)}${t(dark ? 'Escuro' : 'Claro')}</span>`;
  }

  private renderSidebar(): void {
    const left = this.leftPane();
    const el = this.els['sidebar'];
    const active = document.activeElement as HTMLElement | null;
    const focus = active?.dataset.act && el.contains(active) ? actSelector(active.dataset.act, active.dataset) : null;
    if (!left || left === 'dbml') return;
    if (left === 'files') {
      const folder = this.projectName().toUpperCase();
      const list = this.files.length
        ? this.files
            .map(
              (f) => `<button type="button" aria-current="${f.active}" class="trow frow ${f.active ? 'on' : ''}" data-act="open-file" data-path="${esc(f.path)}">
          <span style="color:${f.active ? 'var(--accent)' : 'var(--faint)'};display:flex">${ICON('FileCode', 14)}</span><span class="nm">${esc(f.name)}</span><span class="grow"></span><span class="cnt-mono">${f.active ? `${this.schema.tables.length} tab` : ''}</span></button>`
            )
            .join('')
        : `<div class="pempty">${t("Nenhum arquivo .dbml encontrado no workspace.")}</div>`;
      el.innerHTML = `<div class="phead">${t("Projeto")}<span class="grow"></span>
        <button class="ib sm" data-act="NewFile" title="${t("Novo arquivo · Ctrl+N")}">${ICON('FilePlus', 14)}</button>
        <button class="ib sm" data-act="ToggleSidebar" title="${t("Fechar painel · Ctrl+B")}">${ICON('ChevronsLeft', 14)}</button></div>
        <div class="pbody">
          <div class="psec">
            <div class="pshd"><span style="color:var(--faint);display:flex">${ICON('ChevronDown', 13)}</span><span>${esc(folder)}</span><span class="pill">${this.files.length}</span></div>
            ${list}
          </div>
          <div class="grow" style="border-top:1px solid var(--edge-soft)"></div>
        </div>`;
    } else if (left === 'schema') {
      const colp = (key: string) => this.collapsed.has(key);
      const q = this.schemaFilter.trim().toLowerCase();
      const match = (s: string) => !q || s.toLowerCase().includes(q);
      const sec = (key: string, label: string, n: number, body: string, forceRight = false) =>
        `<div class="psec"><button type="button" class="pshd" aria-expanded="${!colp(key) && !forceRight}" data-act="toggle-sec" data-sec="${key}"><span style="color:var(--faint);display:flex">${ICON(colp(key) || forceRight ? 'ChevronRight' : 'ChevronDown', 13)}</span><span>${t(label)}</span><span class="pill">${n}</span></button>${colp(key) || forceRight ? '' : body}</div>`;
      const row = (name: string, cnt: number, cls = ''): string => {
        const t = this.tint(name);
        const on = this.selection.includes(name);
        return `<button type="button" aria-pressed="${on}" class="trow ${cls} ${on ? 'on' : ''}" data-act="focus-table" data-table="${esc(name)}">
          <span style="color:${t || 'var(--accent)'};display:flex">${ICON('Table2', 14)}</span><span class="nm">${esc(name)}</span><span class="grow"></span><span class="cnt-mono">${cnt}</span></button>`;
      };
      const groups = this.scene?.groups || [];
      const grouped = new Set<string>();
      for (const gr of groups) for (const n of gr.tables) grouped.add(n);

      let groupsSec = '';
      if (groups.length) {
        let gbody = '';
        for (const gr of groups) {
          const gTables = gr.tables.filter((n) => match(n) || match(gr.name));
          if (q && !gTables.length) continue;
          gbody += `<button type="button" class="trow grp" aria-expanded="${!colp('g:' + gr.name)}" data-act="toggle-sec" data-sec="g:${esc(gr.name)}"><span style="color:${gr.color};display:flex">${ICON('Layers', 14)}</span><span class="nm">${esc(gr.name)}</span><span class="grow"></span><span class="cnt-mono">${gr.tables.length}</span></button>`;
          if (!colp('g:' + gr.name)) {
            for (const n of gTables) {
              const t = this.schema.tables.find((x) => qualifiedTableName(x) === n);
              if (t) gbody += row(n, t.columns.length, 'sub');
            }
          }
        }
        groupsSec = sec('groups', 'GRUPOS', groups.length, gbody);
      }

      const loose = this.schema.tables.filter((t) => !grouped.has(qualifiedTableName(t)) && match(qualifiedTableName(t)));
      const looseTotal = this.schema.tables.filter((t) => !grouped.has(qualifiedTableName(t))).length;
      const looseHtml = loose.map((t) => row(qualifiedTableName(t), t.columns.length)).join('');
      const tablesSec = sec('tables', groups.length ? 'SEM GRUPO' : 'TABELAS', looseTotal, looseHtml, looseTotal === 0);

      const enums = (this.scene?.enums || []).filter((ev) => match(ev.name));
      const enumsTotal = (this.scene?.enums || []).length;
      const enumsHtml = enums
        .map(
          (ev) => `<button type="button" aria-pressed="${this.selectedEnum === ev.name}" class="trow ${this.selectedEnum === ev.name ? 'on' : ''}" data-act="focus-enum" data-enum="${esc(ev.name)}">
          <span style="color:var(--accent);display:flex">${ICON('List', 14)}</span><span class="nm">${esc(ev.name)}</span><span class="grow"></span><span class="cnt-mono">${ev.values.length}</span></button>`
        )
        .join('');
      const enumsSec = sec('enums', 'ENUMS', enumsTotal, enumsHtml, enumsTotal === 0);

      const refsAutoCollapse =
        this.schema.refs.length > 3 && this.selection.length > 0 && !this.collapsed.has('refs:open');
      const refsCollapsed = colp('refs') || refsAutoCollapse;
      const refsHtml = this.schema.refs
        .filter((r) => match(qualifiedEndpointTable(r.from)) || match(qualifiedEndpointTable(r.to)))
        .map((r) => `<div class="trow rel"><span style="color:var(--faint);display:flex">${ICON('ArrowUpRight', 14)}</span><span class="nm">${esc(qualifiedEndpointTable(r.from))} → ${esc(qualifiedEndpointTable(r.to))}</span></div>`)
        .join('');
      const refsSec = `<div class="psec"><button type="button" class="pshd" aria-expanded="${!refsCollapsed}" data-act="toggle-sec" data-sec="refs"><span style="color:var(--faint);display:flex">${ICON(refsCollapsed ? 'ChevronRight' : 'ChevronDown', 13)}</span><span>${t("RELAÇÕES")}</span><span class="pill">${this.schema.refs.length}</span></button>${refsCollapsed ? '' : refsHtml}</div>`;

      const errs = this.errorCount();
      const totalCols = this.schema.tables.reduce((acc, t) => acc + t.columns.length, 0);
      const warnings = this.diags.filter((d) => d.severity === 'warning').length;
      const validTxt = diagnosticLabels(this.diags).footer;
      const filterBar = this.schemaFilterOpen
        ? `<div style="padding:0 10px 8px"><input class="fld" id="schema-filter" placeholder="${t('Filtrar esquema…')}" value="${esc(this.schemaFilter)}"></div>`
        : '';
      el.innerHTML = `<div class="phead">${t("Esquema")}<span class="grow"></span>
        <button class="ib sm ${this.schemaFilterOpen ? 'on' : ''}" data-act="FilterExplorer" title="${t("Filtrar esquema · Ctrl+Shift+F")}">${ICON('ListFilter', 14)}</button>
        <button class="ib sm" data-act="NewTable" title="${t("Nova tabela · T")}">${ICON('Plus', 14)}</button>
        <button class="ib sm" data-act="ToggleSidebar" title="${t("Fechar painel · Ctrl+B")}">${ICON('ChevronsLeft', 14)}</button></div>
        ${filterBar}
        <div class="pbody">
          ${groupsSec}${tablesSec}${enumsSec}${refsSec}
          <div class="grow" style="border-top:1px solid var(--edge-soft)"></div>
        </div>
        <button type="button" class="pfoot" data-act="ShowProblems" title="${t("Problemas")}">
          <span style="color:${errs ? 'var(--danger)' : warnings ? 'var(--warning)' : 'var(--ok)'};display:flex">${ICON(errs || warnings ? 'CircleX' : 'CircleCheck', 14)}</span>
          <span>${validTxt}</span><span class="grow"></span>
          <span class="cnt-mono">${this.schema.tables.length} · ${totalCols} · ${this.schema.refs.length}</span>
        </button>`;
      if (this.schemaFilterOpen) {
        const fi = el.querySelector('#schema-filter') as HTMLInputElement | null;
        fi?.addEventListener('input', () => {
          this.schemaFilter = fi.value;
          const pos = fi.selectionStart || 0;
          this.renderSidebar();
          const next = el.querySelector('#schema-filter') as HTMLInputElement | null;
          next?.focus();
          next?.setSelectionRange(pos, pos);
        });
      }
    } else if (left === DIFF_PANE_ID) {
      el.innerHTML = renderDiffPanel(this.diff);
    } else {
      const hasProb = this.errorCount() > 0 || this.diags.length > 0;
      const items = this.diags
        .map((d, i) => {
          const [ic, col] = d.severity === 'error' ? (['CircleX', 'var(--danger)'] as const)
            : d.severity === 'warning' ? (['TriangleAlert', 'var(--warning)'] as const)
            : (['Info', 'var(--faint)'] as const);
          return `<button type="button" class="prob" data-act="goto-diag" data-i="${i}"><span style="color:${col};padding-top:1px;display:flex">${ICON(ic as IconName, 14)}</span>
            <div style="min-width:0"><div class="msg">${esc(diagnosticMessage(d.message))}</div><div class="sub">${esc(this.title)} · ${t('linha')} ${d.range.start.line + 1}</div></div></button>`;
        })
        .join('');
      const pillStyle = hasProb
        ? 'background:color-mix(in srgb, var(--danger) 10%, transparent);color:var(--danger)'
        : '';
      el.innerHTML = `<div class="phead">${t("Problemas")}<span class="pill" style="${pillStyle}">${this.diags.length}</span><span class="grow"></span>
        <button class="ib sm" data-act="ToggleSidebar" title="${t("Fechar painel · Ctrl+B")}">${ICON('ChevronsLeft', 14)}</button></div>
        <div class="pbody">
          <div class="psec" style="padding:6px 0">
            <div class="pshd"><span style="color:var(--faint);display:flex">${ICON('ChevronDown', 13)}</span><span class="mono" style="letter-spacing:0;font-weight:500">${esc(this.title)}</span></div>
            ${items || `<div class="pempty"><span style="color:var(--ok);display:flex">${ICON('CircleCheck', 14)}</span>${t("Nenhum problema neste arquivo")}</div>`}
          </div>
          <div class="grow" style="border-top:1px solid var(--edge-soft)"></div>
        </div>`;
    }
    if (focus) el.querySelector<HTMLElement>(focus)?.focus({ preventScroll: true });
  }

  private renderEditor(): void {
    if (this.leftPane() !== 'dbml') return;
    const errs = this.errorCount();
    const lines = this.source.split('\n');
    let hl: [number, number] | null = null;
    if (this.selection.length === 1) {
      const t = this.schema.tables.find((x) => qualifiedTableName(x) === this.selection[0]);
      if (t) hl = tableLineRange(lines, t.schema, t.name);
    }
    const crumb =
      this.selection.length === 1 && hl
        ? `<span style="display:flex">${ICON('ChevronRight', 12)}</span><span style="color:var(--text)">${t('Tabela')} ${esc(this.selection[0])}</span>`
        : '';
    const state = errs ? t(errs === 1 ? '{} erro' : '{} erros', errs)
      : this.isDirty() ? t('Alterações não salvas') : t('sincronizado');
    this.els['ed-head'].innerHTML = `<span style="color:var(--accent);display:flex">${ICON('FileCode', 14)}</span><span>${esc(this.title)}</span>${crumb}<span class="grow"></span>
      <span class="state" style="color:${errs ? 'var(--danger)' : this.isDirty() ? 'var(--muted)' : 'var(--ok)'}">${ICON(errs ? 'CircleX' : 'RefreshCw', 12)}${state}</span>
      <button class="ib" data-act="FormatDocument" title="${translate("Formatar documento · Alt+Shift+F")}">${ICON('WandSparkles', 14)}</button>
      <button class="ib" data-act="ToggleEditor" title="${translate("Fechar editor · Ctrl+E")}">${ICON('X', 14)}</button>`;
    const key = [this.prefs.lang, this.source, this.selection.join('\u0001'), this.diags.map((d) => `${d.severity}${d.range.start.line}:${d.range.start.character}:${d.range.end.character}${d.message}`).join('\u0002')].join('\u0000');
    if (key === this.edKey) return;
    this.edKey = key;
    const tokHtml = (text: string) =>
      tokenizeLine(text).map((t) => `<span class="t-${t.kind}">${esc(t.text)}</span>`).join('');
    const errAt = new Map<number, Diagnostic>();
    for (const d of this.diags) if (d.severity === 'error' && !errAt.has(d.range.start.line)) errAt.set(d.range.start.line, d);
    this.els['ebd-in'].innerHTML = lines
      .map((line, i) => {
        const d = errAt.get(i);
        let code: string;
        if (d) {
          const s = Math.min(d.range.start.character, line.length);
          let e = d.range.end.line === i ? Math.min(d.range.end.character, line.length) : line.length;
          if (e <= s) e = s + (/^\w+/.exec(line.slice(s))?.[0].length || 1);
          code = `${tokHtml(line.slice(0, s))}<span class="wavy">${esc(line.slice(s, e))}</span>${tokHtml(line.slice(e))}`;
        } else {
          code = tokHtml(line);
        }
        const on = hl && i >= hl[0] && i <= hl[1];
        return `<div class="ln${on ? ' hl' : ''}${d ? ' err' : ''}"><span class="gut"><span class="num">${i + 1}</span><span class="mark">${d ? '<i></i>' : ''}</span></span><span class="code">${code}</span>${d ? `<span class="emsg">${esc(diagnosticMessage(d.message))}</span>` : ''}</div>`;
      })
      .join('');
    this.syncEditorScroll();
  }

  private syncEditorScroll(): void {
    const ta = this.els['code'] as HTMLTextAreaElement;
    this.els['ebd'].scrollTop = ta.scrollTop;
    this.els['ebd'].scrollLeft = ta.scrollLeft;
  }

  private renderInspector(): void {
    const el = this.els['inspector'];
    const sel = this.selection;
    const show = this.inspectorVisible();
    this.setShown(el, show, 'flex', 'Right');
    if (!show) return;
    // não recria o inspector enquanto o usuário digita num campo dele
    const ae = document.activeElement as HTMLElement | null;
    if (ae && el.contains(ae) && ae.tagName === 'INPUT' && !this.focusCol) return;
    const inspHead = `<div class="phead">${translate("Propriedades")}<span class="grow"></span><button class="ib sm" data-act="ToggleInspector" title="${translate("Ocultar inspector · Ctrl+I")}">${ICON('ChevronsRight', 14)}</button></div>`;
    const lab = (t: string, right = '') => `<div class="ilabel"><span>${translate(t)}</span>${right}</div>`;
    const cnt = (n: number) => `<span class="cnt-mono">${n}</span>`;
    const trash = (attrs: string, title: string, size = 13) =>
      `<button class="ib sm" ${attrs} title="${translate(title)}">${ICON('Trash', size)}</button>`;
    const g = getGlass(this.prefs.mode, 1, this.prefs.translucent);

    if (sel.length > 1) {
      const rows = sel
        .map((n) => {
          const grp = this.schema.groups.find((x) => x.tables.includes(n));
          return `<div class="mrow" data-act="focus-table" data-table="${esc(n)}"><span style="color:${this.tint(n) || 'var(--accent)'};display:flex">${ICON('Table2', 13)}</span><span>${esc(n)}</span><span class="g">${grp ? esc(grp.name) : translate('sem grupo')}</span></div>`;
        })
        .join('');
      el.innerHTML = `${inspHead}
        <div class="ident" style="gap:12px">
          <div class="multi-ic"><div class="back"></div><div class="front"><span style="color:var(--accent);display:flex">${ICON('Copy', 13)}</span></div></div>
          <div class="id-col"><div class="ititle">${translate('{} tabelas', sel.length)}</div><div class="isub">${translate("Shift+clique adiciona ou remove")}</div></div>
        </div>
        <div class="colsec" style="padding:4px 8px 12px">
          <div class="isec-hd"><span style="color:var(--faint);display:flex">${ICON('ChevronDown', 13)}</span><span>${translate("TABELAS")}</span><span class="pill">${sel.length}</span></div>
          ${rows}
        </div>
        <div class="isec" style="padding:14px;gap:6px">
          <button class="btn primary" style="height:32px;font-size:12.5px;justify-content:center;gap:8px;width:100%" data-act="GroupSelection">${ICON('Layers', 14)}${translate("Agrupar seleção")}<span class="mono" style="font-size:10.5px;opacity:.75">Ctrl G</span></button>
        </div>
        <div class="grow" style="border-top:1px solid var(--edge-soft)"></div>
        <div class="ifoot"><button class="btn danger" data-act="DeleteSelection">${ICON('Trash', 13)}${translate('Excluir {} tabelas', sel.length)}</button></div>`;
      return;
    }
    const t = sel.length === 1 ? this.schema.tables.find((x) => qualifiedTableName(x) === sel[0]) : undefined;
    if (!t && this.selectedEnum) {
      const ev = this.scene?.enums?.find((x) => x.name === this.selectedEnum);
      if (ev) {
        const valsHtml = ev.values
          .map(
            (v, idx) => `<div class="evrow">
            <input class="fld" style="flex:1" data-edit="enumval" data-enum="${esc(ev.name)}" data-orig="${esc(v)}" value="${esc(v)}">
            <button class="ib sm" data-act="enum-move" data-enum="${esc(ev.name)}" data-val="${esc(v)}" data-up="1" ${idx === 0 ? 'disabled' : ''} title="${translate("Mover para cima")}">↑</button>
            <button class="ib sm" data-act="enum-move" data-enum="${esc(ev.name)}" data-val="${esc(v)}" data-up="0" ${idx + 1 === ev.values.length ? 'disabled' : ''} title="${translate("Mover para baixo")}">↓</button>
            ${trash(`data-act="enum-delval" data-enum="${esc(ev.name)}" data-val="${esc(v)}"`, 'Excluir valor')}
          </div>`
          )
          .join('');
        const refsHtml = ev.referenced_by.length
          ? ev.referenced_by
              .map(
                (tb) => `<div class="mrow" data-act="focus-table" data-table="${esc(tb)}"><span style="color:var(--accent);display:flex">${ICON('Table2', 13)}</span>${esc(tb)}</div>`
              )
              .join('')
          : `<span class="isub">${translate("Nenhuma coluna referencia este enum")}</span>`;
        el.innerHTML = `${inspHead}
          <div class="ident">
            <div class="tile" style="background:${hexWithAlpha(g.accent, 0.16)}"><span style="color:var(--accent);display:flex">${ICON('List', 15)}</span></div>
            <div class="id-col">
              <input class="fld big" id="insp-name" aria-label="${translate('Nome do enum')}" data-edit="enum" data-enum="${esc(ev.name)}" data-orig="${esc(ev.name)}" value="${esc(ev.name)}">
              <span class="isub">${translate('Enum · {} valores · F2 para renomear', ev.values.length)}</span>
            </div>
          </div>
          <div class="isec">${lab('VALORES', `<button class="ib sm" data-act="enum-addval" data-enum="${esc(ev.name)}" title="${translate("Adicionar valor")}">${ICON('Plus', 13)}</button>`)}${valsHtml}</div>
          <div class="isec tight">${lab('REFERENCIADO POR', cnt(ev.referenced_by.length))}${refsHtml}</div>
          <div class="grow" style="border-top:1px solid var(--edge-soft)"></div>
          <div class="ifoot"><button class="btn danger" data-act="enum-del" data-enum="${esc(ev.name)}">${ICON('Trash', 13)}${translate('Excluir enum')}</button></div>`;
        return;
      }
    }
    if (!t && this.selectedRef) {
      const sr = this.selectedRef;
      const r = this.schema.refs.find(
        (x) =>
          (qualifiedEndpointTable(x.from) === sr.from_table &&
            x.from.column === sr.from_col &&
            qualifiedEndpointTable(x.to) === sr.to_table &&
            x.to.column === sr.to_col) ||
          (qualifiedEndpointTable(x.from) === sr.to_table &&
            x.from.column === sr.to_col &&
            qualifiedEndpointTable(x.to) === sr.from_table &&
            x.to.column === sr.from_col)
      );
      if (r) {
        const kinds: Array<[import('./types').RefKind, string]> = [
          ['ManyToOne', 'N:1 (>)'],
          ['OneToMany', '1:N (<)'],
          ['OneToOne', '1:1 (-)'],
          ['ManyToMany', 'N:N (<>)'],
        ];
        const actions: Array<[import('./types').RefAction | '', string]> = [
          ['', 'Nenhuma'],
          ['no_action', 'sem ação'],
          ['restrict', 'restringir'],
          ['cascade', 'cascata'],
          ['set_null', 'definir nulo'],
          ['set_default', 'definir padrão'],
        ];
        const cardBtns = kinds
          .map(([k, label]) => `<button class="chip ${r.kind === k ? 'on' : ''}" data-act="update-ref-kind" data-kind="${k}">${translate(label)}</button>`)
          .join('');
        const mkActionBtns = (act: 'update-ref-del' | 'update-ref-upd', cur: string) =>
          actions
            .map(([val, label]) => `<button class="chip ${cur === val ? 'on' : ''}" data-act="${act}" data-val="${val}">${translate(label)}</button>`)
            .join('');
        el.innerHTML = `${inspHead}
          <div class="isec tight">${lab('RELAÇÃO')}
          <div class="ititle mono" style="font-size:15px">${esc(qualifiedEndpointTable(r.from))}.${esc(r.from.column)}</div>
          <span class="isub mono">→ ${esc(qualifiedEndpointTable(r.to))}.${esc(r.to.column)}</span></div>
          <div class="isec">${lab('CARDINALIDADE')}<div class="btnrow">${cardBtns}</div></div>
          <div class="isec">${lab('AO EXCLUIR')}<div class="btnrow">${mkActionBtns('update-ref-del', r.on_delete || '')}</div></div>
          <div class="isec">${lab('AO ATUALIZAR')}<div class="btnrow">${mkActionBtns('update-ref-upd', r.on_update || '')}</div></div>
          <div class="grow" style="border-top:1px solid var(--edge-soft)"></div>
          <div class="ifoot"><button class="btn danger" data-act="del-selected-ref">${ICON('Trash', 13)}${translate("Excluir relação e coluna FK")}</button></div>`;
        return;
      }
    }
    if (!t) {
      const kb = (l: string, k: string) => `<div class="kbrow"><span>${translate(l)}</span><kbd>${k}</kbd></div>`;
      el.innerHTML = `${inspHead}<div class="isec">${lab('NADA SELECIONADO')}
        <div class="hint">${translate("Clique em uma tabela para ver colunas, cor, grupo e relações. Shift+clique seleciona várias.")}</div></div>
        <div class="isec tight">${kb('Selecionar', 'V')}${kb('Mover a vista', 'H')}${kb('Ajustar à tela', 'Shift+1')}${kb('Agrupar seleção', 'Ctrl+G')}${kb('Editor DBML', 'Ctrl+E')}${kb('Todos os comandos', 'Ctrl+K')}</div>`;
      return;
    }
    const tq = qualifiedTableName(t);
    if (this.openColTable !== tq) {
      this.openColTable = tq;
      this.openCol = t.columns[1]?.name ?? t.columns[0]?.name ?? null;
    } else if (this.openCol && !t.columns.some((c) => c.name === this.openCol)) {
      this.openCol = null;
    }
    const tint = this.tint(tq);
    const tileCol = tint || g.accent;
    const grp = this.schema.groups.find((x) => x.tables.includes(tq));
    const grpCol = grp ? this.scene?.groups.find((x) => x.name === grp.name)?.color || g.accent : g.accent;
    const refs = this.schema.refs.filter((r) => qualifiedEndpointTable(r.from) === tq || qualifiedEndpointTable(r.to) === tq);
    const cur = (t.color || '').toLowerCase();
    const sw =
      TABLE_COLORS.map(
        ([l, h]) =>
          `<button class="${cur === h ? 'on' : ''}" title="${translate(l)}" style="background:${h};--ring:${h}" data-act="set-color" data-table="${esc(tq)}" data-color="${h}"></button>`
      ).join('') +
      `<button class="none" title="${translate("Sem cor")}" data-act="set-color" data-table="${esc(tq)}" data-color=""></button>`;
    const fkSet = new Set(this.scene?.tables.find((x) => x.name === tq)?.fk_columns || []);
    const dt = (kind: string, c: string, orig: string, extra: string) =>
      `data-edit="${kind}" data-table="${esc(tq)}" data-col="${esc(c)}" data-orig="${esc(orig)}" value="${esc(orig)}" ${extra}`;
    const cols = t.columns
      .map((c) => {
        const isFk = !!c.fk || fkSet.has(c.name);
        const open = this.openCol === c.name;
        const icName: IconName = c.pk ? 'KeyRound' : isFk ? 'Link' : 'Dot';
        const icCol = c.pk ? 'var(--accent)' : isFk ? 'var(--link)' : 'var(--faint)';
        const marks = [c.not_null && 'NN', c.unique && 'UQ'].filter(Boolean).join(' ');
        const nameCell = open
          ? `<input class="fld bare nmf" aria-label="${translate('Nome da coluna')}" ${dt('colname', c.name, c.name, '')}>`
          : `<span class="cnm">${esc(c.name)}</span>`;
        const flags = (
          [
            ['PK', 'pk', c.pk],
            ['FK', 'fk', isFk],
            ['NN', 'not null', c.not_null],
            ['UQ', 'unique', c.unique],
          ] as Array<[string, string, boolean]>
        )
          .map(
            ([l, f, on]) =>
              `<button class="flag ${on ? 'on' : ''}" data-act="toggle-flag" data-table="${esc(tq)}" data-col="${esc(c.name)}" data-flag="${f}" data-on="${on ? 1 : 0}" aria-pressed="${on}" aria-label="${translate({ pk: 'Chave primária', fk: 'Chave estrangeira', 'not null': 'Obrigatório', unique: 'Único' }[f] || f)}">${l}</button>`
          )
          .join('');
        const body = open
          ? `<div class="col-body">
              <span class="clbl">${translate("Tipo")}</span>
              <div class="tywrap"><input class="fld tyf" aria-label="${translate('Tipo')}" ${dt('coltype', c.name, c.ty, '')}><span class="chev">${ICON('ChevronDown', 11)}</span></div>
              <span class="clbl">Flags</span>
              <div class="flags">${flags}</div>
              <span class="clbl">${translate("Padrão")}</span>
              <input class="fld" placeholder="${translate("nenhum")}" ${dt('coldefault', c.name, c.default_value || '', '')}>
              <span class="clbl">Check</span>
              <div style="display:flex;align-items:center;gap:4px">
                <input class="fld" placeholder="${translate("expressão…")}" ${dt('colcheck', c.name, c.check || '', '')}>
                <button class="ib sm" data-act="del-col" data-table="${esc(tq)}" data-col="${esc(c.name)}" title="${translate("Excluir coluna")}">${ICON('Trash', 13)}</button>
              </div>
              <span class="clbl">${translate("Nota")}</span>
              <input class="fld" placeholder="${translate("adicionar nota…")}" ${dt('colnote', c.name, c.note || '', '')}>
            </div>`
          : '';
        return `<div class="colcard ${open ? 'open' : ''}">
          <div class="col-top" ${open ? 'role="group"' : 'role="button" aria-expanded="false"'} tabindex="0" data-act="toggle-col" data-col="${esc(c.name)}">
            <span style="color:var(--faint);display:flex">${ICON('GripVertical', 12)}</span>
            <span style="width:14px;display:flex;justify-content:center;color:${icCol};flex:none">${ICON(icName, 12)}</span>
            ${nameCell}
            ${marks ? `<span class="cmarks">${marks}</span>` : ''}
            <span class="cty">${esc(c.ty)}</span>
          </div>
          ${body}
        </div>`;
      })
      .join('');
    const idxTypes: Array<[import('./types').IndexType | '', string]> = [
      ['', 'padrão'],
      ['b_tree', 'btree'],
      ['hash', 'hash'],
      ['gin', 'gin'],
      ['gist', 'gist'],
    ];
    const indexes = t.indexes || [];
    const indexesList = indexes
      .map((ix, idx) => {
        const activeCols = new Set((ix.columns || []).filter((ic) => ic.kind === 'column').map((ic) => ic.value));
        const colChips = t.columns
          .map(
            (c) =>
              `<button class="chip ${activeCols.has(c.name) ? 'on' : ''}" data-act="idx-toggle-col" data-table="${esc(tq)}" data-idx="${idx}" data-col="${esc(c.name)}">${esc(c.name)}</button>`
          )
          .join('');
        const typeBtns = idxTypes
          .map(
            ([val, label]) =>
              `<button class="chip ${(ix.index_type || '') === val ? 'on' : ''}" data-act="idx-set-type" data-table="${esc(tq)}" data-idx="${idx}" data-type="${val}">${translate(label)}</button>`
          )
          .join('');
        return `<div class="ixcard">
          <div class="ixln" style="flex-wrap:nowrap">
            <input class="fld" placeholder="${translate("nome do índice (opcional)")}" data-edit="idxname" data-table="${esc(tq)}" data-idx="${idx}" data-orig="${esc(ix.name || '')}" value="${esc(ix.name || '')}">
            ${trash(`data-act="del-index" data-table="${esc(tq)}" data-idx="${idx}"`, 'Excluir índice')}
          </div>
          <div class="ixln"><span class="lb">Cols</span>${colChips}</div>
          <div class="ixln"><span class="lb">${translate("Tipo")}</span>${typeBtns}</div>
          <div class="ixln"><span class="lb">${translate("Modo")}</span>
            <button class="chip ${ix.unique ? 'on' : ''}" data-act="idx-toggle-flag" data-table="${esc(tq)}" data-idx="${idx}" data-flag="unique" title="${translate("Índice único")}">${translate("único")}</button>
            <button class="chip ${ix.pk ? 'on' : ''}" data-act="idx-toggle-flag" data-table="${esc(tq)}" data-idx="${idx}" data-flag="pk" title="${translate("Chave primária composta")}">${translate("primário")}</button>
          </div>
        </div>`;
      })
      .join('');
    const rels = refs
      .map((r) => {
        const fq = qualifiedEndpointTable(r.from);
        const tqEnd = qualifiedEndpointTable(r.to);
        const out = fq === tq;
        const card = { ManyToOne: 'N:1', OneToMany: '1:N', OneToOne: '1:1', ManyToMany: 'N:N' }[r.kind];
        const aTxt = out ? r.from.column : fq;
        const bTxt = out ? `${tqEnd}.${r.to.column}` : r.to.column;
        return `<div class="irel">
          <span style="color:var(--accent);display:flex">${ICON(out ? 'ArrowUpRight' : 'ArrowDownLeft', 13)}</span>
          <span style="overflow:hidden;text-overflow:ellipsis">${esc(aTxt)}</span>
          <span style="color:var(--faint)">${card}</span>
          <span class="tx">${esc(bTxt)}</span>
          <button class="del-fk" data-act="del-ref" data-from-table="${esc(fq)}" data-from-col="${esc(r.from.column)}" data-to-table="${esc(tqEnd)}" data-to-col="${esc(r.to.column)}" title="${translate("Excluir relação e coluna FK")}">${translate('Excluir FK')}</button>
        </div>`;
      })
      .join('');
    const grpHtml = grp
      ? `<button class="grp-sel" data-act="menu:table-group" data-table="${esc(tq)}" title="${translate("Alterar ou remover grupo")}"><span style="color:${grpCol};display:flex">${ICON('Layers', 13)}</span><span class="nm">${esc(grp.name)}</span><span class="grow"></span><span style="color:var(--faint);display:flex">${ICON('ChevronsUpDown', 12)}</span></button>`
      : `<button class="grp-empty" data-act="menu:table-group" data-table="${esc(tq)}" title="${translate("Escolher grupo")}"><span>${translate("Sem grupo")}</span><span class="grow"></span><span class="mono" style="font-size:10.5px">Ctrl G</span></button>`;
    el.innerHTML = `${inspHead}
      <div class="ident">
        <div class="tile" style="background:${hexWithAlpha(tileCol, 0.16)}"><span style="color:${tileCol};display:flex">${ICON('Table2', 15)}</span></div>
        <div class="id-col">
          <input class="fld big" id="insp-name" aria-label="${translate('Nome da tabela')}" data-edit="table" data-table="${esc(tq)}" data-orig="${esc(tq)}" value="${esc(tq)}">
          <span class="isub">${translate('Tabela · {} · F2 para renomear', grp ? esc(grp.name) : translate('sem grupo'))}</span>
        </div>
      </div>
      <div class="isec">
        <div class="isec-hd"><span style="color:var(--faint);display:flex">${ICON('ChevronDown', 13)}</span><span>${translate("GERAL")}</span></div>
        <div class="gen-row"><span class="glbl">${translate("Grupo")}</span>${grpHtml}</div>
        <div class="gen-row"><span class="glbl">${translate("Cor")}</span><div class="sw">${sw}</div></div>
        <div class="gen-row"><span class="glbl">${translate("Nota")}</span><input class="fld" style="flex:1;min-width:0" placeholder="${translate("adicionar nota…")}" data-edit="tablenote" data-table="${esc(tq)}" data-orig="${esc(t.note || '')}" value="${esc(t.note || '')}"></div>
      </div>
      <div class="colsec">
        <div class="isec-hd"><span style="color:var(--faint);display:flex">${ICON('ChevronDown', 13)}</span><span>${translate("COLUNAS")}</span><span class="pill">${t.columns.length}</span><span class="grow"></span><button class="ib sm" data-act="add-col" data-table="${esc(tq)}" title="${translate("Adicionar coluna")}">${ICON('Plus', 14)}</button></div>
        ${cols}
        <div class="presets">${COLUMN_PRESET_ACTIONS.map((p) => `<button class="preset" data-act="${p.action}" title="${translate('Adicionar colunas {}', p.label)}">${ICON('Plus', 11)}${p.label}</button>`).join('')}</div>
      </div>
      <div class="isec tight">
        <div class="isec-hd"><span style="color:var(--faint);display:flex">${ICON('ChevronDown', 13)}</span><span>${translate("ÍNDICES")}</span><span class="pill">${indexes.length}</span><span class="grow"></span><button class="ib sm" data-act="add-index" data-table="${esc(tq)}" title="${translate("Adicionar índice")}">${ICON('Plus', 14)}</button></div>
        ${indexesList || `<span class="isub" style="font-size:12px;white-space:normal">${translate("Sem índices · a chave primária é indexada automaticamente")}</span>`}
      </div>
      <div class="isec tight">
        <div class="isec-hd"><span style="color:var(--faint);display:flex">${ICON('ChevronDown', 13)}</span><span>${translate("RELAÇÕES")}</span><span class="pill">${refs.length}</span></div>
        ${rels || `<span class="isub" style="font-size:12px;white-space:normal">${translate("Sem relações · arraste de uma coluna para outra tabela")}</span>`}
      </div>
      <div class="grow" style="border-top:1px solid var(--edge-soft)"></div>
      <div class="ifoot"><button class="btn danger" data-act="DeleteSelection">${ICON('Trash', 13)}${translate("Excluir tabela")}<span class="mono" style="font-size:10.5px;color:var(--faint);margin-left:4px">Del</span></button></div>`;
    if (this.focusCol) {
      const [ft, fc] = this.focusCol;
      this.focusCol = null;
      const inp = el.querySelector(`[data-edit="colname"][data-table="${CSS.escape(ft)}"][data-col="${CSS.escape(fc)}"]`) as HTMLInputElement | null;
      inp?.focus();
      inp?.select();
    }
  }

  // ───────────────────────── editor DBML ─────────────────────────

  private syncEditorValue(): void {
    const ta = this.els['code'] as HTMLTextAreaElement;
    if (ta.value !== this.source) {
      const s = ta.selectionStart, e = ta.selectionEnd, top = ta.scrollTop;
      ta.value = this.source;
      ta.setSelectionRange(Math.min(s, this.source.length), Math.min(e, this.source.length));
      ta.scrollTop = top;
    }
  }

  private offsetOf(line: number, ch: number): number {
    const lines = this.source.split('\n');
    let off = 0;
    for (let i = 0; i < line && i < lines.length; i++) off += lines[i].length + 1;
    return off + ch;
  }

  private moveLine(up: boolean): void {
    const ta = this.els['code'] as HTMLTextAreaElement;
    const line = ta.value.slice(0, ta.selectionStart).split('\n').length - 1;
    try {
      const patch: TextPatch = this.wasm!.swap_dbml_lines(this.source, line, up);
      if (patch.updated_source && patch.updated_source !== this.source) {
        this.pushHistory();
        this.applyPatch(patch);
        ta.value = this.source;
        const nl = up ? line - 1 : line + 1;
        const p = this.offsetOf(nl, 0);
        ta.setSelectionRange(p, p);
        this.scheduleRender();
      }
    } catch (e) {
      console.error(e);
    }
  }

  // ───────────────────────── ações ─────────────────────────

  /** Aplica um preset de colunas à tabela selecionada (um passo de undo; sem tabela ou sem novidade avisa). */
  private applyColumnPreset(id: ActionId): void {
    const entry = COLUMN_PRESET_ACTIONS.find((p) => p.action === id);
    const table = this.selection.length === 1 ? this.selection[0] : null;
    if (!entry || !table) {
      this.toast(translate("Selecione uma tabela para adicionar a predefinição de coluna"));
      return;
    }
    const patch = this.dispatch({ action: 'add_column_preset', payload: { table, preset: entry.preset } });
    if (patch && !patch.new_text && patch.byte_start === patch.byte_end) {
      this.toast(translate('{}: as colunas já existem', entry.label));
    }
  }

  private runAction(id: ActionId): void {
    this.fxStart(`[data-act="${id}"]`, 'fx-press');
    switch (id) {
      case 'NewFile': this.transport.createNewFile?.(); return;
      case 'SaveFile':
        void this.saveDocument().then(async (saved) => {
          if (!saved) return;
          try {
            await this.saveLayout();
            this.toast(translate('Salvo {}', this.title));
          } catch (error) {
            this.layoutFailed(error, true);
          }
        });
        return;
      case 'Undo': this.undo(); return;
      case 'Redo': this.redo(); return;
      case 'ShowFiles': this.showPane('files'); return;
      case 'ShowSchema': this.showPane('schema'); return;
      case 'ShowProblems': this.showPane('problems'); return;
      case 'ThemeLight': case 'ThemeDark':
        this.prefs.mode = id === 'ThemeLight' ? 'light' : 'dark';
        break;
      case 'LanguagePortuguese': case 'LanguageEnglish':
        this.prefs.lang = id === 'LanguagePortuguese' ? 'pt-BR' : 'en';
        setLanguage(this.prefs.lang);
        break;
      case 'ToggleTheme': this.prefs.mode = this.prefs.mode === 'dark' ? 'light' : 'dark'; break;
      case 'ShowDiagramView':
        if (this.editorOpen) { this.editorOpen = false; this.saveDocSession(); }
        break;
      case 'ShowSplitView':
        if (!this.editorOpen) { this.openEditor(); this.saveDocSession(); setTimeout(() => this.els['code'].focus(), 0); }
        break;
      case 'FilterExplorer':
        if (this.leftPane() !== 'schema') { this.showPane('schema'); this.schemaFilterOpen = true; }
        else this.schemaFilterOpen = !this.schemaFilterOpen;
        if (!this.schemaFilterOpen) this.schemaFilter = '';
        this.renderSidebar();
        if (this.schemaFilterOpen) (this.els['sidebar'].querySelector('#schema-filter') as HTMLInputElement | null)?.focus();
        return;
      case 'ToggleEditor':
        if (this.editorOpen) this.editorOpen = false;
        else {
          this.openEditor();
          setTimeout(() => this.els['code'].focus(), 0);
        }
        this.saveDocSession();
        break;
      case 'ToggleSidebar':
        if (this.leftPane()) {
          this.prefs.sidebar = false;
          this.editorOpen = false;
        } else this.prefs.sidebar = true;
        break;
      case 'ExportDbml':
        void this.transport.exportFile?.(this.title || 'schema.dbml', this.source, false);
        return;
      case 'ToggleInspector': this.prefs.inspector = !this.prefs.inspector; break;
      case 'ToggleLegend': this.prefs.legend = !this.prefs.legend; break;
      case 'ToggleMinimap': this.prefs.minimap = !this.prefs.minimap; break;
      case 'FitView': this.glide(() => this.fit()); return;
      case 'Relayout': this.relayout(true); return;
      case 'ZoomIn': case 'ZoomOut': {
        const r = this.els['surface'].getBoundingClientRect();
        this.glide(() => this.zoomAt(r.width / 2, r.height / 2, id === 'ZoomIn' ? 1.2 : 1 / 1.2));
        return;
      }
      case 'ToolSelect': this.tool = 'select'; break;
      case 'ToolHand': this.tool = 'hand'; break;
      case 'ToggleEdgeStraight': case 'ResetEdgeCurve': {
        if (!this.selectedRef) return;
        const key = edgeKey(this.selectedRef);
        if (id === 'ResetEdgeCurve' && !this.edges[key]) return;
        this.pushHistory();
        if (id === 'ResetEdgeCurve') delete this.edges[key];
        else if (this.edges[key]?.straight) {
          const { straight: _, ...curve } = this.edges[key];
          if (Object.keys(curve).length) this.edges[key] = curve;
          else delete this.edges[key];
        } else this.edges[key] = { ...this.edges[key], straight: true };
        this.queueSaveLayout();
        this.scheduleRender();
        return;
      }
      case 'Deselect':
        if (this.diff.status !== 'off' && this.selection.length === 0 && !this.selectedRef && !this.selectedEnum) {
          this.exitDiff();
          return;
        }
        this.selection = []; this.selectedRef = null; this.selectedEnum = null; this.focusMode = false; this.highlightedColumn = null; break;
      case 'ToggleFocusMode':
        if (this.focusMode) this.focusMode = false;
        else if (this.selection.length > 0) this.focusMode = true;
        break;
      case 'ToggleDiff': this.toggleDiff(); return;
      case 'CycleFocusDepth':
        this.focusDepth = this.focusDepth === 1 ? 2 : 1;
        if (this.selection.length > 0) this.focusMode = true;
        break;
      case 'GroupSelection':
        if (this.selection.length) this.dispatch({ action: 'group_tables', payload: { tables: [...this.selection] } });
        return;
      case 'UngroupSelection':
        if (this.selection.length) this.dispatch({ action: 'ungroup_tables', payload: { tables: [...this.selection] } });
        return;
      case 'NewTable': this.dispatch({ action: 'add_table', payload: {} }); return;
      case 'NewEnum': this.dispatch({ action: 'add_enum', payload: {} }); return;
      case 'DeleteSelection':
        if (this.selectedEnum) {
          this.dispatch({ action: 'delete_enum', payload: { name: this.selectedEnum } });
          this.selectedEnum = null;
        } else if (this.selectedRef) {
          this.dispatch({ action: 'delete_ref', payload: { ...this.selectedRef } });
          this.selectedRef = null;
        } else if (this.selection.length) {
          this.dispatch({ action: 'delete_tables', payload: { names: [...this.selection] } });
          this.selection = [];
        }
        break;
      case 'RenameSelected': {
        if (this.selection.length !== 1 && !this.selectedEnum) return;
        this.prefs.inspector = true;
        this.renderChrome();
        const i = this.els['inspector'].querySelector('#insp-name') as HTMLInputElement | null;
        i?.focus(); i?.select();
        return;
      }
      case 'FormatDocument': {
        const f = this.wasm!.format_dbml(this.source);
        if (f !== this.source) {
          this.pushHistory();
          this.applyPatch(this.fullReplace(f));
        }
        break;
      }
      case 'MoveLineUp': this.moveLine(true); return;
      case 'MoveLineDown': this.moveLine(false); return;
      case 'ZoomInUi': this.stepUi(1); break;
      case 'ZoomOutUi': this.stepUi(-1); break;
      case 'ResetUiZoom': this.prefs.uiScale = 1; break;
      case 'ToggleAutoCollapseInspector': this.prefs.autoHide = !this.prefs.autoHide; break;
      case 'OpenPalette': this.openPalette(); return;
      case 'QuickSearch': this.openQuickSearch(); return;
      case 'ShowShortcuts': this.openShortcuts(); return;
      case 'ShowTour': this.openTour(); return;
      case 'ShowWhatsNew': this.openWhatsNew(); return;
      case 'ExportSvg': this.doExport('svg'); return;
      case 'ExportPng': this.doExport('png'); return;
      case 'ExportJpeg': this.doExport('jpeg'); return;
      case 'ExportMermaid': this.doExport('mermaid'); return;
      case 'ExportDocsHtml': this.doExportDocs('html'); return;
      case 'ExportDocsMarkdown': this.doExportDocs('md'); return;
      case 'ExportSqlPostgres': this.doExportSql('postgres'); return;
      case 'ExportSqlMysql': this.doExportSql('mysql'); return;
      case 'ExportSqlSqlite': this.doExportSql('sqlite'); return;
      case 'ExportSqlSqlServer': this.doExportSql('sqlserver'); return;
      case 'ExportSqlOracle': this.doExportSql('oracle'); return;
      case 'ImportSqlDdl': this.openImportSqlModal(); return;
      case 'ConnectDatabase': this.openConnectDbModal(); return;
      case 'AddPresetUuidPk': case 'AddPresetBigIntPk': case 'AddPresetTimestamps': case 'AddPresetSoftDelete':
        this.applyColumnPreset(id);
        return;
    }
    this.savePrefs();
    this.scheduleRender();
  }

  private stepUi(d: number): void {
    let i = UI_STEPS.findIndex((s) => Math.abs(s - this.prefs.uiScale) < 0.05);
    if (i < 0) i = 0;
    this.prefs.uiScale = UI_STEPS[Math.min(Math.max(i + d, 0), UI_STEPS.length - 1)];
  }

  private showPane(p: Pane): void {
    if (this.leftPane() === p) {
      // clicar no painel ativo recolhe
      this.prefs.sidebar = false;
    } else {
      this.pane = p;
      this.prefs.sidebar = true;
      this.editorOpen = false;
    }
    this.savePrefs();
    this.saveDocSession();
    this.renderChrome();
  }

  private async doExport(kind: 'svg' | 'png' | 'jpeg' | 'mermaid'): Promise<void> {
    if (!this.transport.exportFile) return;
    let rasterizing = false;
    try {
      if (kind === 'mermaid') {
        await this.transport.exportFile('diagram.mmd', exportToMermaid(this.schema), false);
        return;
      }
      await document.fonts.ready;
      const svg = exportToSvg(this.schema, this.positions, this.scene, this.prefs.mode, this.edges, {
        zoom: this.zoom,
        selection: this.selection,
        selectedRef: this.selectedRef ? edgeKey(this.selectedRef) : undefined,
        selectedEnum: this.selectedEnum,
        top: this.top,
      });
      if (kind === 'svg') {
        await this.transport.exportFile('diagram.svg', svg, false);
        return;
      }
      rasterizing = true;
      const blob = await exportSvgStringToRasterBlob(svg, kind === 'png' ? 'image/png' : 'image/jpeg');
      const b64: string = await new Promise((res, rej) => {
        const fr = new FileReader();
        fr.onload = () => res(String(fr.result).split(',')[1] || '');
        fr.onerror = rej;
        fr.readAsDataURL(blob);
      });
      rasterizing = false;
      await this.transport.exportFile(kind === 'png' ? 'diagram.png' : 'diagram.jpg', b64, true);
    } catch (e) {
      console.error('Diagram export failed:', e);
      this.toast(t(rasterizing
        ? 'Não foi possível gerar {}. Tente exportar em SVG.'
        : 'Não foi possível exportar {}. Tente outro nome ou outra pasta.', kind.toUpperCase()));
    }
  }

  private async doExportDocs(format: 'html' | 'md'): Promise<void> {
    if (!this.transport.exportFile || !this.wasm) return;
    const stem = this.title.replace(/\.dbml$/i, '') || 'schema';
    try {
      if (format === 'md') {
        if (!this.wasm.export_docs_markdown) return;
        const md = this.wasm.export_docs_markdown(this.source, stem);
        await this.transport.exportFile(`${stem}.md`, md, false);
        return;
      }
      if (!this.wasm.export_docs_html) return;
      await document.fonts.ready;
      const svg = exportToSvg(this.schema, this.positions, this.scene, this.prefs.mode, this.edges, {
        zoom: this.zoom,
      });
      const html = this.wasm.export_docs_html(this.source, stem, svg);
      await this.transport.exportFile(`${stem}.html`, html, false);
    } catch (e) {
      console.error('Docs export failed:', e);
      this.toast(t('Não foi possível exportar {}. Tente outro nome ou outra pasta.', format.toUpperCase()));
    }
  }

  private async doExportSql(
    dialect: 'postgres' | 'mysql' | 'sqlite' | 'sqlserver' | 'oracle'
  ): Promise<void> {
    if (!this.transport.exportFile || !this.wasm?.export_sql_ddl) return;
    try {
      const sql = this.wasm.export_sql_ddl(this.source, dialect) as string;
      await this.transport.exportFile(`schema.${dialect}.sql`, sql, false);
    } catch (e) {
      console.error('SQL export failed:', e);
      const message = e instanceof Error ? e.message : String(e);
      const unsupported = /^SQLite export does not support schema `(.+)`$/.exec(message);
      if (message.startsWith('invalid DBML: ')) {
        this.toast(t('Revise o DBML antes de exportar SQL: {}', diagnosticMessage(message.slice('invalid DBML: '.length))));
      } else if (unsupported) {
        this.toast(t('O SQLite não aceita o schema {} nesta exportação. Escolha outro dialeto.', unsupported[1]));
      } else {
        this.toast(t('Não foi possível exportar {}. Tente outro nome ou outra pasta.', 'SQL'));
      }
    }
  }

  private toast(msg: string): void {
    const t = this.els['toast'];
    t.textContent = msg;
    t.classList.remove('out');
    t.style.display = 'block';
    window.clearTimeout(this.toastTimer);
    this.toastTimer = window.setTimeout(() => {
      t.classList.add('out');
      this.toastTimer = window.setTimeout(() => (t.style.display = 'none'), noMotion() ? 0 : MOTION.base);
    }, 3200);
  }

  // ───────────────────────── menus / paleta / atalhos ─────────────────────────

  private closeMenus(instant = false): void {
    this.wb.querySelectorAll(instant ? '.menu, .menus, .suggest' : '.menu:not(.out), .menus:not(.out), .suggest').forEach((n) => this.fadeOut(n, instant));
  }

  /** Remove uma sobreposição com saída animada (inerte enquanto some). */
  private fadeOut(n: Element, instant = false): void {
    if (instant || noMotion() || n.classList.contains('suggest')) { n.remove(); return; }
    n.classList.add('out');
    window.setTimeout(() => n.remove(), MOTION.fast);
  }

  private openMenu(anchor: HTMLElement, html: string, width: number, className = ''): void {
    this.closeMenus();
    const m = document.createElement('div');
    m.className = className ? `menu ${className}` : 'menu';
    m.style.width = width + 'px';
    m.innerHTML = html;
    this.wb.appendChild(m);
    const r = anchor.getBoundingClientRect();
    m.style.top = Math.min(r.bottom + 6, window.innerHeight - m.offsetHeight - 8) + 'px';
    m.style.left = Math.max(8, Math.min(r.right - width, window.innerWidth - width - 8)) + 'px';
  }

  private keysOf(id: ActionId): string {
    return catalog().find((e) => e.id === id)?.displayKeys.join(' ') || '';
  }

  private miHtml(id: ActionId, label: string): string {
    return `<button class="mi" data-act="${id}">${ICON(ACTION_ICON[id] || 'Command', 14)}${esc(label)}<span class="k">${esc(this.keysOf(id))}</span></button>`;
  }

  /** Menu principal: lista de seções à esquerda e submenu da seção ativa ao lado. */
  private mainMenu(): void {
    this.closeMenus();
    const labels = new Map(catalog().map((e) => [e.id, e.label]));
    const root = MAIN_MENU.map(
      ([name, icon]) =>
        `<button class="mi ${name === this.menuSub ? 'on' : ''}" data-act="menu-sub" data-sub="${name}">${ICON(icon, 14)}${t(name)}<span class="k">${ICON('ChevronRight', 13)}</span></button>`
    ).join('');
    const items = (MAIN_MENU.find((m) => m[0] === this.menuSub) || MAIN_MENU[0])[2]
      .map((id) => this.miHtml(id, labels.get(id) || id))
      .join('');
    const m = document.createElement('div');
    m.className = 'menus';
    m.innerHTML = `<div class="menu" style="width:220px">${root}<hr>
        <button class="mi" data-act="OpenPalette">${ICON('Command', 14)}${t('Paleta de comandos')}<span class="k">Ctrl K</span></button>
        <button class="mi" data-act="ShowShortcuts">${ICON('Keyboard', 14)}${t('Atalhos de teclado')}<span class="k">Ctrl /</span></button>
        <button class="mi" data-act="ShowTour">${ICON('Info', 14)}${t('Tour guiado')}</button>
        <button class="mi" data-act="ShowWhatsNew">${ICON('WandSparkles', 14)}${t('O que há de novo')}</button></div>
      <div class="menu" style="width:280px">${items}</div>`;
    this.wb.appendChild(m);
  }

  private appearanceMenu(): void {
    // reabrir após um clique dentro dele: troca no lugar, sem sair e entrar de novo
    const reopen = !!this.wb.querySelector('.menu.app');
    this.closeMenus(reopen);
    const p = this.prefs;
    const sc = (v: number) => Math.abs(p.uiScale - v) < 0.05;
    const tgl = (act: string, on: boolean, name: string, desc: string) =>
      `<button class="tgl ${on ? 'on' : ''}" data-act="${act}" data-keep="1"><span class="tx"><span>${name}</span><small>${desc}</small></span><span class="sw2"><i></i></span></button>`;
    const m = document.createElement('div');
    m.className = 'menu app ui' + (reopen ? ' noanim' : '');
    m.innerHTML = `
      <div class="blk"><div class="ilabel">${t('TEMA')}<span class="cnt-mono">Ctrl T</span></div>
        <div class="seg" style="grid-template-columns:1fr 1fr">
          ${([['light', 'Claro'], ['dark', 'Escuro']] as const).map(([mode, label]) => `<button class="${p.mode === mode ? 'on' : ''}" data-act="set-mode" data-mode="${mode}" data-keep="1">${t(label)}</button>`).join('')}</div>
</div>
      <div class="blk"><div class="ilabel">${t('IDIOMA')}</div>
        <div class="seg" style="grid-template-columns:1fr 1fr">
          <button class="${p.lang === 'pt-BR' ? 'on' : ''}" data-act="set-lang" data-lang="pt-BR" data-keep="1">Português</button>
          <button class="${p.lang === 'en' ? 'on' : ''}" data-act="set-lang" data-lang="en" data-keep="1">English</button></div></div>
      <div class="blk"><div class="ilabel">${t('ESCALA DA UI')}<span class="cnt-mono">Ctrl Shift = / -</span></div>
        <div class="seg" style="grid-template-columns:1fr 1fr 1fr">${UI_STEPS.map((s) => `<button class="${sc(s) ? 'on' : ''}" data-act="set-ui" data-ui="${s}" data-keep="1">${Math.round(s * 100)}%</button>`).join('')}</div></div>
      <div class="blk"><div class="ilabel" style="padding:0 8px 4px">${t('PAINÉIS')}</div>
        ${tgl('ToggleAutoCollapseInspector', p.autoHide, t('Recolher inspetor automaticamente'), t('Oculta quando nada está selecionado'))}</div>`;
    this.wb.appendChild(m);
  }

  /** Popover sob o botão: exportar código (linhas de 32px) ou imagem (44px com descrição). */
  private exportMenu(anchor: HTMLElement, kind: 'code' | 'image'): void {
    const code = kind === 'code';
    const rows = code
      ? EXPORT_CODE_ROWS.map(
          ([id, n, ext]) => `<button class="mi" data-act="${id}">${ICON(id === 'ExportMermaid' ? 'GitFork' : 'Database', 14)}${n}<span class="ext">${ext}</span></button>`
        )
      : EXPORT_IMAGE_ROWS.map(
          ([id, n, d, ext]) => `<button class="mi image-option" data-act="${id}"><span class="image-icon">${ICON(ACTION_ICON[id] || 'Image', 16)}</span><span class="two"><span>${translate(n)}</span><small>${translate(d)}</small></span><span class="ext">${ext}</span></button>`
        );
    this.openMenu(anchor, `<div class="lb">${t(code ? GROUP_EXPORT_CODE : GROUP_EXPORT_IMAGE).toUpperCase()}</div>${rows.join('')}`, Math.min(290, window.innerWidth - 16), code ? '' : 'image-menu');
  }

  private tableGroupMenu(anchor: HTMLElement, table: string): void {
    const rows = this.schema.groups
      .map((gr) => {
        const col = this.scene?.groups.find((x) => x.name === gr.name)?.color || 'var(--accent)';
        return `<button class="mi" data-act="assign-group" data-table="${esc(table)}" data-group="${esc(gr.name)}"><span style="color:${col};display:flex">${ICON('Layers', 14)}</span>${esc(gr.name)}</button>`;
      })
      .join('');
    this.openMenu(
      anchor,
      `${rows}${rows ? '<hr>' : ''}<button class="mi" data-act="GroupSelection">${ICON('Plus', 14)}${translate("Criar grupo")}<span class="k">Ctrl G</span></button><button class="mi" data-act="UngroupSelection">${ICON('X', 14)}${translate("Sem grupo")}<span class="k">Ctrl Shift G</span></button>`,
      Math.max(anchor.offsetWidth, 180)
    );
  }

  private modal(inner: string, cls = '', bgCls = ''): HTMLElement {
    this.closeModal();
    const bg = document.createElement('div');
    bg.className = 'modal-bg ' + bgCls;
    bg.dataset.modal = '1';
    bg.innerHTML = `<div class="modal ${cls}">${inner}</div>`;
    bg.addEventListener('mousedown', (e) => {
      if (e.target === bg) this.closeModal();
    });
    this.wb.appendChild(bg);
    return bg;
  }

  private closeModal(): void {
    this.wb.querySelectorAll('[data-modal]:not(.out)').forEach((n) => this.fadeOut(n));
    this.paletteOpen = false;
    this.quickSearchOpen = false;
    this.shortcutsOpen = false;
    this.whatsNewOpen = false;
  }

  private importConflicts(dbml: string): string[] {
    return dbml ? this.wasm?.import_conflicts?.(this.source, dbml) ?? [] : [];
  }

  private openConnectDbModal(): void {
    if (!this.transport.introspect) {
      this.toast(translate("Conectar ao banco precisa do servidor `dbml run` ou da CLI dbml no PATH."));
      return;
    }
    const introspect = this.transport.introspect.bind(this.transport);
    const model = new ConnectDbModel(this.connectEngine);
    const field = (id: string, label: string, ph: string, type = 'text') =>
      `<label data-row="${id}" for="db-${id}">${translate(label)}</label>
       <input data-row="${id}" id="db-${id}" type="${type}" placeholder="${esc(translate(ph))}" autocomplete="off" spellcheck="false">`;

    const bg = this.modal(`
      <div role="dialog" aria-modal="true" aria-labelledby="db-title" style="display:flex;flex-direction:column;min-height:0;overflow:auto">
        <div class="mhd"><div><div class="tt" id="db-title">${translate("Conectar ao banco")}</div><div class="st">${translate("Faça engenharia reversa de um esquema existente para DBML.")}</div></div><span class="grow"></span>
          <button class="ib" id="db-x" type="button" title="${t("Fechar")}">${ICON('X', 15)}</button></div>
        <div style="padding:14px 18px 6px"><div class="seg row acc" id="db-engines">
          ${DB_ENGINES.map((e) => `<button type="button" data-engine="${e.value}">${e.label}</button>`).join('')}
          <button type="button" disabled title="${translate("Suporte a Oracle em breve")}">${translate("Oracle (em breve)")}</button>
        </div></div>
        <div class="mform" id="db-form">
          ${field('host', 'Host', 'localhost')}
          ${field('port', 'Porta', 'porta padrão')}
          ${field('database', 'Banco', 'nome do banco')}
          ${field('user', 'Usuário', 'usuário')}
          ${field('password', 'Senha', 'senha', 'password')}
          ${field('schemas', 'Schemas', 'todos os schemas (ou: public, sales)')}
          <label data-row="${TLS_ROW_ID}" for="${TLS_SELECT_ID}">${TLS_FIELD_LABEL}</label>
          <select data-row="${TLS_ROW_ID}" id="${TLS_SELECT_ID}" class="fld">
            ${TLS_SELECT_OPTIONS.map((opt) => `<option value="${opt.value}">${translate(opt.labelKey)}</option>`).join('')}
          </select>
          ${field('path', 'Arquivo', '/caminho/para/banco.sqlite')}
          <label class="chk" data-row="trust"><input id="db-trust" type="checkbox">${translate("Confiar no certificado do servidor (autoassinado, ex.: Docker)")}</label>
        </div>
        <div class="mnote" id="db-note"><span id="db-ico" style="color:var(--ok);display:flex">${ICON('ShieldCheck', 14)}</span><span id="db-status" aria-live="polite"></span></div>
        <div class="mwarn" id="db-warns" style="display:none"></div>
        <div class="mfooter">
          <button class="btn" id="db-test" type="button">${ICON('PlugZap', 13)}${translate("Testar conexão")}</button><span class="grow"></span>
          <button class="btn ghost" id="db-cancel" type="button">${translate("Cancelar")}</button>
          <button class="btn" id="db-replace" type="button" disabled>${translate("Substituir")}</button>
          <button class="btn primary" id="db-insert" type="button" disabled>${translate("Inserir")}</button>
          <button class="btn primary" id="db-connect" type="button">${translate("Conectar")}</button>
        </div>
      </div>`, 'w560', 'top76');

    const $ = <T extends HTMLElement>(sel: string) => bg.querySelector(sel) as T;
    const val = (id: string) => $<HTMLInputElement>(`#db-${id}`).value;
    const statusEl = $<HTMLElement>('#db-status');
    const warnsEl = $<HTMLElement>('#db-warns');
    const connectBtn = $<HTMLButtonElement>('#db-connect');
    const testBtn = $<HTMLButtonElement>('#db-test');
    const replaceBtn = $<HTMLButtonElement>('#db-replace');
    const insertBtn = $<HTMLButtonElement>('#db-insert');
    let cliStatus: 'checking' | 'ready' | 'missing' = this.transport.checkDatabaseSupport ? 'checking' : 'ready';

    const syncView = () => {
      const isFile = model.engine === 'sqlite';
      const connecting = model.status === 'connecting';
      for (const id of CONNECT_NETWORK_ROW_IDS) {
        bg.querySelectorAll<HTMLElement>(`[data-row="${id}"]`).forEach((n) => (n.style.display = isFile ? 'none' : ''));
      }
      bg.querySelectorAll<HTMLElement>('[data-row="path"]').forEach((n) => (n.style.display = isFile ? '' : 'none'));
      bg.querySelector<HTMLElement>('[data-row="trust"]')!.style.display =
        model.engine === 'sqlserver' ? 'flex' : 'none';
      bg.querySelectorAll<HTMLButtonElement>('[data-engine]').forEach((b) => {
        b.classList.toggle('on', b.dataset.engine === model.engine);
        b.disabled = connecting;
      });
      const port = DB_ENGINES.find((e) => e.value === model.engine)?.port;
      $<HTMLInputElement>('#db-port').placeholder = port ? String(port) : translate("porta padrão");
      const ready = model.canApply();
      const conflicts = ready ? this.importConflicts(model.dbml) : [];
      replaceBtn.disabled = !ready;
      insertBtn.disabled = !ready || conflicts.length > 0;
      connectBtn.disabled = connecting || cliStatus !== 'ready';
      connectBtn.classList.toggle('primary', !ready);
      testBtn.disabled = connecting || cliStatus !== 'ready' || !this.transport.testConnection;
      connectBtn.textContent = connecting ? translate("Conectando…") : translate("Conectar");
      insertBtn.style.display = replaceBtn.style.display = ready ? '' : 'none';
      replaceBtn.textContent = model.confirmReplace ? translate("Confirmar substituição?") : translate("Substituir");
      statusEl.style.color = cliStatus === 'missing' || model.status === 'failed' ? 'var(--danger)' : 'var(--muted)';
      $<HTMLElement>('#db-ico').style.color = cliStatus === 'missing' || model.status === 'failed' ? 'var(--danger)' : 'var(--ok)';
      statusEl.textContent =
        cliStatus === 'checking' ? t('Verificando CLI dbml…')
        : cliStatus === 'missing' ? t('CLI dbml indisponível. Instale-a ou configure dbmodeling.cliPath.')
        : model.status === 'connecting'
          ? translate("Conectando e lendo o esquema…")
          : model.status === 'failed'
            ? model.error
            : model.status === 'ready'
              ? translate(model.tables === 1 ? '{} tabela encontrada. Substitua o documento ou insira-a.' : '{} tabelas encontradas. Substitua o documento ou insira-as.', model.tables)
              : model.status === 'tested'
                ? model.error
                : translate("Somente leitura: apenas o catálogo é lido, nunca os seus dados.");
      const warnings = [...model.warnings];
      if (conflicts.length) warnings.push(translate('Já existem no modelo: {}.', conflicts.join(', ')));
      warnsEl.style.display = warnings.length ? 'block' : 'none';
      warnsEl.innerHTML = warnings.map((w) => `<div>${esc(w)}</div>`).join('');
    };

    bg.querySelectorAll<HTMLButtonElement>('[data-engine]').forEach((b) =>
      b.addEventListener('click', () => {
        this.connectEngine = b.dataset.engine as DbEngine;
        model.setEngine(this.connectEngine);
        syncView();
      })
    );
    $<HTMLInputElement>('#db-trust').addEventListener('change', (e) => {
      model.trustCert = (e.target as HTMLInputElement).checked;
    });
    $<HTMLSelectElement>(`#${TLS_SELECT_ID}`).addEventListener('change', (e) => {
      model.tls = (e.target as HTMLSelectElement).value as import('./types').TlsMode;
    });

    const readRequest = () =>
      buildRequest(
        model.engine,
        {
          host: val('host'),
          port: val('port'),
          database: val('database'),
          user: val('user'),
          password: val('password'),
          path: val('path'),
          schemas: val('schemas'),
        },
        model.trustCert,
        model.tls
      );

    let guard: IntrospectGuard | null = null;
    /** Chamada de rede com prazo e cancelamento; o modal pode ter sido fechado no meio. */
    const run = async <T>(
      dialect: IntrospectRequest['dialect'],
      call: (signal: AbortSignal) => Promise<T>,
      done: (result: T) => void
    ) => {
      if (model.status === 'connecting') return;
      model.start();
      syncView();
      const g = (guard = new IntrospectGuard());
      try {
        done(await g.race(call(g.signal)));
      } catch (e) {
        if (g.cancelled) model.cancel();
        else if (g.timedOut) model.fail(translate('A conexão demorou mais de {} s e foi cancelada. Confira o host e a porta.', INTROSPECT_TIMEOUT_MS / 1000));
        else model.fail(connectionHint(e instanceof Error ? e.message : String(e), dialect === 'sqlite'));
      } finally {
        g.dispose();
        if (guard === g) guard = null;
      }
      if (bg.isConnected) syncView();
    };

    const test = () => {
      const testConnection = this.transport.testConnection?.bind(this.transport);
      if (!testConnection) return;
      const req = readRequest();
      return run(req.dialect, (signal) => testConnection(req, signal), (r) => model.finishTest(r.server, r.schemas));
    };

    const connect = () => {
      const req = readRequest();
      return run(req.dialect, (signal) => introspect(req, signal), (r) => model.finish(r));
    };

    /** Durante a conexão, Cancelar/Esc/X só abortam a chamada; fora dela, fecham o modal. */
    const dismiss = () => {
      if (guard) { guard.cancel(); return; }
      this.closeModal();
    };

    const applyMode = (mode: 'insert' | 'replace') => {
      if (!model.canApply() || (mode === 'insert' && this.importConflicts(model.dbml).length)) return;
      if (mode === 'replace' && this.source.trim().length > 0 && !model.confirmReplace) {
        model.confirmReplace = true;
        syncView();
        return;
      }
      const next = model.buildMergedSource(this.source, mode);
      this.closeModal();
      this.applyPatch(this.fullReplace(next));
      if (mode === 'replace') {
        this.relayout(true, false);
      } else {
        this.computeScene();
        this.fit();
        this.queueSaveLayout();
        this.scheduleRender();
      }
    };

    testBtn.addEventListener('click', () => void test());
    connectBtn.addEventListener('click', () => void connect());
    replaceBtn.addEventListener('click', () => applyMode('replace'));
    insertBtn.addEventListener('click', () => applyMode('insert'));
    $<HTMLButtonElement>('#db-cancel').addEventListener('click', dismiss);
    $<HTMLButtonElement>('#db-x').addEventListener('click', dismiss);

    const modalEl = bg.querySelector('.modal') as HTMLElement;
    modalEl.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        dismiss();
      } else if (e.key === 'Enter' && (e.target as HTMLElement).tagName === 'INPUT') {
        e.preventDefault();
        void connect();
      }
    });
    syncView();
    ($<HTMLInputElement>(model.engine === 'sqlite' ? '#db-path' : '#db-host')).focus();
    void this.transport.checkDatabaseSupport?.().then(
      () => { cliStatus = 'ready'; if (bg.isConnected) syncView(); },
      () => { cliStatus = 'missing'; if (bg.isConnected) syncView(); },
    );
  }

  private openImportSqlModal(): void {
    const model = new ImportSqlModel(this.importDialect);
    const runImport = (sql: string, dialect: ImportDialect): ImportSqlResult => {
      if (!this.wasm?.import_sql_ddl) {
        return { dbml: '', warnings: [{ line: 1, column: 1, message: translate("O importador SQL (Wasm) ainda não foi carregado.") }] };
      }
      return this.wasm.import_sql_ddl(sql, dialect) as ImportSqlResult;
    };

    const bg = this.modal(`
      <div role="dialog" aria-modal="true" aria-labelledby="imp-title" style="display:flex;flex-direction:column;min-height:0;overflow:auto">
        <div class="mhd"><div><div class="tt" id="imp-title">${translate("Importar SQL DDL")}</div><div class="st">${translate("Cole instruções CREATE TABLE. A prévia do DBML atualiza enquanto você digita.")}</div></div><span class="grow"></span>
          <button class="ib" id="imp-x" type="button" title="${t("Fechar")}">${ICON('X', 15)}</button></div>
        <div class="mrowh" style="padding:14px 18px">
          <div class="seg row acc" id="imp-dialect">
            ${IMPORT_DIALECTS.map((d) => `<button type="button" data-dialect="${d.value}" class="${model.dialect === d.value ? 'on' : ''}">${d.label}</button>`).join('')}
          </div><span class="grow"></span>
          <input id="imp-file" type="file" accept=".sql,.ddl,text/plain" style="display:none">
          <button class="btn sm" id="imp-pick" type="button">${ICON('FileUp', 13)}${translate("Escolher arquivo .sql…")}</button>
        </div>
        <div class="panes">
          <div class="pane"><div class="ph">${ICON('FileCode', 12)}SQL<span class="grow"></span>${translate("cole ou solte um arquivo .sql")}</div>
            <textarea id="imp-sql" aria-label="SQL DDL" spellcheck="false" placeholder="CREATE TABLE users (id serial PRIMARY KEY, ...);"></textarea></div>
          <div class="pane"><div class="ph">${ICON('Eye', 12)}${translate("Prévia do DBML")}<span class="grow"></span>${translate("somente leitura")}</div>
            <textarea id="imp-preview" aria-label="${translate("Prévia do DBML")}" readonly spellcheck="false"></textarea></div>
        </div>
        <div class="mwarn" id="imp-warns" aria-live="polite" style="display:none"></div>
        <div class="mfooter">
          <span class="sum" id="imp-sum"></span><span class="grow"></span>
          <button class="btn ghost" id="imp-cancel" type="button">${translate("Cancelar")}</button>
          <button class="btn" id="imp-replace" type="button" disabled title="${translate("Substituir o esquema atual")}">${translate("Substituir")}</button>
          <button class="btn primary" id="imp-insert" type="button" disabled title="${translate("Acrescentar ao esquema atual")}">${translate("Inserir")}</button>
        </div>
      </div>`, 'w820', 'top76');

    const sumEl = bg.querySelector('#imp-sum') as HTMLElement;
    const fileInput = bg.querySelector('#imp-file') as HTMLInputElement;
    const pickBtn = bg.querySelector('#imp-pick') as HTMLButtonElement;
    const sqlEl = bg.querySelector('#imp-sql') as HTMLTextAreaElement;
    const previewEl = bg.querySelector('#imp-preview') as HTMLTextAreaElement;
    const warnsEl = bg.querySelector('#imp-warns') as HTMLElement;
    const cancelBtn = bg.querySelector('#imp-cancel') as HTMLButtonElement;
    const replaceBtn = bg.querySelector('#imp-replace') as HTMLButtonElement;
    const insertBtn = bg.querySelector('#imp-insert') as HTMLButtonElement;

    const syncView = () => {
      previewEl.value = model.dbml;
      const can = model.canApply();
      const conflicts = can ? this.importConflicts(model.dbml) : [];
      insertBtn.disabled = !can || conflicts.length > 0;
      replaceBtn.disabled = !can;
      replaceBtn.textContent = model.confirmReplace ? translate("Confirmar substituição?") : translate("Substituir");
      const nt = (model.dbml.match(/^\s*Table\s/gim) || []).length;
      const nr = (model.dbml.match(/\bref\s*:/gi) || []).length;
      sumEl.innerHTML = can
        ? `<span style="color:var(--ok);display:flex">${ICON('CircleCheck', 13)}</span>${nt} ${translate(nt === 1 ? 'tabela' : 'tabelas')} · ${nr} ${translate(nr === 1 ? 'relação detectada' : 'relações detectadas')}`
        : '';
      if (model.tooLarge) {
        warnsEl.style.display = 'block';
        warnsEl.textContent = translate("A entrada excede o limite de 5 MB; divida o dump em arquivos menores.");
      } else if (model.warnings.length || conflicts.length) {
        warnsEl.style.display = 'block';
        const warnings = model.warnings.map((w) => `${w.line}:${w.column}: ${w.message}`);
        if (conflicts.length) warnings.push(translate('Já existem no modelo: {}.', conflicts.join(', ')));
        warnsEl.innerHTML = warnings.map((w) => `<div>${esc(w)}</div>`).join('');
      } else {
        warnsEl.style.display = 'none';
        warnsEl.textContent = '';
      }
    };

    const applyMode = (mode: 'insert' | 'replace') => {
      if (model.sql !== sqlEl.value || !model.canApply() ||
          (mode === 'insert' && this.importConflicts(model.dbml).length)) return;
      if (mode === 'replace' && this.source.trim().length > 0 && !model.confirmReplace) {
        model.confirmReplace = true;
        syncView();
        return;
      }
      const next = model.buildMergedSource(this.source, mode);
      this.closeModal();
      this.applyPatch(this.fullReplace(next));
      if (mode === 'replace') {
        this.relayout(true, false);
      } else {
        // append: quem já estava no canvas não se move; o wasm coloca os cartões novos
        // em área livre (layout::place_without_overlap)
        this.computeScene();
        this.fit();
        this.queueSaveLayout();
        this.scheduleRender();
      }
    };

    let timer: number | undefined;
    sqlEl.addEventListener('input', () => {
      window.clearTimeout(timer);
      insertBtn.disabled = replaceBtn.disabled = true;
      timer = window.setTimeout(() => {
        model.setSql(sqlEl.value, runImport);
        syncView();
      }, 200);
    });
    bg.querySelectorAll<HTMLButtonElement>('[data-dialect]').forEach((b) =>
      b.addEventListener('click', () => {
        this.importDialect = b.dataset.dialect as ImportDialect;
        bg.querySelectorAll('[data-dialect]').forEach((o) => o.classList.toggle('on', o === b));
        model.setDialect(this.importDialect, runImport);
        syncView();
      })
    );
    (bg.querySelector('#imp-x') as HTMLButtonElement).addEventListener('click', () => this.closeModal());
    pickBtn.addEventListener('click', () => fileInput.click());
    fileInput.addEventListener('change', async () => {
      const f = fileInput.files?.[0];
      if (!f) return;
      sqlEl.value = await f.text();
      model.setSql(sqlEl.value, runImport);
      syncView();
    });
    sqlEl.addEventListener('dragover', (e) => {
      e.preventDefault();
    });
    sqlEl.addEventListener('drop', async (e) => {
      const f = e.dataTransfer?.files?.[0];
      if (!f) return;
      e.preventDefault();
      sqlEl.value = await f.text();
      model.setSql(sqlEl.value, runImport);
      syncView();
    });
    cancelBtn.addEventListener('click', () => this.closeModal());
    insertBtn.addEventListener('click', () => applyMode('insert'));
    replaceBtn.addEventListener('click', () => applyMode('replace'));

    const modalEl = bg.querySelector('.modal') as HTMLElement;
    modalEl.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        this.closeModal();
        return;
      }
      if (e.key === 'Tab') {
        const focusables = Array.from(
          modalEl.querySelectorAll<HTMLElement>('textarea, button:not([disabled])')
        );
        if (focusables.length === 0) return;
        const first = focusables[0];
        const last = focusables[focusables.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    });
    sqlEl.focus();
  }

  private confirmSwitchFile(path: string): void {
    const bg = this.modal(`
      <div style="padding:18px;display:flex;flex-direction:column;gap:14px">
        <div style="font-weight:600;font-size:15px">${translate("Alterações não salvas")}</div>
        <div style="font-size:13px;color:var(--muted)">${translate('Deseja salvar as alterações feitas em "{}"?', esc(this.title))}</div>
        <div style="display:flex;justify-content:flex-end;gap:8px">
          <button class="btn ghost" id="dlg-cancel">${translate("Cancelar")}</button>
          <button class="btn" id="dlg-discard" ${this.canDiscard ? '' : 'disabled'}>${translate("Descartar alterações")}</button>
          <button class="btn primary" id="dlg-save">${translate("Salvar")}</button>
        </div>
      </div>`, 'w560', 'mid');
    bg.querySelector('#dlg-cancel')?.addEventListener('click', () => this.closeModal());
    bg.querySelector('#dlg-discard')?.addEventListener('click', async () => {
      const revert = this.savedSource;
      this.closeModal();
      if (revert !== this.source) {
        this.applyPatch(this.fullReplace(revert));
      }
      if (!await this.saveDocument()) return;
      try { await this.saveLayout(); } catch (error) { this.layoutFailed(error, true); return; }
      await this.openProjectFile(path);
    });
    bg.querySelector('#dlg-save')?.addEventListener('click', async () => {
      if (!await this.saveDocument() || this.isDirty()) return;
      try { await this.saveLayout(); } catch (error) { this.layoutFailed(error, true); return; }
      this.closeModal();
      await this.openProjectFile(path);
    });
  }

  private openPalette(): void {
    this.paletteQuery = '';
    this.paletteIdx = 0;
    const bg = this.modal(
      `<div class="mbar">${ICON('Search', 15)}<input id="pal-q" placeholder="${translate('Buscar tabelas, colunas, comandos…')}" autocomplete="off"><span class="esc">Esc</span></div>
       <div class="list" id="pal-list"></div>
       <div class="mfoot"><span>${translate("↑↓ navegar")}</span><span>${translate("↵ executar")}</span><span>${translate("Esc fechar")}</span><span class="grow"></span><span id="pal-n"></span></div>`
    );
    this.paletteOpen = true; // depois de modal(), que zera os flags
    const q = bg.querySelector('#pal-q') as HTMLInputElement;
    const draw = () => {
      const items = this.paletteItems();
      this.paletteIdx = Math.min(this.paletteIdx, Math.max(items.length - 1, 0));
      let last = '';
      (bg.querySelector('#pal-list') as HTMLElement).innerHTML = items
        .map((a, i) => {
          const head = a.group !== last ? `<div class="grp">${esc(a.group)}</div>` : '';
          last = a.group;
          return `${head}<div class="it ${i === this.paletteIdx ? 'on' : ''}" data-act="palette-run" data-index="${i}">${ICON(a.table ? 'Table2' : ACTION_ICON[a.id] || 'Command', 14)}<span>${esc(a.label)}</span><span class="keys">${a.displayKeys.map((k) => `<kbd>${k}</kbd>`).join('')}</span></div>`;
        })
        .join('');
      (bg.querySelector('#pal-n') as HTMLElement).textContent = `${t('{} resultados', items.length)}`;
      (bg.querySelector('.it.on') as HTMLElement | null)?.scrollIntoView({ block: 'nearest' });
    };
    q.addEventListener('input', () => { this.paletteQuery = q.value; this.paletteIdx = 0; draw(); });
    q.addEventListener('keydown', (e) => {
      const items = this.paletteItems();
      if (e.key === 'ArrowDown') { this.paletteIdx = Math.min(this.paletteIdx + 1, items.length - 1); draw(); e.preventDefault(); }
      else if (e.key === 'ArrowUp') { this.paletteIdx = Math.max(this.paletteIdx - 1, 0); draw(); e.preventDefault(); }
      else if (e.key === 'Enter') { const a = items[this.paletteIdx]; this.closeModal(); if (a) this.runPaletteItem(a); e.preventDefault(); }
      else if (e.key === 'Escape') this.closeModal();
    });
    draw();
    q.focus();
  }

  private paletteItems() {
    const q = this.paletteQuery.trim().toLowerCase();
    const fuzzy = (s: string): boolean => {
      let i = 0;
      for (const ch of s.toLowerCase()) if (ch === q[i]) i++;
      return i >= q.length;
    };
    const commands = catalog().filter((a) => (this.selectedRef || (a.id !== 'ToggleEdgeStraight' && a.id !== 'ResetEdgeCurve')) && a.id !== 'OpenPalette' && (!q || fuzzy(a.label) || fuzzy(a.group)));
    const tables = q ? this.schema.tables.flatMap((table) => {
      const name = qualifiedTableName(table);
      const items = [{ label: name, group: translate('Tabelas'), table: name },
        ...table.columns.map((col) => ({ label: `${name}.${col.name}`, group: translate('Colunas'), table: name }))];
      return items.filter((item) => fuzzy(item.label)).map((item) => ({ ...item, id: '' as ActionId, displayKeys: [] as string[] }));
    }) : [];
    return [...commands.map((a) => ({ ...a, table: '' })), ...tables];
  }

  private runPaletteItem(item: { id: ActionId; table: string }): void {
    if (item.table) { this.selectedEnum = null; this.focusTable(item.table); }
    else this.runAction(item.id);
  }

  private openQuickSearch(): void {
    this.quickSearchQuery = '';
    this.quickSearchIdx = 0;
    const bg = this.modal(
      `<div class="mbar">${ICON('Search', 15)}<input id="qs-q" placeholder="${translate('Buscar tabelas, colunas, tipos, enums…')}" autocomplete="off"><span class="esc">Esc</span></div>
       <div class="list" id="qs-list"></div>
       <div class="mfoot"><span>${translate("↑↓ navegar")}</span><span>${translate("↵ ir para")}</span><span>${translate("Esc fechar")}</span><span class="grow"></span><span id="qs-n"></span></div>`
    );
    this.quickSearchOpen = true;
    const q = bg.querySelector('#qs-q') as HTMLInputElement;
    const draw = () => {
      const items = this.quickSearchItems();
      this.quickSearchIdx = Math.min(this.quickSearchIdx, Math.max(items.length - 1, 0));
      (bg.querySelector('#qs-list') as HTMLElement).innerHTML = items
        .map((it, i) => {
          const ic: IconName = it.kind === 'table' ? 'Table2' : it.kind === 'enum' ? 'List' : 'Columns2';
          const badge = it.kind === 'table' ? 'TABLE' : it.kind === 'enum' ? 'ENUM' : 'COL';
          return `<div class="it ${i === this.quickSearchIdx ? 'on' : ''}" data-act="quick-search-run" data-index="${i}">
            ${ICON(ic, 14)}
            <span class="mono" style="flex:1;overflow:hidden;text-overflow:ellipsis">${esc(it.label)}</span>
            <span class="mono" style="font-size:11px;color:var(--faint)">${esc(it.detail)}</span>
            <span class="pill" style="font-family:var(--mono);font-size:9.5px">${badge}</span>
          </div>`;
        })
        .join('');
      (bg.querySelector('#qs-n') as HTMLElement).textContent = `${t('{} resultados', items.length)}`;
      (bg.querySelector('.it.on') as HTMLElement | null)?.scrollIntoView({ block: 'nearest' });
    };
    q.addEventListener('input', () => { this.quickSearchQuery = q.value; this.quickSearchIdx = 0; draw(); });
    q.addEventListener('keydown', (e) => {
      const items = this.quickSearchItems();
      if (e.key === 'ArrowDown') { this.quickSearchIdx = Math.min(this.quickSearchIdx + 1, items.length - 1); draw(); e.preventDefault(); }
      else if (e.key === 'ArrowUp') { this.quickSearchIdx = Math.max(this.quickSearchIdx - 1, 0); draw(); e.preventDefault(); }
      else if (e.key === 'Enter') { const it = items[this.quickSearchIdx]; this.closeModal(); if (it) this.runQuickSearchItem(it); e.preventDefault(); }
      else if (e.key === 'Escape') this.closeModal();
    });
    draw();
    q.focus();
  }

  private quickSearchItems(): Array<{ kind: 'table' | 'column' | 'enum'; table?: string; column?: string; enumName?: string; label: string; detail: string; score: number }> {
    const q = this.quickSearchQuery.trim().toLowerCase();
    const scoreOf = (cand: string): number | null => {
      const c = cand.toLowerCase();
      if (c === q) return 0;
      if (c.startsWith(q)) return 1;
      if (c.includes(q)) return 2;
      let qi = 0;
      for (const ch of c) {
        if (ch === q[qi]) {
          qi++;
          if (qi >= q.length) return 3;
        }
      }
      return null;
    };
    if (!q) {
      return [
        ...this.schema.tables.map((tb) => {
          const qn = qualifiedTableName(tb);
          return { kind: 'table' as const, table: qn, label: qn, detail: `${tb.columns.length} cols`, score: 0 };
        }),
        ...(this.scene?.enums || []).map((ev) => ({
          kind: 'enum' as const, enumName: ev.name, label: ev.name, detail: `${ev.values.length} values`, score: 0,
        })),
      ].slice(0, 60);
    }
    const out: Array<{ kind: 'table' | 'column' | 'enum'; table?: string; column?: string; enumName?: string; label: string; detail: string; score: number; typeOnly: number }> = [];
    for (const tb of this.schema.tables) {
      const qn = qualifiedTableName(tb);
      const ts = [scoreOf(tb.name), scoreOf(qn)].filter((x): x is number => x !== null);
      if (ts.length) {
        out.push({ kind: 'table', table: qn, label: qn, detail: `${tb.columns.length} cols`, score: Math.min(...ts), typeOnly: 0 });
      }
      for (const c of tb.columns) {
        const full = `${qn}.${c.name}`;
        const ns = [scoreOf(c.name), scoreOf(full)].filter((x): x is number => x !== null);
        const tys = scoreOf(c.ty);
        const nameScore = ns.length ? Math.min(...ns) : null;
        const best = nameScore !== null ? nameScore : (tys !== null ? Math.min(3, tys + 1) : null);
        if (best !== null) {
          out.push({ kind: 'column', table: qn, column: c.name, label: full, detail: c.ty, score: best, typeOnly: nameScore === null ? 1 : 0 });
        }
      }
    }
    for (const ev of this.scene?.enums || []) {
      const es = scoreOf(ev.name);
      if (es !== null) {
        out.push({ kind: 'enum', enumName: ev.name, label: ev.name, detail: `${ev.values.length} values`, score: es, typeOnly: 0 });
      }
    }
    const rank = { table: 0, enum: 1, column: 2 } as const;
    out.sort((a, b) => a.score - b.score || a.typeOnly - b.typeOnly || rank[a.kind] - rank[b.kind] || a.label.localeCompare(b.label));
    return out.slice(0, 60);
  }

  private runQuickSearchItem(it: { kind: 'table' | 'column' | 'enum'; table?: string; column?: string; enumName?: string }): void {
    if (it.kind === 'table' && it.table) this.focusTable(it.table);
    else if (it.kind === 'column' && it.table && it.column) this.focusTable(it.table, it.column);
    else if (it.kind === 'enum' && it.enumName) this.focusEnum(it.enumName);
  }

  private openShortcuts(): void {
    const bg = this.modal(
      `<div class="shd">${`<span style="color:var(--accent);display:flex">${ICON('Keyboard', 16)}</span>`}<span class="tt">${translate("Atalhos de teclado")}</span><span class="grow"></span>
         <input id="sc-q" placeholder="${translate("Filtrar atalhos")}" autocomplete="off"><button class="ib" id="sc-x" title="${t("Fechar")}">${ICON('X', 15)}</button></div>
       <div class="shc" id="sc-body"></div>`,
      'w800',
      'mid'
    );
    this.shortcutsOpen = true;
    const draw = (q: string) => {
      const cols: string[][] = [[], []];
      const groups = new Map<string, string>();
      for (const a of catalog()) {
        if (!a.displayKeys.length || (q && !a.label.toLowerCase().includes(q) && !a.group.toLowerCase().includes(q))) continue;
        groups.set(a.group, (groups.get(a.group) || '') +
          `<div class="sr"><span>${esc(a.label)}</span>${a.displayKeys.map((k) => `<kbd>${k}</kbd>`).join('')}</div>`);
      }
      const total = [...groups.values()].reduce((n, h) => n + h.split('class="sr"').length, 0);
      let acc = 0;
      for (const [g, h] of groups) {
        cols[acc < total / 2 ? 0 : 1].push(`<div class="gt">${esc(g)}</div>${h}`);
        acc += h.split('class="sr"').length;
      }
      (bg.querySelector('#sc-body') as HTMLElement).innerHTML = cols.map((c) => `<div>${c.join('')}</div>`).join('');
    };
    draw('');
    const input = bg.querySelector('#sc-q') as HTMLInputElement;
    input.addEventListener('input', () => draw(input.value.trim().toLowerCase()));
    (bg.querySelector('#sc-x') as HTMLButtonElement).addEventListener('click', () => this.closeModal());
    input.focus();
  }

  private loadLocalGuideState(): GuideState {
    try {
      const done = localStorage.getItem(GUIDE_STORAGE_KEY.ONBOARDING_DONE) === 'true';
      const seen = localStorage.getItem(GUIDE_STORAGE_KEY.LAST_SEEN_VERSION);
      return { onboardingDone: done, lastSeenVersion: seen && seen.trim() ? seen.trim() : null };
    } catch {
      return { onboardingDone: false, lastSeenVersion: null };
    }
  }

  private saveGuideState(next: GuideState): void {
    this.guideState = { ...next };
    try {
      localStorage.setItem(GUIDE_STORAGE_KEY.ONBOARDING_DONE, String(next.onboardingDone));
      if (next.lastSeenVersion) localStorage.setItem(GUIDE_STORAGE_KEY.LAST_SEEN_VERSION, next.lastSeenVersion);
    } catch {
      /* fallback seguro em memória quando localStorage está indisponível */
    }
    this.transport.saveGuideState?.(this.guideState);
  }

  private maybeStartGuide(hostState?: GuideState): void {
    if (hostState) this.guideState = { ...hostState };
    if (this.guideChecked || !this.autoGuide) return;
    this.guideChecked = true;
    const decision = decideStartupGuide(APP_VERSION, this.guideState.onboardingDone, this.guideState.lastSeenVersion);
    if (decision === 'tour') {
      this.openTour();
    } else if (decision === 'whats-new') {
      this.openWhatsNew();
    }
  }

  private ensureTourTarget(target: TourTarget): void {
    if (target === 'sidebar' && !this.prefs.sidebar) {
      this.prefs.sidebar = true;
      this.pane = 'schema';
      this.renderChrome();
    } else if (target === 'inspector') {
      if (this.selection.length === 0 && !this.selectedEnum && !this.selectedRef && this.schema.tables.length > 0) {
        this.selection = [qualifiedTableName(this.schema.tables[0])];
      }
      this.prefs.inspector = true;
      this.renderChrome();
    } else if (target === 'minimap' && !this.prefs.minimap) {
      this.prefs.minimap = true;
      this.renderIslands();
    }
  }

  public openTour(): void {
    this.closeModal();
    this.closeMenus();
    if (!this.tour) {
      this.tour = new TourController(this.wb, PRODUCT_GUIDE.tour, () => this.prefs.lang, {
        ensureTarget: (target) => this.ensureTourTarget(target),
        onFinish: () => {
          this.saveGuideState({ onboardingDone: true, lastSeenVersion: APP_VERSION });
        },
      });
    }
    this.tour.start(0);
  }

  public openWhatsNew(): void {
    this.tour?.close();
    this.closeMenus();
    const lang = this.prefs.lang;
    const badgeLabel = (kind: 'added' | 'improved' | 'fixed') =>
      kind === 'added' ? t('Novo') : kind === 'improved' ? t('Melhorado') : t('Corrigido');
    const releasesHtml = PRODUCT_GUIDE.releases
      .map(
        (rel) => `
        <section class="wn-rel">
          <div class="wn-rel-hd"><span class="wn-ver">v${esc(rel.version)}</span><span class="wn-date">${esc(rel.date)}</span></div>
          ${rel.highlights
            .map(
              (h) => `
            <div class="wn-item">
              <span class="wn-badge ${h.kind}">${esc(badgeLabel(h.kind))}</span>
              <div class="wn-copy">
                <strong>${esc(pickText(h.title, lang))}</strong>
                <span>${esc(pickText(h.detail, lang))}</span>
              </div>
              ${h.action ? `<button type="button" class="btn ghost" data-wn-action="${esc(h.action)}">${esc(t('Experimentar'))}</button>` : ''}
            </div>`
            )
            .join('')}
        </section>`
      )
      .join('');

    const markSeen = () => {
      this.saveGuideState({ onboardingDone: this.guideState.onboardingDone, lastSeenVersion: APP_VERSION });
    };

    const bg = this.modal(
      `<div role="dialog" aria-modal="true" aria-labelledby="wn-title" style="display:flex;flex-direction:column;min-height:0">
        <div class="mhd">
          <div>
            <div class="tt" id="wn-title">${esc(t('Novidades da versão {}', APP_VERSION))}</div>
            <div class="st">${esc(t('Histórico de atualizações'))}</div>
          </div>
          <span class="grow"></span>
          <button type="button" class="ib" id="wn-x" aria-label="${esc(t('Fechar'))}" title="${esc(t('Fechar'))}">${ICON('X', 15)}</button>
        </div>
        <div class="wn-list">${releasesHtml}</div>
        <div class="mfooter">
          <button type="button" class="btn ghost" id="wn-tour">${ICON('Info', 13)}${esc(t('Tour guiado'))}</button>
          <span class="grow"></span>
          <button type="button" class="btn primary" id="wn-ok">${esc(t('Entendi'))}</button>
        </div>
      </div>`,
      'w560',
      'mid',
    );
    this.whatsNewOpen = true;
    markSeen();

    bg.querySelector('#wn-x')?.addEventListener('click', () => this.closeModal());
    bg.querySelector('#wn-ok')?.addEventListener('click', () => this.closeModal());
    bg.querySelector('#wn-tour')?.addEventListener('click', () => this.openTour());
    bg.querySelectorAll<HTMLButtonElement>('[data-wn-action]').forEach((btn) =>
      btn.addEventListener('click', () => {
        const act = btn.dataset.wnAction as ActionId | undefined;
        this.closeModal();
        if (act) this.runAction(act);
      })
    );
    (bg.querySelector('#wn-ok') as HTMLElement | null)?.focus();
  }

  // ───────────────────────── eventos ─────────────────────────

  private bind(): void {
    const wb = this.wb;
    wb.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      const el = target.closest('[data-act]') as HTMLElement | null;
      if (!el) { if (!target.closest('.menu')) this.closeMenus(); return; }
      if (el.dataset.act === 'toggle-col' && target.tagName === 'INPUT') return;
      this.handleAct(el.dataset.act!, el);
    });
    const surf = this.els['surface'];
    surf.addEventListener('mousedown', (e) => this.onDown(e));
    const mm = this.els['minimap'];
    mm.addEventListener('wheel', (e) => { e.stopPropagation(); e.preventDefault(); }, { passive: false });
    mm.addEventListener('mousedown', (e) => {
      if (e.button !== 0 || !this.minimapXform || !this.minimapVpRect) return;
      e.stopPropagation();
      e.preventDefault();
      this.stopTween();
      const mr = mm.getBoundingClientRect();
      const mx = ((e.clientX - mr.left) / Math.max(1, mr.width)) * MINIMAP_W;
      const my = ((e.clientY - mr.top) / Math.max(1, mr.height)) * MINIMAP_H;
      const vr = this.minimapVpRect;
      const inside = mx >= vr.x && mx <= vr.x + vr.w && my >= vr.y && my <= vr.y + vr.h;
      const grabX = inside ? mx - (vr.x + vr.w * 0.5) : 0;
      const grabY = inside ? my - (vr.y + vr.h * 0.5) : 0;
      this.minimapDrag = { grabX, grabY };
      if (!inside) this.panFromMinimap(e.clientX, e.clientY, 0, 0);
    });
    window.addEventListener('mousemove', (e) => {
      if (!this.minimapDrag) return;
      this.panFromMinimap(e.clientX, e.clientY, this.minimapDrag.grabX, this.minimapDrag.grabY);
    }, { signal: this.abort.signal });
    window.addEventListener('mouseup', () => {
      if (!this.minimapDrag) return;
      this.minimapDrag = null;
      this.queueSaveLayout();
    }, { signal: this.abort.signal });
    surf.addEventListener('contextmenu', (e) => {
      const hit = (e.target as Element).closest('[data-edge]') as HTMLElement | null;
      if (!hit) return;
      e.preventDefault();
      this.selectEdge(hit);
      this.scheduleRender();
      this.closeMenus();
      const menu = document.createElement('div');
      menu.className = 'menu';
      menu.innerHTML = this.miHtml('ToggleEdgeStraight', 'Relação: reta/curva') + this.miHtml('ResetEdgeCurve', 'Relação: restaurar curva automática');
      this.wb.appendChild(menu);
      menu.style.left = `${Math.min(e.clientX, window.innerWidth - menu.offsetWidth - 8)}px`;
      menu.style.top = `${Math.min(e.clientY, window.innerHeight - menu.offsetHeight - 8)}px`;
    });
    const { signal } = this.abort;
    window.addEventListener('mousemove', (e) => this.onMove(e), { signal });
    window.addEventListener('mouseup', () => this.onUp(), { signal });
    surf.addEventListener('wheel', (e) => this.onWheel(e), { passive: false });
    this.resizeObserver = new ResizeObserver(() => this.scheduleRender());
    this.resizeObserver.observe(surf);
    for (const handle of wb.querySelectorAll<HTMLButtonElement>('[data-resize]')) {
      const isLeft = handle.dataset.resize === 'left';
      const panel = () => this.els[isLeft ? (this.editorOpen ? 'editor' : 'sidebar') : 'inspector'];
      const resize = (el: HTMLElement, width: number) => {
        const scale = Number(getComputedStyle(el).zoom) || 1;
        el.style.width = `${width / scale}px`;
        this.scheduleRender();
      };
      handle.addEventListener('pointerdown', (e) => {
        if (e.button !== 0) return;
        const el = panel();
        const x = e.clientX;
        const width = el.getBoundingClientRect().width;
        const move = (move: PointerEvent) => resize(el, width + (move.clientX - x) * (isLeft ? 1 : -1));
        handle.setPointerCapture(e.pointerId);
        handle.addEventListener('pointermove', move);
        handle.addEventListener('lostpointercapture', () => handle.removeEventListener('pointermove', move), { once: true });
      });
      handle.addEventListener('keydown', (e) => {
        if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
        e.preventDefault();
        e.stopPropagation();
        const el = panel();
        const grow = (e.key === 'ArrowRight') === isLeft;
        resize(el, el.getBoundingClientRect().width + (grow ? 24 : -24));
      });
    }
    window.addEventListener('keydown', (e) => this.onKey(e), { signal });
    window.addEventListener('beforeunload', (e) => {
      if (this.isDirty()) {
        e.preventDefault();
        e.returnValue = '';
      }
    }, { signal });
    window.addEventListener('keyup', (e) => {
      if (e.code === 'Space') { this.space = false; this.scheduleRender(); }
    }, { signal });
    // editor DBML
    const ta = this.els['code'] as HTMLTextAreaElement;
    ta.addEventListener('input', () => {
      if (ta.value === this.source) return;
      this.pushHistory();
      this.applyPatch(this.fullReplace(ta.value));
      this.scheduleRender();
    });
    ta.addEventListener('scroll', () => this.syncEditorScroll());
    // inspector
    const insp = this.els['inspector'];
    insp.addEventListener('change', (e) => this.onInspectorChange(e.target as HTMLInputElement));
    insp.addEventListener('keydown', (e) => {
      const t = e.target as HTMLInputElement;
      if (t.dataset.act === 'toggle-col' && (e.key === 'Enter' || e.key === ' ')) {
        e.preventDefault(); e.stopPropagation();
        const col = t.dataset.col;
        this.handleAct('toggle-col', t);
        (Array.from(insp.querySelectorAll<HTMLElement>('[data-act=toggle-col]')).find((row) => row.dataset.col === col))?.focus();
        return;
      }
      if (t.tagName !== 'INPUT') return;
      if (e.key === 'Enter') t.blur();
      else if (e.key === 'Escape') { t.value = t.dataset.orig || ''; t.blur(); this.closeMenus(); e.stopPropagation(); }
    });
    insp.addEventListener('focusin', (e) => this.suggest(e.target as HTMLInputElement));
    insp.addEventListener('input', (e) => this.suggest(e.target as HTMLInputElement));
    insp.addEventListener('focusout', () => setTimeout(() => this.wb.querySelectorAll('.suggest').forEach((n) => n.remove()), 120));
  }

  private handleAct(act: string, el: HTMLElement): void {
    this.fxStart(actSelector(act, el.dataset), 'fx-press');
    this.handleActInner(act, el);
    // popover Aparência continua aberto para ver o efeito
    if (el.dataset.keep) { this.appearanceMenu(); this.applyFx(); }
  }

  private handleActInner(act: string, el: HTMLElement): void {
    const d = el.dataset;
    switch (act) {
      case 'menu:appearance': this.appearanceMenu(); return;
      case 'menu:overflow':
        this.openMenu(el, [
          this.miHtml('ExportDbml', t('Exportar DBML')),
          `<button class="mi" data-act="menu:export-code">${t('Exportar código')}</button>`,
          `<button class="mi" data-act="menu:export-image">${t('Exportar imagem')}</button>`,
          this.miHtml('ToggleTheme', t('Alternar tema claro/escuro')),
        ].join(''), Math.min(230, window.innerWidth - 16));
        return;
      case 'menu:main': this.mainMenu(); return;
      case 'menu-sub': this.menuSub = d.sub!; this.mainMenu(); return;
      case 'menu:export-code': this.exportMenu(el, 'code'); return;
      case 'menu:export-image': this.exportMenu(el, 'image'); return;
      case 'menu:table-group': this.tableGroupMenu(el, d.table!); return;
      case 'assign-group':
        this.closeMenus();
        this.dispatch({ action: 'group_tables', payload: { name: d.group!, tables: [d.table!] } });
        return;
      case 'toggle-col': {
        const ae = document.activeElement as HTMLElement | null;
        if (ae && this.els['inspector'].contains(ae)) ae.blur();
        this.openCol = this.openCol === d.col ? null : (d.col || null);
        this.renderInspector();
        return;
      }
      case 'toggle-sec': {
        const k = d.sec!;
        if (k === 'refs' && this.schema.refs.length > 3 && this.selection.length > 0 && !this.collapsed.has('refs:open')) {
          this.collapsed.add('refs:open');
          this.collapsed.delete('refs');
        } else if (this.collapsed.has(k)) {
          this.collapsed.delete(k);
        } else {
          this.collapsed.add(k);
        }
        this.renderChrome();
        return;
      }
      case 'set-mode': this.prefs.mode = d.mode as Mode; break;
      case 'set-lang': this.prefs.lang = d.lang as Language; setLanguage(this.prefs.lang); break;
      case 'set-ui': this.prefs.uiScale = parseFloat(d.ui || '1'); break;
      case 'open-file':
        if (this.isDirty()) {
          this.confirmSwitchFile(d.path!);
        } else {
          void (async () => {
            try {
              if (this.saveTimer) await this.saveLayout();
            } catch (error) {
              this.layoutFailed(error);
              return;
            }
            await this.openProjectFile(d.path!);
          })();
        }
        return;
      case DIFF_ACT.FOCUS: this.focusDiffItem(d.diffId!); return;
      case DIFF_ACT.RETRY: this.startDiff(); return;
      case 'focus-table': this.selectedEnum = null; this.focusTable(d.table!); return;
      case 'focus-enum': {
        this.selection = [];
        this.selectedRef = null;
        this.selectedEnum = d.enum!;
        const p = this.positions[`enum:${d.enum!}`];
        if (p) {
          const r = this.els['surface'].getBoundingClientRect();
          this.pan = { x: r.width / 2 - (p.x + CARD_W / 2) * this.zoom, y: r.height / 2 - (p.y + 60) * this.zoom };
        }
        this.scheduleRender();
        return;
      }
      case 'enum-addval':
        this.dispatch({ action: 'add_enum_value', payload: { enum_name: d.enum! } });
        return;
      case 'enum-delval':
        this.dispatch({ action: 'delete_enum_value', payload: { enum_name: d.enum!, value: d.val! } });
        return;
      case 'enum-move':
        this.dispatch({
          action: 'reorder_enum_value',
          payload: { enum_name: d.enum!, value: d.val!, up: d.up === '1' },
        });
        return;
      case 'enum-del':
        this.dispatch({ action: 'delete_enum', payload: { name: d.enum! } });
        this.selectedEnum = null;
        return;
      case 'goto-diag': {
        const g = this.diags[parseInt(d.i!, 10)];
        if (!g) return;
        this.openEditor();
        this.renderChrome();
        const ta = this.els['code'] as HTMLTextAreaElement;
        const off = this.offsetOf(g.range.start.line, g.range.start.character);
        ta.focus();
        ta.setSelectionRange(off, off);
        ta.scrollTop = Math.max(0, (g.range.start.line - 3) * 22);
        return;
      }
      case 'palette-run': { const item = this.paletteItems()[Number(d.index)]; this.closeModal(); if (item) this.runPaletteItem(item); return; }
      case 'quick-search-run': { const item = this.quickSearchItems()[Number(d.index)]; this.closeModal(); if (item) this.runQuickSearchItem(item); return; }
      case 'set-color':
        this.dispatch({ action: 'set_table_color', payload: { table: d.table!, color: d.color || null } });
        return;
      case 'toggle-flag':
        if (d.flag === 'fk' && d.on === '1') {
          const ref = this.schema.refs.find((r) => qualifiedEndpointTable(r.from) === d.table! && r.from.column === d.col!);
          if (ref) {
            this.dispatch({
              action: 'delete_ref',
              payload: { from_table: qualifiedEndpointTable(ref.from), from_col: ref.from.column, to_table: qualifiedEndpointTable(ref.to), to_col: ref.to.column },
            });
            return;
          }
        }
        this.dispatch({ action: 'set_column_flag', payload: { table: d.table!, column: d.col!, flag: d.flag!, value: d.on !== '1' } });
        return;
      case 'del-ref':
        this.dispatch({
          action: 'delete_ref',
          payload: { from_table: d.fromTable!, from_col: d.fromCol!, to_table: d.toTable!, to_col: d.toCol! },
        });
        return;
      case 'del-selected-ref':
        if (this.selectedRef) {
          this.dispatch({ action: 'delete_ref', payload: { ...this.selectedRef } });
          this.selectedRef = null;
        }
        return;
      case 'update-ref-kind':
      case 'update-ref-del':
      case 'update-ref-upd': {
        if (!this.selectedRef) return;
        const sr = this.selectedRef;
        const r = this.schema.refs.find(
          (x) =>
            (qualifiedEndpointTable(x.from) === sr.from_table &&
              x.from.column === sr.from_col &&
              qualifiedEndpointTable(x.to) === sr.to_table &&
              x.to.column === sr.to_col) ||
            (qualifiedEndpointTable(x.from) === sr.to_table &&
              x.from.column === sr.to_col &&
              qualifiedEndpointTable(x.to) === sr.from_table &&
              x.to.column === sr.from_col)
        );
        if (!r) return;
        const kind =
          act === 'update-ref-kind' ? (d.kind as import('./types').RefKind) : r.kind;
        const on_delete =
          act === 'update-ref-del'
            ? ((d.val || null) as import('./types').RefAction | null)
            : (r.on_delete ?? null);
        const on_update =
          act === 'update-ref-upd'
            ? ((d.val || null) as import('./types').RefAction | null)
            : (r.on_update ?? null);
        this.dispatch({
          action: 'update_ref',
          payload: {
            from_table: qualifiedEndpointTable(r.from),
            from_col: r.from.column,
            to_table: qualifiedEndpointTable(r.to),
            to_col: r.to.column,
            kind,
            on_delete,
            on_update,
          },
        });
        return;
      }
      case 'del-col':
        this.dispatch({ action: 'delete_column', payload: { table: d.table!, column: d.col! } });
        return;
      case 'add-col':
        this.dispatch({ action: 'add_column', payload: { table: d.table! } });
        return;
      case 'add-index': {
        const tb = this.schema.tables.find((x) => qualifiedTableName(x) === d.table!);
        const firstCol = tb?.columns[0]?.name;
        if (!firstCol) return;
        this.dispatch({
          action: 'add_index',
          payload: {
            table: d.table!,
            index: {
              columns: [{ kind: 'column', value: firstCol }],
              unique: false,
              pk: false,
              name: null,
              index_type: null,
            },
          },
        });
        return;
      }
      case 'del-index':
        this.dispatch({
          action: 'delete_index',
          payload: { table: d.table!, index_idx: parseInt(d.idx!, 10) },
        });
        return;
      case 'idx-toggle-col':
      case 'idx-toggle-flag':
      case 'idx-set-type': {
        const tb = this.schema.tables.find((x) => qualifiedTableName(x) === d.table!);
        const idxNum = parseInt(d.idx!, 10);
        const ix = tb?.indexes?.[idxNum];
        if (!ix) return;
        const next: import('./types').IndexDef = {
          columns: [...ix.columns],
          unique: ix.unique,
          pk: ix.pk,
          name: ix.name ?? null,
          index_type: ix.index_type ?? null,
        };
        if (act === 'idx-toggle-col') {
          const colName = d.col!;
          const pos = next.columns.findIndex((c) => c.kind === 'column' && c.value === colName);
          if (pos >= 0) {
            if (next.columns.length > 1) next.columns.splice(pos, 1);
          } else {
            next.columns.push({ kind: 'column', value: colName });
          }
        } else if (act === 'idx-toggle-flag') {
          if (d.flag === 'unique') next.unique = !next.unique;
          else if (d.flag === 'pk') next.pk = !next.pk;
        } else if (act === 'idx-set-type') {
          next.index_type = (d.type || null) as import('./types').IndexType | null;
        }
        this.dispatch({
          action: 'update_index',
          payload: { table: d.table!, index_idx: idxNum, index: next },
        });
        return;
      }
      default:
        this.closeMenus();
        this.runAction(act as ActionId);
        return;
    }
    this.closeMenus();
    this.savePrefs();
    this.scheduleRender();
  }

  private onInspectorChange(t: HTMLInputElement): void {
    const kind = t.dataset.edit;
    if (!kind) return;
    const val = t.value.trim();
    const orig = t.dataset.orig || '';
    if (val === orig) return;
    if (kind === 'coldefault') {
      this.dispatch({
        action: 'set_column_default',
        payload: { table: t.dataset.table!, column: t.dataset.col!, default_value: val || null },
      });
      return;
    }
    if (kind === 'colcheck') {
      this.dispatch({
        action: 'set_column_check',
        payload: { table: t.dataset.table!, column: t.dataset.col!, check: val || null },
      });
      return;
    }
    if (kind === 'colnote') {
      this.dispatch({
        action: 'set_column_note',
        payload: { table: t.dataset.table!, column: t.dataset.col!, note: val || null },
      });
      return;
    }
    if (kind === 'tablenote') {
      this.dispatch({
        action: 'set_table_note',
        payload: { table: t.dataset.table!, note: val || null },
      });
      return;
    }
    if (kind === 'idxname') {
      const tb = this.schema.tables.find((x) => x.name === t.dataset.table!);
      const idxNum = parseInt(t.dataset.idx!, 10);
      const ix = tb?.indexes?.[idxNum];
      if (!ix) return;
      this.dispatch({
        action: 'update_index',
        payload: {
          table: t.dataset.table!,
          index_idx: idxNum,
          index: { ...ix, name: val || null },
        },
      });
      return;
    }
    if (kind !== 'coltype' && !IDENT.test(val)) {
      this.toast(translate("Nome inválido: use letras, números e _."));
      t.value = orig;
      return;
    }
    if (!val) { t.value = orig; return; }
    if (kind === 'enum') {
      this.dispatch({ action: 'rename_enum', payload: { old_name: t.dataset.enum!, new_name: val } });
      return;
    }
    if (kind === 'enumval') {
      this.dispatch({
        action: 'rename_enum_value',
        payload: { enum_name: t.dataset.enum!, old_value: orig, new_value: val },
      });
      return;
    }
    const table = t.dataset.table!;
    if (kind === 'table') this.dispatch({ action: 'rename_table', payload: { old_name: table, new_name: val } });
    else if (kind === 'colname') {
      if (this.openCol === t.dataset.col) this.openCol = val;
      this.dispatch({ action: 'rename_column', payload: { table, old_name: t.dataset.col!, new_name: val } });
    } else this.dispatch({ action: 'set_column_type', payload: { table, column: t.dataset.col!, ty: val } });
  }

  private suggest(t: HTMLInputElement): void {
    if (!t || t.dataset?.edit !== 'coltype') return;
    this.wb.querySelectorAll('.suggest').forEach((n) => n.remove());
    const q = t.value.trim().toLowerCase();
    const opts: Array<[string, string]> = [];
    for (const [ty, d] of STANDARD_TYPES) if (!q || ty.toLowerCase().includes(q)) opts.push([ty, d]);
    for (const en of this.enums) if (!q || en.toLowerCase().includes(q)) opts.push([en, translate("enumeração")]);
    if (!opts.length) return;
    const box = document.createElement('div');
    box.className = 'suggest';
    box.innerHTML = `<div class="hh"><span>${translate("Sugestões")}</span><span>Esc</span></div>` +
      opts.slice(0, 8).map(([ty, d]) => `<div class="op" data-ty="${esc(ty)}"><span>${esc(ty)}</span><small>${esc(translate(d))}</small></div>`).join('');
    const r = t.getBoundingClientRect();
    box.style.left = Math.max(8, Math.min(r.left, window.innerWidth - 230)) + 'px';
    box.style.top = r.bottom + 2 + 'px';
    // mousedown + preventDefault: aplica o tipo ANTES do blur do input salvar o texto incompleto
    box.addEventListener('mousedown', (e) => {
      const op = (e.target as HTMLElement).closest('.op') as HTMLElement | null;
      if (!op) return;
      e.preventDefault();
      box.remove();
      this.dispatch({ action: 'set_column_type', payload: { table: t.dataset.table!, column: t.dataset.col!, ty: op.dataset.ty! } });
    });
    this.wb.appendChild(box);
  }

  private selectEdge(hit: HTMLElement): void {
    this.selection = [];
    this.selectedEnum = null;
    this.selectedRef = { from_table: hit.dataset.fromTable!, from_col: hit.dataset.fromCol!, to_table: hit.dataset.toTable!, to_col: hit.dataset.toCol! };
  }

  private moveEdge(dx: number, dy: number): void {
    if (!this.selectedRef || (!dx && !dy)) return;
    const key = edgeKey(this.selectedRef);
    const old = this.edges[key];
    let offset: Pt = { x: old?.dx ?? 0, y: old?.dy ?? 0 };
    if (old?.straight) {
      offset = { x: 0, y: 0 };
      const edge = this.scene?.edges.find((e) => edgeKey(e) === key);
      const a = edge && this.positions[edge.from_table], b = edge && this.positions[edge.to_table];
      if (edge && a && b) {
        const straight = curvedEdge(a.x, edge.ay, b.x, edge.by, old).midpoint;
        const auto = curvedEdge(a.x, edge.ay, b.x, edge.by).midpoint;
        offset = { x: (straight.x - auto.x) / 0.75, y: (straight.y - auto.y) / 0.75 };
      }
    }
    this.edges[key] = { dx: offset.x + dx / 0.75, dy: offset.y + dy / 0.75 };
  }

  private onDown(e: MouseEvent): void {
    const t = e.target as HTMLElement;
    this.stopTween();
    this.closeMenus();
    this.els['surface'].focus();
    if (t.closest('.island, .legend, .empty')) return;
    if (e.button === 1 || this.tool === 'hand' || this.space) {
      this.drag = { kind: 'pan', lx: e.clientX, ly: e.clientY };
      e.preventDefault();
      this.scheduleRender();
      return;
    }
    if (e.button !== 0) return;
    const wp = this.toWorld(e);
    if (t.closest('[data-edge-handle]') && this.selectedRef) {
      this.drag = { kind: 'edge', last: wp, snapshot: this.captureSnapshot(), moved: false };
      e.preventDefault();
      return;
    }
    const edgeHit = t.closest('[data-edge]') as HTMLElement | null;
    if (edgeHit) {
      this.selectEdge(edgeHit);
      this.scheduleRender();
      return;
    }
    const port = t.closest('[data-port]') as HTMLElement | null;
    if (port) {
      this.selectedRef = null;
      this.drag = { kind: 'link', table: port.dataset.table!, col: port.dataset.col!, now: wp };
      e.preventDefault();
      this.scheduleRender();
      return;
    }
    const plus = t.closest('[data-addcol]') as HTMLElement | null;
    if (plus) {
      this.selectedRef = null;
      this.selection = [plus.dataset.addcol!];
      this.dispatch({ action: 'add_column', payload: { table: plus.dataset.addcol! } });
      return;
    }
    const gl = t.closest('[data-group]') as HTMLElement | null;
    if (gl) {
      this.selectedRef = null;
      this.dragStartPositions = this.clonePositions();
      this.drag = { kind: 'group', name: gl.dataset.group!, last: wp };
      this.scheduleRender();
      return;
    }
    const card = t.closest('.card') as HTMLElement | null;
    if (card && card.dataset.enum) {
      this.selectedRef = null;
      this.selection = [];
      const enumName = card.dataset.enum;
      this.selectedEnum = enumName;
      if (e.detail >= 2) {
        this.runAction('RenameSelected');
        return;
      }
      this.dragStartPositions = this.clonePositions();
      this.drag = { kind: 'enum', name: enumName, last: wp };
      this.scheduleRender();
      return;
    }
    if (card) {
      this.selectedRef = null;
      this.selectedEnum = null;
      const name = card.dataset.table!;
      if (e.shiftKey) {
        this.selection = this.selection.includes(name) ? this.selection.filter((n) => n !== name) : [...this.selection, name];
      } else if (!this.selection.includes(name)) {
        this.selection = [name];
      }
      this.top = name;
      if (e.detail >= 2) {
        this.selection = [name];
        this.runAction('RenameSelected');
        return;
      }
      this.dragStartPositions = this.clonePositions();
      this.drag = { kind: 'cards', last: wp };
      this.fxStart('.card.sel', 'fx-lift', MOTION.fast);
      this.scheduleRender();
      return;
    }
    this.selectedRef = null;
    this.selectedEnum = null;
    if (!e.shiftKey) {
      this.focusMode = false;
      this.highlightedColumn = null;
    }
    const base = e.shiftKey ? [...this.selection] : [];
    this.selection = base;
    this.drag = { kind: 'marquee', start: wp, now: wp, base };
    this.scheduleRender();
  }

  private onMove(e: MouseEvent): void {
    const d = this.drag;
    if (!d) return;
    const wp = this.toWorld(e);
    if (d.kind === 'pan') {
      this.pan = { x: this.pan.x + e.clientX - d.lx, y: this.pan.y + e.clientY - d.ly };
      d.lx = e.clientX; d.ly = e.clientY;
      this.scheduleCamera();
      return;
    } else if (d.kind === 'edge') {
      const dx = wp.x - d.last.x, dy = wp.y - d.last.y;
      d.last = wp;
      if (dx || dy) { this.moveEdge(dx, dy); d.moved = true; }
    } else if (d.kind === 'cards') {
      const dx = wp.x - d.last.x, dy = wp.y - d.last.y;
      d.last = wp;
      for (const n of this.selection) {
        const p = this.positions[n];
        if (p) { p.x += dx; p.y += dy; }
      }
      this.queueEdges();
    } else if (d.kind === 'enum') {
      const dx = wp.x - d.last.x, dy = wp.y - d.last.y;
      d.last = wp;
      const key = `enum:${d.name}`;
      const p = this.positions[key];
      if (p) { p.x += dx; p.y += dy; }
    } else if (d.kind === 'group') {
      const dx = wp.x - d.last.x, dy = wp.y - d.last.y;
      d.last = wp;
      const g = this.schema.groups.find((x) => x.name === d.name);
      for (const n of g?.tables || []) {
        const p = this.positions[n];
        if (p) { p.x += dx; p.y += dy; }
      }
      this.queueEdges();
    } else if (d.kind === 'marquee') {
      d.now = wp;
      this.drawMarquee();
      return;
    } else if (d.kind === 'link') {
      d.now = wp;
    }
    this.scheduleRender();
  }

  /** Recalcula rotas ortogonais com as posições atuais (Wasm: `layout::route`). */
  private recomputeEdges(): void {
    this.edgesDirty = false;
    if (this.engineFailed || this.destroyed || !this.wasm) return;
    try {
      const session = this.analysisSession();
      if (session && this.scene) {
        // sem reparse: só as arestas; as molduras de grupo seguem as posições em TS
        const edges = session.route_edges(this.positions);
        const groups = canvasGroups(this.scene.groups, this.schema, this.positions);
        this.scene = { ...this.scene, edges, groups };
      } else {
        this.scene = this.wasm.compute_scene(this.source, this.positions);
      }
    } catch (e) { this.wasmFailed('rotear arestas (arrasto)', e); /* mantém a cena anterior */ }
  }

  /** Pede o reroteamento das arestas para o próximo quadro (vários mousemove por quadro viram um). */
  private queueEdges(): void {
    this.edgesDirty = true;
    this.scheduleRender();
  }

  private onUp(): void {
    const d = this.drag;
    if (!d) return;
    if (this.edgesDirty) this.recomputeEdges();
    this.drag = null;
    if (d.kind === 'cards') this.fxStart('.card.sel', 'fx-drop', MOTION.fast);
    if (d.kind === 'pan') {
      this.scheduleSessionSave();
      return;
    }
    if (d.kind === 'edge') {
      if (d.moved) { this.pushHistory(d.snapshot); this.queueSaveLayout(); }
    } else if (d.kind === 'marquee') {
      const x0 = Math.min(d.start.x, d.now.x), x1 = Math.max(d.start.x, d.now.x);
      const y0 = Math.min(d.start.y, d.now.y), y1 = Math.max(d.start.y, d.now.y);
      if (x1 - x0 > 2 || y1 - y0 > 2) {
        const sel = [...d.base];
        for (const t of this.schema.tables) {
          const qname = qualifiedTableName(t);
          const p = this.positions[qname];
          if (p && p.x < x1 && p.x + CARD_W > x0 && p.y < y1 && p.y + cardH(t.columns.length) > y0 && !sel.includes(qname)) sel.push(qname);
        }
        this.selection = sel;
      }
    } else if (d.kind === 'link') {
      const el = document.elementFromPoint(this.lastMouse.x, this.lastMouse.y) as HTMLElement | null;
      const targetCard = el?.closest('.card') as HTMLElement | null;
      const targetTable = targetCard?.dataset.table;
      if (targetTable && targetTable !== d.table) {
        this.dispatch({
          action: 'add_ref',
          payload: { from_table: d.table, from_col: d.col, to_table: targetTable, to_col: 'id' },
        });
      }
    } else {
      if (this.dragStartPositions && !this.positionsEqual(this.dragStartPositions, this.positions)) {
        this.pushHistory({
          source: this.source,
          positions: this.dragStartPositions,
          selection: [...this.selection],
          selectedEnum: this.selectedEnum,
          selectedRef: this.selectedRef ? { ...this.selectedRef } : null,
          edges: structuredClone(this.edges),
        });
      }
      this.dragStartPositions = null;
      this.queueSaveLayout();
    }
    this.scheduleRender();
  }

  private lastMouse: Pt = { x: 0, y: 0 };

  private onWheel(e: WheelEvent): void {
    e.preventDefault();
    this.stopTween();
    const r = this.els['surface'].getBoundingClientRect();
    if (e.ctrlKey || e.metaKey) {
      // pinça do touchpad chega como Ctrl+roda com delta pequeno: precisa de ganho maior que a roda do mouse
      const gain = Math.abs(e.deltaY) < 50 && e.deltaMode === 0 ? 0.01 : 0.0025;
      this.zoomAt(e.clientX - r.left, e.clientY - r.top, Math.min(Math.max(1 - e.deltaY * gain, 0.5), 2));
    } else {
      if (e.shiftKey && e.deltaX === 0) {
        this.pan.x -= e.deltaY;
      } else {
        this.pan.x -= e.deltaX;
        this.pan.y -= e.deltaY;
      }
      this.queueSaveLayout();
      this.scheduleCamera();
      return;
    }
    this.queueSaveLayout();
    this.invalidate(Dirty.World | Dirty.Islands | Dirty.Status);
  }

  /** Teclado no cartão focado: Enter seleciona; setas movem (Shift = 1px, senão 8px). */
  private cardKey(card: HTMLElement, e: KeyboardEvent): boolean {
    const table = card.dataset.table;
    const enumName = card.dataset.enum;
    if (e.key === 'Enter' && table !== undefined) {
      this.selection = [table];
      this.selectedEnum = null;
      this.selectedRef = null;
      this.top = table;
      this.scheduleRender();
      return true;
    }
    const arrows: Record<string, Pt> = {
      ArrowLeft: { x: -1, y: 0 }, ArrowRight: { x: 1, y: 0 }, ArrowUp: { x: 0, y: -1 }, ArrowDown: { x: 0, y: 1 },
    };
    const dir = arrows[e.key];
    if (!dir || (table === undefined && enumName === undefined)) return false;
    const step = e.shiftKey ? 1 : 8;
    const keys = table !== undefined
      ? (this.selection.includes(table) ? this.selection : [table])
      : [`enum:${enumName}`];
    if (table !== undefined && !this.selection.includes(table)) {
      this.selection = [table];
      this.selectedEnum = null;
      this.selectedRef = null;
    }
    this.pushHistory();
    for (const k of keys) {
      const p = this.positions[k];
      if (p) { p.x += dir.x * step; p.y += dir.y * step; }
    }
    this.recomputeEdges();
    this.queueSaveLayout();
    this.scheduleRender();
    const p = this.positions[keys[0]];
    if (p) this.announce(translate('{} movida para x {}, y {}', keys.length > 1 ? translate('{} itens', keys.length) : keys[0].replace(/^enum:/, ''), Math.round(p.x), Math.round(p.y)));
    return true;
  }

  private keyOf(e: KeyboardEvent): string {
    let k = e.key.toLowerCase();
    if (e.code.startsWith('Digit')) k = e.code.slice(5);
    else if (e.code === 'Minus') k = '-';
    else if (e.code === 'Equal') k = '=';
    else if (e.code === 'Slash') k = '/';
    if (k === 'backspace') k = 'delete';
    if (k === 'arrowup') k = 'up';
    if (k === 'arrowdown') k = 'down';
    const mods = [(e.ctrlKey || e.metaKey) && 'ctrl', e.altKey && 'alt', e.shiftKey && 'shift'].filter(Boolean);
    return [...mods, k].join('-');
  }

  private onKey(e: KeyboardEvent): void {
    const ae = document.activeElement as HTMLElement | null;
    const typing = !!ae && (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA' || ae.isContentEditable);
    if (e.code === 'Space' && !typing && !this.paletteOpen && !this.quickSearchOpen && ae?.tagName !== 'BUTTON') {
      if (!this.space) { this.space = true; this.scheduleRender(); }
      e.preventDefault();
      return;
    }
    if ((this.paletteOpen || this.quickSearchOpen) && e.key !== 'Escape') return;
    if (this.selectedRef && !typing && !this.shortcutsOpen && !e.ctrlKey && !e.metaKey && !e.altKey && ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) {
      const step = e.shiftKey ? 1 : 8;
      this.pushHistory();
      this.moveEdge(e.key === 'ArrowLeft' ? -step : e.key === 'ArrowRight' ? step : 0, e.key === 'ArrowUp' ? -step : e.key === 'ArrowDown' ? step : 0);
      this.queueSaveLayout();
      this.scheduleRender();
      e.preventDefault();
      return;
    }
    const focusedCard = !typing && !e.ctrlKey && !e.metaKey && !e.altKey && !this.shortcutsOpen
      ? ae?.closest<HTMLElement>('.card') ?? null
      : null;
    if (focusedCard && this.cardKey(focusedCard, e)) {
      e.preventDefault();
      return;
    }
    const key = this.keyOf(e);
    if (key === 'ctrl-p') {
      e.preventDefault();
      this.closeMenus();
      this.openQuickSearch();
      return;
    }
    const entry = catalog().find((a) => a.key === key || (a.id === 'DeleteSelection' && key === 'delete'));
    const modalOpen = this.paletteOpen || this.quickSearchOpen || this.shortcutsOpen || this.whatsNewOpen;
    if (!entry || !entry.key) {
      if (e.key === 'Escape' && modalOpen) this.closeModal();
      return;
    }
    if (entry.context === 'Canvas' && (typing || modalOpen)) {
      if (entry.id === 'Deselect' && modalOpen) this.closeModal();
      return;
    }
    // Alt+Up/Down só valem dentro do editor DBML
    if ((entry.id === 'MoveLineUp' || entry.id === 'MoveLineDown') && ae !== this.els['code']) return;
    e.preventDefault();
    this.closeMenus();
    if (entry.id === 'Deselect' && modalOpen) { this.closeModal(); return; }
    this.runAction(entry.id);
  }

  // rastreia posição do mouse para soltar relações sobre colunas
  public attachMouseTracking(): void {
    window.addEventListener('mousemove', (e) => { this.lastMouse = { x: e.clientX, y: e.clientY }; }, { signal: this.abort.signal });
  }

  /**
   * Desmonta o workbench: remove os listeners globais, cancela timers e
   * animações, solta o ResizeObserver, tira o `<style>` e esvazia o `root`.
   * O transporte não é fechado aqui (quem o criou chama `transport.close()`).
   */
  public destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.tour?.destroy();
    this.tour = null;
    this.abort.abort();
    this.resizeObserver?.disconnect();
    this.resizeObserver = null;
    this.stopTween();
    if (this.raf !== null) cancelAnimationFrame(this.raf);
    this.raf = null;
    for (const timer of [this.autosaveTimer, this.saveTimer, this.sessionTimer, this.toastTimer]) {
      if (timer) window.clearTimeout(timer);
    }
    this.saveTimer = this.sessionTimer = null;
    this.styleEl?.remove();
    this.styleEl = null;
    this.session?.free();
    this.session = null;
    this.root.innerHTML = '';
  }
}

/** Monta o Workbench em `root` e inicia o rastreamento de mouse. */
export function mountWorkbench(root: HTMLElement, transport: HostTransport, wasm?: WasmApi): Workbench {
  const wb = new Workbench(root, transport, wasm);
  wb.attachMouseTracking();
  return wb;
}
