/* tslint:disable */
/* eslint-disable */

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
 * Executa a formatação canônica de código do documento DBML.
 */
export function format_dbml(source: string): string;

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
    readonly apply_command: (a: number, b: number, c: any) => [number, number, number];
    readonly compute_layout: (a: number, b: number, c: any) => [number, number, number];
    readonly compute_scene: (a: number, b: number, c: any) => [number, number, number];
    readonly format_dbml: (a: number, b: number) => [number, number];
    readonly parse_and_analyze: (a: number, b: number) => [number, number, number];
    readonly swap_dbml_lines: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
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
