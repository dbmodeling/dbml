/**
 * Definições de tipos canônicos para o Canvas Web do DB Modeling.
 * Espelha fielmente os modelos semânticos das crates `schema` e `dbml-wasm`.
 */

export type RefKind = 'ManyToOne' | 'OneToMany' | 'OneToOne' | 'ManyToMany';

export type RefAction = 'no_action' | 'restrict' | 'cascade' | 'set_null' | 'set_default';

export type IndexType = 'b_tree' | 'hash' | 'gin' | 'gist';

export type IndexColumn =
  | { kind: 'column'; value: string }
  | { kind: 'expression'; value: string };

export interface IndexDef {
  columns: IndexColumn[];
  unique: boolean;
  pk: boolean;
  name?: string | null;
  index_type?: IndexType | null;
}

export interface Endpoint {
  schema?: string | null;
  table: string;
  column: string;
}

export interface Ref {
  from: Endpoint;
  to: Endpoint;
  kind: RefKind;
  on_delete?: RefAction | null;
  on_update?: RefAction | null;
}

export interface Column {
  name: string;
  ty: string;
  pk: boolean;
  fk?: boolean;
  not_null: boolean;
  unique: boolean;
  default_value?: string | null;
  check?: string | null;
  note?: string | null;
}

export interface Table {
  schema?: string | null;
  name: string;
  columns: Column[];
  color?: string | null;
  indexes?: IndexDef[];
  note?: string | null;
}

export function qualifiedTableName(t: Pick<Table, 'schema' | 'name'>): string {
  return t.schema ? `${t.schema}.${t.name}` : t.name;
}

export function qualifiedEndpointTable(e: Pick<Endpoint, 'schema' | 'table'>): string {
  return e.schema ? `${e.schema}.${e.table}` : e.table;
}

export interface Group {
  name: string;
  tables: string[];
  color?: string | null;
}

export interface Schema {
  tables: Table[];
  refs: Ref[];
  groups: Group[];
}

export interface Position {
  line: number;
  character: number;
}

export interface Range {
  start: Position;
  end: Position;
}

export interface Diagnostic {
  range: Range;
  severity: 'error' | 'warning' | 'info';
  message: string;
  line?: number;
  byte_offset?: number;
}

export interface TextPatch {
  range: Range;
  byte_start: number;
  byte_end: number;
  new_text: string;
  expected_source?: string;
  expected_file?: string;
  /** Versão do documento em que o patch foi calculado; preenchida pelo transporte, nunca pelo Workbench. */
  base_version?: number;
  updated_source?: string;
  created_table?: string;
  created_column?: [string, string];
  renamed_table?: [string, string];
  created_enum?: string;
  renamed_enum?: [string, string];
  should_relayout?: boolean;
}

export interface NodeCoordinate {
  x: number;
  y: number;
}

export interface RoutedEdge {
  from_table: string;
  from_col: string;
  to_table: string;
  to_col: string;
  ax: number;
  ay: number;
  bx: number;
  by: number;
  sa: number;
  sb: number;
  points: NodeCoordinate[];
  svg_path: string;
  /** Sem estilo: curva automática, ou rota que desvia dos outros cartões (Wasm). */
  auto_path?: string;
  auto_mid?: NodeCoordinate;
  start_label: string;
  start_label_pos: NodeCoordinate;
  end_label: string;
  end_label_pos: NodeCoordinate;
  cardinality: string;
}

export interface GroupBox {
  name: string;
  x: number;
  y: number;
  w: number;
  h: number;
  color: string;
  count: number;
  tables: string[];
}

export interface TableVisual {
  name: string;
  schema?: string | null;
  x: number;
  y: number;
  w: number;
  h: number;
  tint?: string | null;
  group?: string | null;
  fk_columns: string[];
}

export interface EnumVisual {
  name: string;
  x: number;
  y: number;
  w: number;
  h: number;
  values: string[];
  referenced_by: string[];
}

export interface SceneGeometry {
  positions: Record<string, NodeCoordinate>;
  tables: TableVisual[];
  enums?: EnumVisual[];
  groups: GroupBox[];
  edges: RoutedEdge[];
}

export interface EdgeStyle { straight?: boolean; dx?: number; dy?: number }

export function edgeKey(e: Pick<RoutedEdge, 'from_table' | 'from_col' | 'to_table' | 'to_col'>): string {
  return `${e.from_table}.${e.from_col}->${e.to_table}.${e.to_col}`;
}

/** Formato do arquivo sidecar `<schema>.dbml.layout.json` */
export interface SidecarLayout {
  version?: number;
  tables: Record<string, { x: number; y: number }>;
  colors?: Record<string, string>;
  edges?: Record<string, EdgeStyle>;
  viewport?: {
    zoom: number;
    pan_x: number;
    pan_y: number;
  };
}

/** Presets de coluna (ids iguais a `ColumnPreset::id` em `crates/core/dbml-edit`). */
export type ColumnPresetId = 'uuid_pk' | 'bigint_pk' | 'timestamps' | 'soft_delete';

/** Comandos semânticos emitidos pela UI (idênticos ao enum `Command` em `crates/dbml-wasm/src/types.rs`) */
export type Command =
  | { action: 'add_table'; payload: { name?: string } }
  | { action: 'delete_table'; payload: { name: string } }
  | { action: 'delete_tables'; payload: { names: string[] } }
  | { action: 'rename_table'; payload: { old_name: string; new_name: string } }
  | { action: 'add_column'; payload: { table: string; name?: string; ty?: string } }
  | { action: 'add_column_preset'; payload: { table: string; preset: ColumnPresetId } }
  | { action: 'delete_column'; payload: { table: string; column: string } }
  | { action: 'rename_column'; payload: { table: string; old_name: string; new_name: string } }
  | { action: 'set_column_type'; payload: { table: string; column: string; ty: string } }
  | { action: 'set_column_flag'; payload: { table: string; column: string; flag: string; value: boolean } }
  | { action: 'add_ref'; payload: { from_table: string; from_col?: string; to_table: string; to_col?: string } }
  | { action: 'delete_ref'; payload: { from_table: string; from_col: string; to_table: string; to_col: string } }
  | { action: 'create_junction_table'; payload: { left_table: string; right_table: string; junction_name?: string } }
  | {
      action: 'update_ref';
      payload: {
        from_table: string;
        from_col: string;
        to_table: string;
        to_col: string;
        kind: RefKind;
        on_delete?: RefAction | null;
        on_update?: RefAction | null;
      };
    }
  | { action: 'group_tables'; payload: { name?: string; tables: string[] } }
  | { action: 'ungroup_table'; payload: { table: string } }
  | { action: 'ungroup_tables'; payload: { tables: string[] } }
  | { action: 'set_table_color'; payload: { table: string; color?: string | null } }
  | { action: 'set_table_note'; payload: { table: string; note?: string | null } }
  | { action: 'set_column_note'; payload: { table: string; column: string; note?: string | null } }
  | { action: 'add_enum'; payload: { name?: string } }
  | { action: 'delete_enum'; payload: { name: string } }
  | { action: 'rename_enum'; payload: { old_name: string; new_name: string } }
  | { action: 'add_enum_value'; payload: { enum_name: string; value?: string } }
  | { action: 'delete_enum_value'; payload: { enum_name: string; value: string } }
  | { action: 'rename_enum_value'; payload: { enum_name: string; old_value: string; new_value: string } }
  | { action: 'reorder_enum_value'; payload: { enum_name: string; value: string; up: boolean } }
  | { action: 'set_column_default'; payload: { table: string; column: string; default_value?: string | null } }
  | { action: 'set_column_check'; payload: { table: string; column: string; check?: string | null } }
  | { action: 'add_index'; payload: { table: string; index: IndexDef } }
  | { action: 'update_index'; payload: { table: string; index_idx: number; index: IndexDef } }
  | { action: 'delete_index'; payload: { table: string; index_idx: number } };

export interface WorkspaceFileEntry {
  name: string;
  path: string;
  active?: boolean;
}

export interface SessionSnapshot {
  pane?: 'files' | 'schema' | 'problems';
  editorOpen?: boolean;
  zoom?: number;
  pan?: { x: number; y: number };
  selection?: string[];
  selectedEnum?: string | null;
  savedAt?: number;
}

/** Política de TLS para introspecção de banco remoto. */
export type TlsMode = 'auto' | 'verify' | 'require' | 'disable';

/** Interface unificada de comunicação com o hospedeiro (VS Code Webview ou CLI WebSocket) */
/** Campos do modal "Connect to Database" (iguais a `IntrospectRequest` em `crates/cli`). */
export interface IntrospectRequest {
  dialect: 'postgres' | 'mysql' | 'sqlserver' | 'sqlite';
  host: string;
  port: number | null;
  database: string;
  user: string;
  password: string;
  path: string;
  trust_server_certificate: boolean;
  tls?: TlsMode;
  schemas: string[];
}

export interface IntrospectResult {
  dbml: string;
  warnings: string[];
  tables: number;
}

export interface TestConnectionResult {
  server: string;
  schemas: string[];
}

export interface GuideStateMetadata {
  onboardingDone: boolean;
  lastSeenVersion: string | null;
}

export interface DocumentMetadata {
  title?: string;
  files?: WorkspaceFileEntry[];
  session?: SessionSnapshot;
  guideState?: GuideStateMetadata;
  documentId?: string;
  saved?: boolean;
  savedSource?: string;
  autosave?: boolean;
  /** Versão do texto no host (contador monotônico); o transporte a devolve como `base_version` nos patches. */
  version?: number;
}

/** `connecting`: tentando (re)conectar; `online`: canal aberto; `offline`: caiu, aguardando nova tentativa. */
export type ConnectionState = 'connecting' | 'online' | 'offline';

export interface HostTransport {
  checkDatabaseSupport?(): Promise<void>;
  /** Só confere as credenciais e lista os schemas (sem ler tabelas). */
  testConnection?(req: IntrospectRequest, signal?: AbortSignal): Promise<TestConnectionResult>;
  /** Lê o catálogo de um banco (somente leitura) e devolve DBML; ausente quando o host não suporta. */
  introspect?(req: IntrospectRequest, signal?: AbortSignal): Promise<IntrospectResult>;
  sendPatch(patch: TextPatch): Promise<void>;
  saveDocument?(source: string, documentId?: string): Promise<void>;
  saveLayout(layout: SidecarLayout, documentId?: string): Promise<void>;
  exportFile?(filename: string, content: string, isBinary: boolean): Promise<void>;
  openFile?(path: string): Promise<void>;
  createNewFile?(): Promise<void>;
  saveSession?(session: SessionSnapshot): void;
  getSession?(): SessionSnapshot | null;
  saveGuideState?(state: GuideStateMetadata): void;
  onDocumentUpdated(
    callback: (
      source: string,
      layout?: SidecarLayout,
      meta?: DocumentMetadata
    ) => void
  ): void;
  onWorkspaceFilesUpdated?(
    callback: (files: WorkspaceFileEntry[], activeTitle?: string) => void
  ): void;
  onFileChangedExternally?(callback: (newSource: string, documentId?: string) => void): void;
  onError?(callback: (diagnostics: Diagnostic[]) => void): void;
  /** Estado do canal com o host; só transportes que podem cair (WebSocket) o emitem. */
  onConnectionChange?(callback: (state: ConnectionState) => void): void;
  /** Libera sockets, timers e listeners do transporte; ele não deve ser usado depois. */
  close?(): void;
  /** Publica os diagnósticos do Wasm no host (ex.: aba Problems do VS Code). */
  sendDiagnostics?(diagnostics: Diagnostic[]): void;
}

/**
 * Sessão Wasm (`dbml_wasm::session::Session`): analisa o DBML uma vez por versão de texto e
 * roteia arestas sem reparse. Chame `free()` ao descartar.
 */
export interface WasmSession {
  /** Igual a `parse_and_analyze`; reanalisa só se `source` mudou. */
  analyze(source: string): { schema: Schema; diagnostics: Diagnostic[]; enums?: string[] };
  /** Igual a `compute_scene`, reaproveitando a análise da versão atual do texto. */
  compute_scene(source: string, positions: Record<string, NodeCoordinate>): SceneGeometry;
  /** Só as arestas roteadas para `positions` (sem reparse): barato por quadro de arrasto. */
  route_edges(positions: Record<string, NodeCoordinate>): RoutedEdge[];
  /** Quantas análises completas a sessão fez. */
  readonly analysis_count: number;
  free(): void;
}

/** Valor de enum (`schema::EnumValue`). */
export interface EnumValueDef {
  name: string;
  note?: string | null;
}

/** Definição de enum (`schema::EnumDef`). */
export interface EnumDef {
  schema?: string | null;
  name: string;
  values: EnumValueDef[];
}

/** Mudança por chave (`schema::diff::Change`): adicionada, removida ou modificada. */
export type DiffChange<T> =
  | { kind: 'added'; value: T }
  | { kind: 'removed'; value: T }
  | { kind: 'modified'; value: { before: T; after: T } };

/** Propriedades de nível de tabela comparadas pelo diff. */
export interface DiffTableProps {
  color?: string | null;
  alias?: string | null;
  note?: string | null;
}

/** Tabela presente em base e head com diferenças internas. */
export interface DiffTableModified {
  schema?: string | null;
  name: string;
  props?: { before: DiffTableProps; after: DiffTableProps } | null;
  columns: DiffChange<Column>[];
  indexes: DiffChange<IndexDef>[];
}

export type DiffTableChange =
  | { kind: 'added'; value: Table }
  | { kind: 'removed'; value: Table }
  | { kind: 'modified'; value: DiffTableModified };

/** Enum presente em base e head com valores diferentes. */
export interface DiffEnumModified {
  schema?: string | null;
  name: string;
  values: DiffChange<EnumValueDef>[];
}

export type DiffEnumChange =
  | { kind: 'added'; value: EnumDef }
  | { kind: 'removed'; value: EnumDef }
  | { kind: 'modified'; value: DiffEnumModified };

/** Diff estrutural entre dois schemas (`schema::SchemaDiff`). */
export interface SchemaDiff {
  tables: DiffTableChange[];
  enums: DiffEnumChange[];
  refs: DiffChange<Ref>[];
}

/** Contagens agregadas de mudanças atômicas (`schema::DiffSummary`). */
export interface DiffSummary {
  added: number;
  removed: number;
  modified: number;
}

/** Retorno de `diff_dbml`. */
export interface DiffResult {
  diff: SchemaDiff;
  summary: DiffSummary;
}

/** Superfície do módulo Wasm (`crates/dbml-wasm`) usada pelo workbench. */
export interface WasmApi {
  /** Ausente em hosts antigos (ex.: bundle do VS Code): o workbench cai nas funções soltas. */
  Session?: new () => WasmSession;
  /** Reinstancia o módulo Wasm após um trap fatal (#76). */
  reinit?: () => Promise<WasmApi>;
  parse_and_analyze(source: string): { schema: Schema; diagnostics: Diagnostic[]; enums?: string[] };
  compute_scene(source: string, positions: Record<string, NodeCoordinate>): SceneGeometry;
  compute_layout(source: string, options: object): { positions: Record<string, NodeCoordinate> };
  apply_command(source: string, command: Command): TextPatch;
  swap_dbml_lines(source: string, line: number, up: boolean): TextPatch;
  format_dbml(source: string): string;
  export_sql_ddl?(source: string, dialect: string): string;
  export_docs_markdown?(source: string, title: string): string;
  export_docs_html?(source: string, title: string, svg: string): string;
  search_schema?(source: string, query: string): unknown;
  import_sql_ddl?(sql: string, dialect: string): unknown;
  import_conflicts?(source: string, dbml: string): string[];
  /** Diff estrutural base vs. head; lança string `base|head has DBML errors: ...` se um lado for inválido. */
  diff_dbml?(base_source: string, head_source: string): DiffResult;
  /** Sugere nome único de tabela associativa para um relacionamento N:N. */
  suggest_junction_name?(source: string, leftTable: string, rightTable: string): string;
}
