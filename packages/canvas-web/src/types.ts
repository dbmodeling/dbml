/**
 * Definições de tipos canônicos para o Canvas Web do DB Modeling.
 * Espelha fielmente os modelos semânticos das crates `schema` e `dbml-wasm`.
 */

export type RefKind = 'ManyToOne' | 'OneToMany' | 'OneToOne' | 'ManyToMany';

export type RefAction = 'no_action' | 'restrict' | 'cascade' | 'set_null' | 'set_default';

export interface Endpoint {
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
}

export interface Table {
  name: string;
  columns: Column[];
  color?: string | null;
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
  updated_source?: string;
  created_table?: string;
  created_column?: [string, string];
  renamed_table?: [string, string];
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
  x: number;
  y: number;
  w: number;
  h: number;
  tint?: string | null;
  group?: string | null;
  fk_columns: string[];
}

export interface SceneGeometry {
  positions: Record<string, NodeCoordinate>;
  tables: TableVisual[];
  groups: GroupBox[];
  edges: RoutedEdge[];
}

/** Formato do arquivo sidecar `<schema>.dbml.layout.json` */
export interface SidecarLayout {
  version?: number;
  tables: Record<string, { x: number; y: number }>;
  colors?: Record<string, string>;
  viewport?: {
    zoom: number;
    pan_x: number;
    pan_y: number;
  };
}

/** Comandos semânticos emitidos pela UI (idênticos ao enum `Command` em `crates/dbml-wasm/src/types.rs`) */
export type Command =
  | { action: 'add_table'; payload: { name?: string } }
  | { action: 'delete_table'; payload: { name: string } }
  | { action: 'delete_tables'; payload: { names: string[] } }
  | { action: 'rename_table'; payload: { old_name: string; new_name: string } }
  | { action: 'add_column'; payload: { table: string; name?: string; ty?: string } }
  | { action: 'delete_column'; payload: { table: string; column: string } }
  | { action: 'rename_column'; payload: { table: string; old_name: string; new_name: string } }
  | { action: 'set_column_type'; payload: { table: string; column: string; ty: string } }
  | { action: 'set_column_flag'; payload: { table: string; column: string; flag: string; value: boolean } }
  | { action: 'add_ref'; payload: { from_table: string; from_col?: string; to_table: string; to_col?: string } }
  | { action: 'delete_ref'; payload: { from_table: string; from_col: string; to_table: string; to_col: string } }
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
  | { action: 'set_table_color'; payload: { table: string; color?: string | null } };

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
}

/** Interface unificada de comunicação com o hospedeiro (VS Code Webview ou CLI WebSocket) */
export interface HostTransport {
  sendPatch(patch: TextPatch): Promise<void>;
  saveLayout(layout: SidecarLayout): Promise<void>;
  exportFile?(filename: string, content: string, isBinary: boolean): Promise<void>;
  openFile?(path: string): Promise<void>;
  createNewFile?(): Promise<void>;
  saveSession?(session: SessionSnapshot): void;
  getSession?(): SessionSnapshot | null;
  onDocumentUpdated(
    callback: (
      source: string,
      layout?: SidecarLayout,
      meta?: { title?: string; files?: WorkspaceFileEntry[]; session?: SessionSnapshot }
    ) => void
  ): void;
  onWorkspaceFilesUpdated?(
    callback: (files: WorkspaceFileEntry[], activeTitle?: string) => void
  ): void;
  onFileChangedExternally?(callback: (newSource: string) => void): void;
  onError?(callback: (diagnostics: Diagnostic[]) => void): void;
}
