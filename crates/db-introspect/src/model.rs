//! Neutral, driver-independent description of a database catalog.

/// Whole catalog read from a database.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Introspected {
    /// Tables in catalog order (sorted by name for determinism).
    pub tables: Vec<Table>,
    /// Foreign keys, one per constraint.
    pub refs: Vec<ForeignKey>,
    /// Enum types (PostgreSQL, MySQL inline enums are expanded by their introspector).
    pub enums: Vec<EnumType>,
    /// Non-fatal problems (unsupported objects, unknown types).
    pub warnings: Vec<String>,
}

/// A table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// Schema name, when the database has schemas.
    pub schema: Option<String>,
    /// Table name.
    pub name: String,
    /// Columns in declaration order.
    pub columns: Vec<Column>,
    /// Indexes that are not already expressed as a column flag.
    pub indexes: Vec<Index>,
    /// Table comment.
    pub comment: Option<String>,
}

/// A column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// Column name.
    pub name: String,
    /// Declared type as the database reports it.
    pub ty: String,
    /// `NOT NULL`.
    pub not_null: bool,
    /// Part of a single-column primary key.
    pub primary_key: bool,
    /// Single-column unique constraint.
    pub unique: bool,
    /// Auto-increment / identity.
    pub auto_increment: bool,
    /// Default as raw SQL text.
    pub default: Option<String>,
    /// Column comment.
    pub comment: Option<String>,
    /// SQL CHECK expression, when representable in DBML.
    pub check: Option<String>,
}

/// An index or composite key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    /// Index name.
    pub name: Option<String>,
    /// Indexed columns, in order.
    pub columns: Vec<String>,
    /// Unique index.
    pub unique: bool,
    /// Composite primary key.
    pub primary_key: bool,
}

/// A foreign key constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKey {
    /// Referencing schema.
    pub from_schema: Option<String>,
    /// Referencing table.
    pub from_table: String,
    /// Referencing columns.
    pub from_cols: Vec<String>,
    /// Referenced schema.
    pub to_schema: Option<String>,
    /// Referenced table.
    pub to_table: String,
    /// Referenced columns.
    pub to_cols: Vec<String>,
    /// `ON DELETE` action in DBML spelling (`cascade`, `set null`, ...).
    pub on_delete: Option<String>,
    /// `ON UPDATE` action in DBML spelling.
    pub on_update: Option<String>,
}

/// An enum type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumType {
    /// Schema name.
    pub schema: Option<String>,
    /// Enum name.
    pub name: String,
    /// Values in order.
    pub values: Vec<String>,
}
