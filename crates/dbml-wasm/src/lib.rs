#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! WebAssembly bindings exposing the DB Modeling Rust core (CST parser, semantic
//! analysis, minimal text patcher, clustered auto-layout, and orthogonal routing).

pub mod line_index;
pub mod types;

use line_index::LineIndex;
use schema::Schema;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use types::{
    AnalysisResult, Command, DiagnosticSeverity, GroupBox, LayoutResult, NodeCoordinate,
    RoutedEdge, SceneGeometry, TableVisual, TextPatch, WasmDiagnostic,
};
use wasm_bindgen::prelude::*;

pub use layout::{CARD_W, GROUP_PAD, HEADER_H, ROW_H, TABLE_COLORS, card_h};

/// Serializa estruturas para `JsValue` garantindo que `HashMap` vire objeto JSON (`{}`) e não `Map`.
fn to_js_json<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    value
        .serialize(&serializer)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Cor padrão de um grupo sem `color`: paleta fixa pela posição do grupo (idêntico a `canvas.rs`).
#[must_use]
pub fn group_color(schema: &Schema, group: &str) -> String {
    let i = schema
        .groups
        .iter()
        .position(|g| g.name == group)
        .unwrap_or(0);
    TABLE_COLORS[i % TABLE_COLORS.len()].to_string()
}

/// Extrai nomes de `Enum` definidos no código DBML (idêntico a `crates/app/src/shell/mod.rs`).
#[must_use]
pub fn extract_enums_from_dbml(text: &str) -> Vec<String> {
    let mut enums = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Enum ") {
            let name = rest
                .split(|c: char| c.is_whitespace() || c == '{')
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() && !enums.contains(&name.to_string()) {
                enums.push(name.to_string());
            }
        }
    }
    enums
}

/// Converte um diagnóstico do `dbml_analysis` para a representação do Wasm.
fn convert_diagnostic(
    diag: &dbml_analysis::Diagnostic,
    line_index: &LineIndex,
    text: &str,
) -> WasmDiagnostic {
    let range = line_index.byte_range_to_range(text, diag.range.clone());
    let severity = match diag.severity {
        dbml_analysis::Severity::Error => DiagnosticSeverity::Error,
        dbml_analysis::Severity::Warning => DiagnosticSeverity::Warning,
        dbml_analysis::Severity::Info => DiagnosticSeverity::Info,
    };

    WasmDiagnostic {
        line: range.start.line as usize + 1,
        byte_offset: diag.range.start,
        range,
        severity,
        message: diag.message.clone(),
    }
}

/// Calcula o patch textual mínimo comparando o texto original e o novo texto gerado.
/// Localiza o prefixo comum e o sufixo comum para gerar a edição mais pontual possível.
#[must_use]
pub fn compute_minimal_patch(original: &str, updated: &str, line_index: &LineIndex) -> TextPatch {
    if original == updated {
        let empty_pos = line_index.offset_to_position(original, original.len());
        return TextPatch {
            range: line_index::Range {
                start: empty_pos,
                end: empty_pos,
            },
            byte_start: original.len(),
            byte_end: original.len(),
            new_text: String::new(),
            updated_source: Some(updated.to_string()),
            created_table: None,
            created_column: None,
            renamed_table: None,
            should_relayout: false,
        };
    }

    let orig_bytes = original.as_bytes();
    let upd_bytes = updated.as_bytes();

    // 1. Encontra o prefixo comum mais longo a nível de bytes
    let mut prefix_len = 0;
    while prefix_len < orig_bytes.len()
        && prefix_len < upd_bytes.len()
        && orig_bytes[prefix_len] == upd_bytes[prefix_len]
    {
        prefix_len += 1;
    }

    // Garante que o prefixo termine em uma fronteira válida de caractere UTF-8 em ambas as strings
    while prefix_len > 0
        && (!original.is_char_boundary(prefix_len) || !updated.is_char_boundary(prefix_len))
    {
        prefix_len -= 1;
    }

    // 2. Encontra o sufixo comum mais longo a nível de bytes
    let mut orig_suffix = orig_bytes.len();
    let mut upd_suffix = upd_bytes.len();

    while orig_suffix > prefix_len
        && upd_suffix > prefix_len
        && orig_bytes[orig_suffix - 1] == upd_bytes[upd_suffix - 1]
    {
        orig_suffix -= 1;
        upd_suffix -= 1;
    }

    // Garante que o sufixo inicie em uma fronteira válida de caractere UTF-8 em ambas as strings
    while orig_suffix < orig_bytes.len() && !original.is_char_boundary(orig_suffix) {
        orig_suffix += 1;
    }
    while upd_suffix < upd_bytes.len() && !updated.is_char_boundary(upd_suffix) {
        upd_suffix += 1;
    }

    let new_text = updated[prefix_len..upd_suffix].to_string();
    let range = line_index.byte_range_to_range(original, prefix_len..orig_suffix);

    TextPatch {
        range,
        byte_start: prefix_len,
        byte_end: orig_suffix,
        new_text,
        updated_source: Some(updated.to_string()),
        created_table: None,
        created_column: None,
        renamed_table: None,
        should_relayout: false,
    }
}

/// Auto-layout oficial compartilhado com `crates/app/src/canvas.rs` via `layout::compute_diagram_positions`.
/// Respeita clusters de `TableGroup` via `layout::clustered` ou camadas via `layout::layered`.
#[must_use]
pub fn auto_layout_schema(schema: &Schema) -> HashMap<String, NodeCoordinate> {
    let col_counts: Vec<usize> = schema.tables.iter().map(|t| t.columns.len()).collect();
    let idx = |n: &str| schema.tables.iter().position(|t| t.name == n);
    let edges: Vec<_> = schema
        .refs
        .iter()
        .filter_map(|r| Some((idx(&r.from.table)?, idx(&r.to.table)?)))
        .collect();
    let clusters = (!schema.groups.is_empty()).then(|| {
        let mut c: Vec<Vec<usize>> = schema
            .groups
            .iter()
            .map(|g| g.tables.iter().filter_map(|t| idx(t)).collect())
            .collect();
        c.push(
            (0..schema.tables.len())
                .filter(|&i| schema.group_of(&schema.tables[i].name).is_none())
                .collect(),
        );
        c
    });
    let pos = layout::compute_diagram_positions(&col_counts, &edges, clusters.as_deref());
    schema
        .tables
        .iter()
        .zip(pos)
        .map(|(t, p)| (t.name.clone(), NodeCoordinate { x: p.x, y: p.y }))
        .collect()
}

/// Gera a string de path SVG (`M ... L ... Q ...`) para uma polilinha ortogonal com cantos arredondados
/// em coordenadas de mundo (`z = 1.0`, raio `8.0`), usando a matemática exata de `polyline()` de `canvas.rs`.
#[must_use]
pub fn build_rounded_svg_path(pts: &[layout::Pos]) -> String {
    let Some(first) = pts.first() else {
        return String::new();
    };
    if pts.len() == 1 {
        return format!("M {:.2} {:.2}", first.x, first.y);
    }
    let mut d = format!("M {:.2} {:.2}", first.x, first.y);
    for w in pts.windows(3) {
        let (p, c, n) = (w[0], w[1], w[2]);
        let (l1, l2) = (
            (c.x - p.x).abs() + (c.y - p.y).abs(),
            (n.x - c.x).abs() + (n.y - c.y).abs(),
        );
        let r = 8.0_f32.min(l1 / 2.0).min(l2 / 2.0);
        if r < 0.5 {
            let _ = write!(d, " L {:.2} {:.2}", c.x, c.y);
            continue;
        }
        let d_in = ((c.x - p.x) / l1.max(0.001), (c.y - p.y) / l1.max(0.001));
        let d_out = ((n.x - c.x) / l2.max(0.001), (n.y - c.y) / l2.max(0.001));
        let start_corner = (c.x - d_in.0 * r, c.y - d_in.1 * r);
        let end_corner = (c.x + d_out.0 * r, c.y + d_out.1 * r);
        let _ = write!(
            d,
            " L {:.2} {:.2} Q {:.2} {:.2} {:.2} {:.2}",
            start_corner.0, start_corner.1, c.x, c.y, end_corner.0, end_corner.1
        );
    }
    if let Some(last) = pts.last() {
        let _ = write!(d, " L {:.2} {:.2}", last.x, last.y);
    }
    d
}

/// Calcula toda a geometria da cena (posições, grupos, tabelas, arestas ortogonais roteadas).
#[must_use]
#[expect(
    clippy::implicit_hasher,
    reason = "Concrete HashMap<String, NodeCoordinate> keeps Option::None callers ergonomic without turbofish"
)]
pub fn compute_scene_internal(
    schema: &Schema,
    provided_positions: Option<HashMap<String, NodeCoordinate>>,
) -> SceneGeometry {
    let mut pos = auto_layout_schema(schema);
    if let Some(custom) = provided_positions {
        for (k, v) in custom {
            if schema.table(&k).is_some() {
                pos.insert(k, v);
            }
        }
    }

    // Colunas que são origem de alguma relação (FK)
    let fks: HashSet<(&str, &str)> = schema
        .refs
        .iter()
        .map(|r| (r.from.table.as_str(), r.from.column.as_str()))
        .collect();

    // Metadados visuais de cada tabela
    let mut tables = Vec::with_capacity(schema.tables.len());
    for t in &schema.tables {
        let p = pos
            .get(&t.name)
            .copied()
            .unwrap_or(NodeCoordinate { x: 0.0, y: 0.0 });
        let group = schema.group_of(&t.name).map(|g| g.name.clone());
        let tint = t
            .color
            .clone()
            .or_else(|| schema.group_of(&t.name).and_then(|gr| gr.color.clone()))
            .or_else(|| {
                schema
                    .group_of(&t.name)
                    .map(|gr| group_color(schema, &gr.name))
            });
        let fk_columns = t
            .columns
            .iter()
            .filter(|c| fks.contains(&(t.name.as_str(), c.name.as_str())))
            .map(|c| c.name.clone())
            .collect();

        tables.push(TableVisual {
            name: t.name.clone(),
            x: p.x,
            y: p.y,
            w: CARD_W,
            h: card_h(t.columns.len()),
            tint,
            group,
            fk_columns,
        });
    }

    // Molduras de grupo (caixas que envolvem as tabelas do grupo)
    let mut groups = Vec::new();
    for gr in &schema.groups {
        let members: Vec<_> = gr
            .tables
            .iter()
            .filter_map(|t| Some((pos.get(t)?, schema.table(t)?)))
            .collect();
        if members.is_empty() {
            continue;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (p, t) in &members {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x + CARD_W);
            y1 = y1.max(p.y + card_h(t.columns.len()));
        }
        let pad = GROUP_PAD - 8.0;
        let color = gr
            .color
            .clone()
            .unwrap_or_else(|| group_color(schema, &gr.name));
        groups.push(GroupBox {
            name: gr.name.clone(),
            x: x0 - pad,
            y: y0 - pad - 8.0,
            w: x1 - x0 + 2.0 * pad,
            h: y1 - y0 + 2.0 * pad + 8.0,
            color,
            count: members.len(),
            tables: gr.tables.clone(),
        });
    }

    // Arestas roteadas ortogonalmente com desvio de obstáculos
    let mut edges = Vec::new();
    for r in &schema.refs {
        let Some(ft) = schema.table(&r.from.table) else {
            continue;
        };
        let Some(tt) = schema.table(&r.to.table) else {
            continue;
        };
        let Some(fc) = ft.columns.iter().position(|c| c.name == r.from.column) else {
            continue;
        };
        let Some(tc) = tt.columns.iter().position(|c| c.name == r.to.column) else {
            continue;
        };
        let p_a = pos[&ft.name];
        let p_b = pos[&tt.name];
        let ay = p_a.y + HEADER_H + 4.0 + ROW_H * (fc as f32 + 0.5);
        let by = p_b.y + HEADER_H + 4.0 + ROW_H * (tc as f32 + 0.5);

        // Lado de saída/entrada: +1 borda direita, -1 esquerda
        let (sa, sb) = if p_a.x + CARD_W < p_b.x {
            (1.0, -1.0)
        } else if p_b.x + CARD_W < p_a.x {
            (-1.0, 1.0)
        } else {
            (1.0, 1.0)
        };
        let ax = if sa > 0.0 { p_a.x + CARD_W } else { p_a.x };
        let bx = if sb > 0.0 { p_b.x + CARD_W } else { p_b.x };

        let obstacles: Vec<layout::Rect> = schema
            .tables
            .iter()
            .filter(|t| t.name != ft.name && t.name != tt.name)
            .map(|t| {
                let p = pos[&t.name];
                layout::Rect {
                    x: p.x,
                    y: p.y,
                    w: CARD_W,
                    h: card_h(t.columns.len()),
                }
            })
            .collect();

        let route_pts = layout::route(
            layout::Pos { x: ax, y: ay },
            sa,
            layout::Pos { x: bx, y: by },
            sb,
            &obstacles,
            24.0,
        );
        let svg_path = build_rounded_svg_path(&route_pts);
        let points = route_pts
            .into_iter()
            .map(|p| NodeCoordinate { x: p.x, y: p.y })
            .collect();

        let (start_label, end_label, cardinality) = match r.kind {
            schema::RefKind::ManyToOne => ("N", "1", "N:1"),
            schema::RefKind::OneToMany => ("1", "N", "1:N"),
            schema::RefKind::OneToOne => ("1", "1", "1:1"),
            schema::RefKind::ManyToMany => ("N", "N", "N:N"),
        };

        edges.push(RoutedEdge {
            from_table: r.from.table.clone(),
            from_col: r.from.column.clone(),
            to_table: r.to.table.clone(),
            to_col: r.to.column.clone(),
            ax,
            ay,
            bx,
            by,
            sa,
            sb,
            points,
            svg_path,
            start_label: start_label.to_string(),
            start_label_pos: NodeCoordinate {
                x: ax + sa * 10.0 - 4.0,
                y: ay - 15.0,
            },
            end_label: end_label.to_string(),
            end_label_pos: NodeCoordinate {
                x: bx + sb * 10.0 - 4.0,
                y: by - 15.0,
            },
            cardinality: cardinality.to_string(),
        });
    }

    SceneGeometry {
        positions: pos,
        tables,
        groups,
        edges,
    }
}

/// Analisa o código-fonte DBML e devolve a projeção semântica (`Schema`), diagnósticos e `Enum`s.
///
/// # Errors
///
/// Returns a [`JsValue`] error if JSON serialization fails.
#[wasm_bindgen]
pub fn parse_and_analyze(source: &str) -> Result<JsValue, JsValue> {
    let schema = dbml_analysis::analyze(source);
    let raw_diagnostics = dbml_analysis::lint(source);
    let line_index = LineIndex::new(source);

    let diagnostics: Vec<WasmDiagnostic> = raw_diagnostics
        .iter()
        .map(|d| convert_diagnostic(d, &line_index, source))
        .collect();
    let enums = extract_enums_from_dbml(source);

    let output = AnalysisResult {
        schema,
        diagnostics,
        enums,
    };

    to_js_json(&output)
}

/// Aplica um comando semântico e gera o patch textual mínimo com metadados de UI.
///
/// # Errors
///
/// Returns a [`JsValue`] error if `command_js` cannot be deserialized or serialization fails.
#[wasm_bindgen]
pub fn apply_command(source: &str, command_js: JsValue) -> Result<JsValue, JsValue> {
    let command: Command = serde_wasm_bindgen::from_value(command_js)
        .map_err(|e| JsValue::from_str(&format!("Invalid command: {e}")))?;

    let line_index = LineIndex::new(source);
    let mut created_table = None;
    let mut created_column = None;
    let mut renamed_table = None;
    let mut should_relayout = false;

    let new_source = match command {
        Command::AddTable { name } => {
            let table_name =
                name.unwrap_or_else(|| dbml_edit::unique_table_name(source, "new_table"));
            created_table = Some(table_name.clone());
            dbml_edit::add_table(source, &table_name)
        }
        Command::DeleteTable { name } => dbml_edit::delete_table(source, &name),
        Command::DeleteTables { names } => names.iter().fold(source.to_string(), |acc, t| {
            dbml_edit::delete_table(&acc, t)
        }),
        Command::RenameTable { old_name, new_name } => {
            if old_name != new_name {
                renamed_table = Some((old_name.clone(), new_name.clone()));
            }
            dbml_edit::rename_table(source, &old_name, &new_name)
        }
        Command::AddColumn { table, name, ty } => {
            let col_name =
                name.unwrap_or_else(|| dbml_edit::unique_column_name(source, &table, "column"));
            let col_ty = ty.unwrap_or_else(|| "varchar(255)".to_string());
            created_column = Some((table.clone(), col_name.clone()));
            dbml_edit::add_column_text(source, &table, &col_name, &col_ty).ok_or_else(|| {
                JsValue::from_str(&format!("Table '{table}' not found when adding column"))
            })?
        }
        Command::DeleteColumn { table, column } => {
            dbml_edit::delete_column(source, &table, &column).ok_or_else(|| {
                JsValue::from_str(&format!("Column '{column}' not found in table '{table}'"))
            })?
        }
        Command::RenameColumn {
            table,
            old_name,
            new_name,
        } => dbml_edit::rename_column(source, &table, &old_name, &new_name).ok_or_else(|| {
            JsValue::from_str(&format!("Column '{old_name}' not found in table '{table}'"))
        })?,
        Command::SetColumnType { table, column, ty } => {
            dbml_edit::set_column_type(source, &table, &column, &ty).ok_or_else(|| {
                JsValue::from_str(&format!("Column '{column}' not found in table '{table}'"))
            })?
        }
        Command::SetColumnFlag {
            table,
            column,
            flag,
            value,
        } => {
            dbml_edit::set_column_flag(source, &table, &column, &flag, value).ok_or_else(|| {
                JsValue::from_str(&format!(
                    "Could not toggle flag '{flag}' on column '{column}' in table '{table}'"
                ))
            })?
        }
        Command::AddRef {
            from_table,
            from_col: _,
            to_table,
            to_col: _,
        } => dbml_edit::link_tables(source, &from_table, &to_table),
        Command::DeleteRef {
            from_table,
            from_col,
            to_table,
            to_col,
        } => dbml_edit::delete_ref(source, (&from_table, &from_col), (&to_table, &to_col)),
        Command::GroupTables { name, tables } => {
            should_relayout = true;
            let group_name = name.unwrap_or_else(|| dbml_edit::next_group_name(source));
            let table_refs: Vec<&str> = tables.iter().map(std::string::String::as_str).collect();
            dbml_edit::group_tables(source, &table_refs, &group_name)
        }
        Command::UngroupTable { table } => {
            should_relayout = true;
            dbml_edit::ungroup_table(source, &table)
        }
        Command::UngroupTables { tables } => {
            should_relayout = true;
            tables.iter().fold(source.to_string(), |acc, t| {
                dbml_edit::ungroup_table(&acc, t)
            })
        }
        Command::SetTableColor { table, color } => {
            dbml_edit::set_table_color(source, &table, color.as_deref()).ok_or_else(|| {
                JsValue::from_str(&format!("Table '{table}' not found when setting color"))
            })?
        }
    };

    let mut patch = compute_minimal_patch(source, &new_source, &line_index);
    patch.created_table = created_table;
    patch.created_column = created_column;
    patch.renamed_table = renamed_table;
    patch.should_relayout = should_relayout;

    to_js_json(&patch)
}

/// Move uma linha para cima ou para baixo (`Alt+Up` / `Alt+Down`) preservando terminadores CRLF/LF.
///
/// # Errors
///
/// Returns a [`JsValue`] error if JSON serialization fails.
#[wasm_bindgen]
pub fn swap_dbml_lines(source: &str, line: usize, up: bool) -> Result<JsValue, JsValue> {
    let has_crlf = source.contains("\r\n");
    let normalized = if has_crlf {
        source.replace("\r\n", "\n")
    } else {
        source.to_string()
    };

    let updated = match dbml_edit::swap_lines(&normalized, line, up) {
        Some((range, replacement)) => {
            let mut full = format!(
                "{}{}{}",
                &normalized[..range.start],
                replacement,
                &normalized[range.end..]
            );
            if has_crlf {
                full = full.replace('\n', "\r\n");
            }
            full
        }
        None => source.to_string(),
    };

    let line_index = LineIndex::new(source);
    let patch = compute_minimal_patch(source, &updated, &line_index);
    to_js_json(&patch)
}

/// Executa a formatação canônica de código do documento DBML.
#[wasm_bindgen]
#[must_use]
pub fn format_dbml(source: &str) -> String {
    dbml_edit::format(source)
}

/// Executa o algoritmo de auto-layout oficial (`auto_layout_schema`: `layout::layered` ou `layout::clustered`).
///
/// # Errors
///
/// Returns a [`JsValue`] error if JSON serialization fails.
#[wasm_bindgen]
pub fn compute_layout(source: &str, _params_js: JsValue) -> Result<JsValue, JsValue> {
    let schema = dbml_analysis::analyze(source);
    let positions = auto_layout_schema(&schema);
    let result = LayoutResult { positions };
    to_js_json(&result)
}

/// Calcula toda a cena geométrica (posições de tabelas, caixas de grupos, FKs, polilinhas ortogonais SVG).
///
/// # Errors
///
/// Returns a [`JsValue`] error if JSON serialization fails.
#[wasm_bindgen]
pub fn compute_scene(source: &str, positions_js: JsValue) -> Result<JsValue, JsValue> {
    let schema = dbml_analysis::analyze(source);
    let provided: Option<HashMap<String, NodeCoordinate>> =
        if positions_js.is_undefined() || positions_js.is_null() {
            None
        } else {
            serde_wasm_bindgen::from_value(positions_js).ok()
        };

    let scene = compute_scene_internal(&schema, provided);
    to_js_json(&scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_minimal_patch_add_column() {
        let orig = "Table users {\n  id int\n}\n";
        let updated = "Table users {\n  id int\n  name varchar\n}\n";
        let line_index = LineIndex::new(orig);

        let patch = compute_minimal_patch(orig, updated, &line_index);
        assert_eq!(patch.new_text, "  name varchar\n");
        assert_eq!(patch.byte_start, 23);
        assert_eq!(patch.byte_end, 23);

        let mut reconstructed = orig.to_string();
        reconstructed.replace_range(patch.byte_start..patch.byte_end, &patch.new_text);
        assert_eq!(reconstructed, updated);
    }

    #[test]
    fn test_compute_minimal_patch_with_accents() {
        let orig = "Table usuário {\n  id int\n}\n";
        let updated = "Table usuário {\n  id int\n  descrição text\n}\n";
        let line_index = LineIndex::new(orig);

        let patch = compute_minimal_patch(orig, updated, &line_index);
        assert_eq!(patch.new_text, "  descrição text\n");

        let mut reconstructed = orig.to_string();
        reconstructed.replace_range(patch.byte_start..patch.byte_end, &patch.new_text);
        assert_eq!(reconstructed, updated);
    }

    #[test]
    fn test_auto_layout_and_scene_with_groups_and_routes() {
        let sample = include_str!("../../app/assets/sample.dbml");
        let schema = dbml_analysis::analyze(sample);
        let scene = compute_scene_internal(&schema, None);

        assert!(!scene.tables.is_empty());
        assert!(!scene.groups.is_empty());
        assert!(!scene.edges.is_empty());

        let orders = scene.positions["orders"];
        let order_items = scene.positions["order_items"];
        assert!(orders.x < order_items.x);

        for edge in &scene.edges {
            assert!(edge.svg_path.starts_with("M "));
            assert!(edge.svg_path.contains(" L "));
        }
    }

    #[test]
    fn test_extract_enums() {
        let src = "Enum order_status {\n  pending\n  paid\n}\nTable orders {\n  id int\n}\n";
        let enums = extract_enums_from_dbml(src);
        assert_eq!(enums, vec!["order_status".to_string()]);
    }
}
