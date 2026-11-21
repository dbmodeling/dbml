/* tslint:disable */
/* eslint-disable */

/**
 * Sessão Wasm: analisa o DBML uma vez por versão de texto e roteia arestas sem reparse.
 *
 * O JS deve chamar `free()` quando descartar a sessão (`Workbench.destroy()`).
 */
export class Session {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Mesmo retorno de `parse_and_analyze(source)`; reanalisa só se `source` mudou.
     *
     * # Errors
     *
     * Returns a [`JsValue`] error if JSON serialization fails.
     */
    analyze(source: string): any;
    /**
     * Mesmo retorno de `compute_scene(source, positions)`, reaproveitando a análise da versão.
     *
     * # Errors
     *
     * Returns a [`JsValue`] error if JSON serialization fails.
     */
    compute_scene(source: string, positions_js: any): any;
    /**
     * Cria uma sessão vazia.
     */
    constructor();
    /**
     * Arestas roteadas (`RoutedEdge[]`) para `positions`, sem reanalisar. Barato o bastante
     * para rodar a cada quadro do arrasto. Exige `analyze`/`compute_scene` prévio.
     *
     * # Errors
     *
     * Returns a [`JsValue`] error if there is no analysed document yet, `positions_js` is not
     * a `{name: {x, y}}` object, or JSON serialization fails.
     */
    route_edges(positions_js: any): any;
    /**
     * Número de análises completas feitas por esta sessão (diagnóstico/testes).
     */
    readonly analysis_count: number;
}

/**
 * Aplica um comando semântico e gera o patch textual mínimo com metadados de UI.
 *
 * # Errors
 *
 * Returns a [`JsValue`] error if `command_js` cannot be deserialized or serialization fails.
 */
export function apply_command(source: string, command_js: any): any;

/**
 * Executa o algoritmo de auto-layout oficial (`auto_layout_schema`: `layout::layered` ou `layout::clustered`).
 *
 * # Errors
 *
 * Returns a [`JsValue`] error if JSON serialization fails.
 */
export function compute_layout(source: string, _params_js: any): any;

/**
 * Calcula toda a cena geométrica (posições de tabelas, caixas de grupos, FKs, polilinhas ortogonais SVG).
 *
 * # Errors
 *
 * Returns a [`JsValue`] error if JSON serialization fails.
 */
export function compute_scene(source: string, positions_js: any): any;

/**
 * Exports DBML `source` to SQL DDL for the requested `dialect`
 * (`postgres`, `mysql`, `sqlite`, `sqlserver`/`mssql`, `oracle`).
 *
 * # Errors
 *
 * Returns an error if `dialect` is not recognized.
 */
export function export_sql_ddl(source: string, dialect: string): string;

/**
 * Executa a formatação canônica de código do documento DBML.
 */
export function format_dbml(source: string): string;

/**
 * Identifies DBML objects that conflict with an import in append mode.
 */
export function import_conflicts(existing: string, incoming: string): string[];

/**
 * Imports SQL DDL written for `dialect` (`postgres`, `mysql`, `sqlite`) into DBML.
 *
 * Returns `{ dbml, warnings: [{ line, column, message }] }`. Unsupported or malformed
 * statements never fail the call; they are reported in `warnings`.
 *
 * # Errors
 *
 * Returns an error if `dialect` is not recognized or serialization fails.
 */
export function import_sql_ddl(sql: string, dialect: string): any;

/**
 * Instala o hook de pânico para imprimir a mensagem e localização no `console.error`
 * em vez de falhar silenciosamente com `RuntimeError: unreachable` (#76).
 */
export function install_panic_hook(): void;

/**
 * Analisa o código-fonte DBML e devolve a projeção semântica (`Schema`), diagnósticos e `Enum`s.
 *
 * # Errors
 *
 * Returns a [`JsValue`] error if JSON serialization fails.
 */
export function parse_and_analyze(source: string): any;

/**
 * Move uma linha para cima ou para baixo (`Alt+Up` / `Alt+Down`) preservando terminadores CRLF/LF.
 *
 * # Errors
 *
 * Returns a [`JsValue`] error if JSON serialization fails.
 */
export function swap_dbml_lines(source: string, line: number, up: boolean): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_session_free: (a: number, b: number) => void;
    readonly apply_command: (a: number, b: number, c: any) => [number, number, number];
    readonly compute_layout: (a: number, b: number, c: any) => [number, number, number];
    readonly compute_scene: (a: number, b: number, c: any) => [number, number, number];
    readonly export_sql_ddl: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly format_dbml: (a: number, b: number) => [number, number];
    readonly import_conflicts: (a: number, b: number, c: number, d: number) => [number, number];
    readonly import_sql_ddl: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly install_panic_hook: () => void;
    readonly parse_and_analyze: (a: number, b: number) => [number, number, number];
    readonly session_analysis_count: (a: number) => number;
    readonly session_analyze: (a: number, b: number, c: number) => [number, number, number];
    readonly session_compute_scene: (a: number, b: number, c: number, d: any) => [number, number, number];
    readonly session_new: () => number;
    readonly session_route_edges: (a: number, b: any) => [number, number, number];
    readonly swap_dbml_lines: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
