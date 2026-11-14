//! Serializable input and output DTO types for the WebAssembly API.

use crate::line_index::Range;
use schema::Schema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Severity of a diagnostic emitted to the editor or webview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    /// Syntax or semantic error.
    Error,
    /// Non-fatal warning.
    Warning,
    /// Informational hint.
    Info,
}

/// Syntax or semantic diagnostic with LSP-compatible UTF-16 range and 1-based line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmDiagnostic {
    /// UTF-16 LSP range.
    pub range: Range,
    /// Diagnostic severity.
    pub severity: DiagnosticSeverity,
    /// Human-readable message.
    pub message: String,
    /// 1-based line number.
    pub line: usize,
    /// 0-based UTF-8 byte offset.
    pub byte_offset: usize,
}

/// Output returned by `parse_and_analyze`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// Parsed semantic schema.
    pub schema: Schema,
    /// Lint diagnostics found in the document.
    pub diagnostics: Vec<WasmDiagnostic>,
    /// Declared `Enum` names extracted from the document.
    pub enums: Vec<String>,
}

/// Minimal textual patch resulting from a semantic edit command plus UI metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextPatch {
    /// Line/column range to replace in the editor.
    pub range: Range,
    /// Start UTF-8 byte offset.
    pub byte_start: usize,
    /// End UTF-8 byte offset.
    pub byte_end: usize,
    /// Replacement text for the byte range.
    pub new_text: String,
    /// Full updated source text for immediate client sync.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_source: Option<String>,
    /// Newly created table name, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_table: Option<String>,
    /// Newly created `(table, column)` pair, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_column: Option<(String, String)>,
    /// `(old_name, new_name)` pair when a table is renamed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed_table: Option<(String, String)>,
    /// Newly created enum name, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_enum: Option<String>,
    /// `(old_name, new_name)` pair when an enum is renamed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed_enum: Option<(String, String)>,
    /// Whether the operation requires an immediate relayout.
    #[serde(default)]
    pub should_relayout: bool,
}

/// Semantic editing commands emitted by the Canvas or Inspector UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload", rename_all = "snake_case")]
pub enum Command {
    /// Add a new table (auto-naming if `name` is `None`).
    AddTable {
        /// Optional explicit table name.
        name: Option<String>,
    },
    /// Delete a single table by name.
    DeleteTable {
        /// Table name to delete.
        name: String,
    },
    /// Delete multiple tables by name.
    DeleteTables {
        /// Table names to delete.
        names: Vec<String>,
    },
    /// Rename an existing table and cascade references.
    RenameTable {
        /// Current table name.
        old_name: String,
        /// New table name.
        new_name: String,
    },
    /// Add a column to an existing table.
    AddColumn {
        /// Target table name.
        table: String,
        /// Optional column name.
        name: Option<String>,
        /// Optional column type.
        ty: Option<String>,
    },
    /// Delete a column from a table.
    DeleteColumn {
        /// Target table name.
        table: String,
        /// Column name to delete.
        column: String,
    },
    /// Rename a column and update references.
    RenameColumn {
        /// Target table name.
        table: String,
        /// Current column name.
        old_name: String,
        /// New column name.
        new_name: String,
    },
    /// Change the data type of a column.
    SetColumnType {
        /// Target table name.
        table: String,
        /// Target column name.
        column: String,
        /// New data type.
        ty: String,
    },
    /// Toggle a column constraint flag (`pk`, `not null`, `unique`, `fk`).
    SetColumnFlag {
        /// Target table name.
        table: String,
        /// Target column name.
        column: String,
        /// Flag name.
        flag: String,
        /// Desired boolean state.
        value: bool,
    },
    /// Create a foreign-key relationship between two tables/columns.
    AddRef {
        /// Source table name.
        from_table: String,
        /// Source column name (auto-created if empty).
        #[serde(default)]
        from_col: String,
        /// Target table name.
        to_table: String,
        /// Target column name (resolved to primary key if empty).
        #[serde(default)]
        to_col: String,
    },
    /// Delete a foreign-key relationship.
    DeleteRef {
        /// Source table name.
        from_table: String,
        /// Source column name.
        from_col: String,
        /// Target table name.
        to_table: String,
        /// Target column name.
        to_col: String,
    },
    /// Update cardinality and referential actions on an existing relationship.
    UpdateRef {
        /// Source table name.
        from_table: String,
        /// Source column name.
        from_col: String,
        /// Target table name.
        to_table: String,
        /// Target column name.
        to_col: String,
        /// Target cardinality.
        kind: schema::RefKind,
        /// Optional `ON DELETE` referential action.
        #[serde(default)]
        on_delete: Option<schema::RefAction>,
        /// Optional `ON UPDATE` referential action.
        #[serde(default)]
        on_update: Option<schema::RefAction>,
    },
    /// Group tables into a `TableGroup`.
    GroupTables {
        /// Optional group name.
        name: Option<String>,
        /// Tables to include in the group.
        tables: Vec<String>,
    },
    /// Remove a single table from its group.
    UngroupTable {
        /// Table name to ungroup.
        table: String,
    },
    /// Remove multiple tables from their groups.
    UngroupTables {
        /// Table names to ungroup.
        tables: Vec<String>,
    },
    /// Set or clear the `[headercolor: #hex]` of a table.
    SetTableColor {
        /// Target table name.
        table: String,
        /// Optional hex color (`None` removes `headercolor`).
        color: Option<String>,
    },
    /// Add a new `Enum` block (auto-naming if `name` is `None`).
    AddEnum {
        /// Optional explicit enum name.
        name: Option<String>,
    },
    /// Delete an `Enum` block by name.
    DeleteEnum {
        /// Enum name to delete.
        name: String,
    },
    /// Rename an `Enum` block and cascade column types.
    RenameEnum {
        /// Current enum name.
        old_name: String,
        /// New enum name.
        new_name: String,
    },
    /// Add a new value to an `Enum` block.
    AddEnumValue {
        /// Target enum name.
        enum_name: String,
        /// Optional value name (auto-generated if `None`).
        value: Option<String>,
    },
    /// Delete a value from an `Enum` block.
    DeleteEnumValue {
        /// Target enum name.
        enum_name: String,
        /// Value to remove.
        value: String,
    },
    /// Rename a value inside an `Enum` block.
    RenameEnumValue {
        /// Target enum name.
        enum_name: String,
        /// Current value name.
        old_value: String,
        /// New value name.
        new_value: String,
    },
    /// Move an enum value up or down inside its `Enum` block.
    ReorderEnumValue {
        /// Target enum name.
        enum_name: String,
        /// Value to move.
        value: String,
        /// `true` to move up, `false` to move down.
        up: bool,
    },
}

/// Dimensions of a table card for auto-layout computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDimension {
    /// Table name.
    pub name: String,
    /// Card width in world pixels.
    pub width: f32,
    /// Card height in world pixels.
    pub height: f32,
}

/// Optional input parameters for auto-layout computation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LayoutParams {
    /// Explicit table dimensions; falls back to defaults when empty.
    #[serde(default)]
    pub tables: Vec<TableDimension>,
    /// Optional horizontal gap between layers.
    pub gap_x: Option<f32>,
    /// Optional vertical gap between stacked cards.
    pub gap_y: Option<f32>,
    /// Optional outer margin.
    pub margin: Option<f32>,
}

/// Calculated 2D coordinate of a table node.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeCoordinate {
    /// Horizontal world coordinate.
    pub x: f32,
    /// Vertical world coordinate.
    pub y: f32,
}

/// Output of the auto-layout algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutResult {
    /// Map of table name to computed top-left world coordinate.
    pub positions: HashMap<String, NodeCoordinate>,
}

/// Orthogonally routed edge in world coordinates (`z = 1.0`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutedEdge {
    /// Source table name.
    pub from_table: String,
    /// Source column name.
    pub from_col: String,
    /// Target table name.
    pub to_table: String,
    /// Target column name.
    pub to_col: String,
    /// Relationship cardinality kind.
    pub kind: schema::RefKind,
    /// Optional `ON DELETE` referential action.
    pub on_delete: Option<schema::RefAction>,
    /// Optional `ON UPDATE` referential action.
    pub on_update: Option<schema::RefAction>,
    /// Anchor start X coordinate.
    pub ax: f32,
    /// Anchor start Y coordinate.
    pub ay: f32,
    /// Anchor end X coordinate.
    pub bx: f32,
    /// Anchor end Y coordinate.
    pub by: f32,
    /// Exit side direction (`+1.0` right, `-1.0` left).
    pub sa: f32,
    /// Entry side direction (`+1.0` right, `-1.0` left).
    pub sb: f32,
    /// Polyline waypoints in world coordinates.
    pub points: Vec<NodeCoordinate>,
    /// SVG path string (`M ... L ... Q ...`) with 8px rounded corners.
    pub svg_path: String,
    /// Cardinality symbol near the start endpoint.
    pub start_label: String,
    /// World position of `start_label`.
    pub start_label_pos: NodeCoordinate,
    /// Cardinality symbol near the end endpoint.
    pub end_label: String,
    /// World position of `end_label`.
    pub end_label_pos: NodeCoordinate,
    /// Combined cardinality label (e.g., `N:1`).
    pub cardinality: String,
}

/// Bounding box of a `TableGroup` in world coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupBox {
    /// Group name.
    pub name: String,
    /// Left X coordinate.
    pub x: f32,
    /// Top Y coordinate.
    pub y: f32,
    /// Box width.
    pub w: f32,
    /// Box height.
    pub h: f32,
    /// Resolved hex color.
    pub color: String,
    /// Number of member tables.
    pub count: usize,
    /// Member table names.
    pub tables: Vec<String>,
}

/// Computed visual metadata for a table card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableVisual {
    /// Table name.
    pub name: String,
    /// Left X coordinate.
    pub x: f32,
    /// Top Y coordinate.
    pub y: f32,
    /// Card width.
    pub w: f32,
    /// Card height.
    pub h: f32,
    /// Resolved header tint color, if any.
    pub tint: Option<String>,
    /// Containing group name, if any.
    pub group: Option<String>,
    /// Names of foreign-key columns in this table.
    pub fk_columns: Vec<String>,
}

/// Computed visual metadata for an `Enum` card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVisual {
    /// Enum name.
    pub name: String,
    /// Left X coordinate.
    pub x: f32,
    /// Top Y coordinate.
    pub y: f32,
    /// Card width.
    pub w: f32,
    /// Card height.
    pub h: f32,
    /// Ordered enum value names.
    pub values: Vec<String>,
    /// Table names containing columns that reference this enum.
    pub referenced_by: Vec<String>,
}

/// Complete scene geometry calculated to match `crates/app/src/canvas.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneGeometry {
    /// Table and enum (`enum:<name>`) positions.
    pub positions: HashMap<String, NodeCoordinate>,
    /// Visual metadata for each table.
    pub tables: Vec<TableVisual>,
    /// Visual metadata for each enum card.
    #[serde(default)]
    pub enums: Vec<EnumVisual>,
    /// Bounding boxes for each group.
    pub groups: Vec<GroupBox>,
    /// Orthogonally routed relationship edges.
    pub edges: Vec<RoutedEdge>,
}
