//! Dialect-neutral intermediate representation (IR) produced by the parser.

use std::borrow::Cow;

/// Identifier or literal text, borrowed from the source unless unescaping was required.
pub(crate) type Text<'a> = Cow<'a, str>;

/// Non-fatal problem found while importing (unsupported or malformed construct).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImportWarning {
    /// 1-based line of the offending token.
    pub line: u32,
    /// 1-based column of the offending token.
    pub column: u32,
    /// Human readable description.
    pub message: String,
}

/// Optionally schema-qualified object name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Name<'a> {
    pub(crate) schema: Option<Text<'a>>,
    pub(crate) name: Text<'a>,
}

/// Referential action of a foreign key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FkAction {
    NoAction,
    Restrict,
    Cascade,
    SetNull,
    SetDefault,
}

/// Foreign key (inline, table-level, or added by `ALTER TABLE`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fk<'a> {
    pub(crate) name: Option<Text<'a>>,
    pub(crate) from_cols: Vec<Text<'a>>,
    pub(crate) to_table: Name<'a>,
    /// Empty when the target columns were omitted (implicit primary key).
    pub(crate) to_cols: Vec<Text<'a>>,
    pub(crate) on_delete: Option<FkAction>,
    pub(crate) on_update: Option<FkAction>,
    pub(crate) line: u32,
    pub(crate) col: u32,
}

/// Parsed `DEFAULT` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DefaultExpr {
    Null,
    Bool(bool),
    /// Numeric literal as written (sign included).
    Number(String),
    /// String literal contents.
    Str(String),
    /// Any other expression, rendered back to SQL text.
    Raw(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Column<'a> {
    pub(crate) name: Text<'a>,
    /// Type as written, words joined by single spaces (`character varying(255)`, `int[]`).
    /// Empty when the dialect allows an untyped column (SQLite).
    pub(crate) ty: String,
    pub(crate) not_null: bool,
    pub(crate) default: Option<DefaultExpr>,
    pub(crate) unique: bool,
    pub(crate) primary_key: bool,
    pub(crate) auto_increment: bool,
    pub(crate) references: Option<Fk<'a>>,
    pub(crate) enum_values: Option<Vec<String>>,
    pub(crate) comment: Option<String>,
    pub(crate) check: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TableConstraint<'a> {
    PrimaryKey(Vec<Text<'a>>),
    Unique {
        name: Option<Text<'a>>,
        cols: Vec<Text<'a>>,
    },
    ForeignKey(Fk<'a>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Table<'a> {
    pub(crate) name: Name<'a>,
    pub(crate) columns: Vec<Column<'a>>,
    pub(crate) constraints: Vec<TableConstraint<'a>>,
    pub(crate) comment: Option<String>,
    pub(crate) line: u32,
    pub(crate) col: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnumType<'a> {
    pub(crate) name: Name<'a>,
    pub(crate) values: Vec<String>,
}

/// Column or expression of an index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IndexCol<'a> {
    Column(Text<'a>),
    Expr(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Index<'a> {
    pub(crate) name: Option<Text<'a>>,
    pub(crate) table: Name<'a>,
    pub(crate) cols: Vec<IndexCol<'a>>,
    pub(crate) unique: bool,
    pub(crate) line: u32,
    pub(crate) col: u32,
}

/// Constraint added by `ALTER TABLE ... ADD`, resolved against tables later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AlterConstraint<'a> {
    pub(crate) table: Name<'a>,
    pub(crate) constraint: TableConstraint<'a>,
    pub(crate) line: u32,
    pub(crate) col: u32,
}

/// `COMMENT ON TABLE` (`column == None`) or `COMMENT ON COLUMN`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Comment<'a> {
    pub(crate) table: Name<'a>,
    pub(crate) column: Option<Text<'a>>,
    pub(crate) text: String,
}

/// Everything extracted from one SQL script.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Ir<'a> {
    pub(crate) tables: Vec<Table<'a>>,
    pub(crate) enums: Vec<EnumType<'a>>,
    pub(crate) indexes: Vec<Index<'a>>,
    pub(crate) alter_constraints: Vec<AlterConstraint<'a>>,
    pub(crate) comments: Vec<Comment<'a>>,
    pub(crate) warnings: Vec<ImportWarning>,
}

impl Ir<'_> {
    pub(crate) fn warn(&mut self, line: u32, column: u32, message: impl Into<String>) {
        self.warnings.push(ImportWarning {
            line,
            column,
            message: message.into(),
        });
    }
}
