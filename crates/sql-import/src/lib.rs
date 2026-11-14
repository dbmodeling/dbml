#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )
)]
//! Tolerant SQL DDL to DBML importer (PostgreSQL, MySQL, SQLite).
//!
//! The crate is a pure function over `&str`: no UI, OS or I/O dependencies (ADR-0005) and it
//! never panics on incomplete input (ADR-0003).

mod dialect;
mod emit;
mod lexer;
mod model;
mod parser;
mod resolve;

pub use dialect::{SqlDialect, UnknownDialect};
pub use model::ImportWarning;

/// Result of [`import_sql_ddl`]: the generated DBML plus everything that could not be imported.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImportOutcome {
    /// Generated DBML text (empty when the script defines no table or enum).
    pub dbml: String,
    /// Skipped or dropped constructs, ordered by position.
    pub warnings: Vec<ImportWarning>,
}

/// Converts SQL DDL written for `dialect` into DBML.
///
/// The function is infallible: unsupported or malformed statements are skipped and reported in
/// [`ImportOutcome::warnings`], and the output is deterministic for a given input.
///
/// ```
/// use sql_import::{SqlDialect, import_sql_ddl};
///
/// let out = import_sql_ddl(
///     "CREATE TABLE users (id serial PRIMARY KEY, email text NOT NULL);",
///     SqlDialect::Postgres,
/// );
/// assert_eq!(
///     out.dbml,
///     "Table users {\n  id integer [pk, increment]\n  email text [not null]\n}\n"
/// );
/// assert!(out.warnings.is_empty());
/// ```
#[must_use]
pub fn import_sql_ddl(sql: &str, dialect: SqlDialect) -> ImportOutcome {
    let tokens = lexer::lex(sql, dialect);
    let ir = parser::parse(&tokens, dialect);
    let resolved = resolve::resolve(ir, dialect);
    let dbml = emit::emit(&resolved, dialect);
    let mut warnings = resolved.warnings;
    warnings.sort_by_key(|w| (w.line, w.column));
    ImportOutcome { dbml, warnings }
}
