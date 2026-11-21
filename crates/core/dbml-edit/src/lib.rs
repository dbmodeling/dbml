#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Semantic editing operations producing minimal CST-preserving text patches.

mod enums;
pub use enums::{
    add_enum, add_enum_value, delete_enum, delete_enum_value, rename_enum, rename_enum_value,
    reorder_enum_value, unique_enum_name,
};
mod format;
pub use format::{format, swap_lines};
pub use groups::{group_tables, next_group_name, set_table_color, ungroup_table};
mod groups;
mod indexes;
pub use indexes::{add_index, delete_index, set_column_check, set_column_default, update_index};
mod model;
mod table;
pub use model::{
    RefUpdate, add_column as add_column_text, add_column_preset as add_column_preset_text, add_ref,
    add_table, delete_column, delete_ref, delete_table, link_tables, rename_column, rename_table,
    set_column_flag, set_column_type, unique_column_name, unique_table_name, update_ref,
};
pub use table::{duplicate_table, move_column, set_table_note, table_range};

use dbml_syntax::{SyntaxKind, parse};
use rowan::TextRange;
use std::fmt::Write as _;

/// Byte-range text replacement; applying patches is the only way to mutate DBML text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    /// Byte range in the original source text to replace.
    pub range: TextRange,
    /// Replacement text inserted at [`Patch::range`].
    pub new_text: String,
}

impl Patch {
    /// Applies this patch to `text` and returns the updated source string.
    #[must_use]
    pub fn apply(&self, text: &str) -> String {
        let mut s = usize::from(self.range.start()).min(text.len());
        let mut e = usize::from(self.range.end()).min(text.len());
        while !text.is_char_boundary(s) {
            s -= 1;
        }
        while !text.is_char_boundary(e) {
            e -= 1;
        }
        if s > e {
            std::mem::swap(&mut s, &mut e);
        }
        format!("{}{}{}", &text[..s], self.new_text, &text[e..])
    }
}

/// Preset de colunas de boilerplate inseridas de uma vez numa tabela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnPreset {
    /// Coluna `id` do tipo `uuid` com `pk` e `default` `gen_random_uuid()`.
    UuidPk,
    /// `id bigserial [pk]`.
    BigIntPk,
    /// `created_at` e `updated_at` (`timestamp`, `not null`, `default: now()`).
    Timestamps,
    /// `deleted_at timestamp`.
    SoftDelete,
}

impl ColumnPreset {
    /// Todos os presets, na ordem de exibição.
    #[must_use]
    pub const fn all() -> [Self; 4] {
        [
            Self::UuidPk,
            Self::BigIntPk,
            Self::Timestamps,
            Self::SoftDelete,
        ]
    }

    /// Identificador estável usado na ponte Wasm.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::UuidPk => "uuid_pk",
            Self::BigIntPk => "bigint_pk",
            Self::Timestamps => "timestamps",
            Self::SoftDelete => "soft_delete",
        }
    }

    /// Rótulo exibido no Inspector e na command palette.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::UuidPk => "UUID PK",
            Self::BigIntPk => "BigInt PK",
            Self::Timestamps => "Timestamps",
            Self::SoftDelete => "Soft Delete",
        }
    }

    /// Procura um preset pelo [`ColumnPreset::id`].
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::all().into_iter().find(|p| p.id() == id)
    }

    /// Colunas do preset como `(nome, definição)`; a definição é o texto após o nome.
    #[must_use]
    pub const fn columns(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::UuidPk => &[("id", "uuid [pk, default: `gen_random_uuid()`]")],
            Self::BigIntPk => &[("id", "bigserial [pk]")],
            Self::Timestamps => &[
                ("created_at", "timestamp [not null, default: `now()`]"),
                ("updated_at", "timestamp [not null, default: `now()`]"),
            ],
            Self::SoftDelete => &[("deleted_at", "timestamp")],
        }
    }
}

/// High-level structural editing command over a DBML document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Inserts the columns of a [`ColumnPreset`] that are still missing from a table.
    AddColumnPreset {
        /// Target table name.
        table: String,
        /// Preset to apply.
        preset: ColumnPreset,
    },
    /// Inserts a new column at the end of an existing table.
    AddColumn {
        /// Target table name.
        table: String,
        /// New column name.
        name: String,
        /// New column data type.
        ty: String,
    },
}

/// Error returned when a structural [`Command`] cannot be applied.
#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    /// Target table was not found in the DBML document.
    TableNotFound(String),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TableNotFound(table) => write!(f, "table not found: {table}"),
        }
    }
}

impl std::error::Error for EditError {}

/// Computes a minimal [`Patch`] for `cmd` against `text`.
///
/// # Errors
///
/// Returns [`EditError::TableNotFound`] if the target table does not exist.
pub fn edit(text: &str, cmd: &Command) -> Result<Patch, EditError> {
    match cmd {
        Command::AddColumn { table, name, ty } => {
            insert_columns(text, table, &[format!("{name} {ty}")])
        }
        Command::AddColumnPreset { table, preset } => add_column_preset(text, table, *preset),
    }
}

fn add_column_preset(text: &str, table: &str, preset: ColumnPreset) -> Result<Patch, EditError> {
    let root = parse(text).syntax();
    let t =
        model::find_table(&root, table).ok_or_else(|| EditError::TableNotFound(table.into()))?;
    let existing = model::column_names(&t);
    let lines: Vec<String> = preset
        .columns()
        .iter()
        .filter(|(name, _)| !existing.iter().any(|e| e.eq_ignore_ascii_case(name)))
        .map(|(name, def)| format!("{name} {def}"))
        .collect();
    if lines.is_empty() {
        return Ok(Patch {
            range: TextRange::empty(0.into()),
            new_text: String::new(),
        });
    }
    insert_columns(text, table, &lines)
}

fn insert_columns(text: &str, table: &str, lines: &[String]) -> Result<Patch, EditError> {
    let root = parse(text).syntax();
    let t =
        model::find_table(&root, table).ok_or_else(|| EditError::TableNotFound(table.into()))?;

    // Imita a indentação da última coluna; senão 2 espaços.
    let last_col = t
        .children()
        .filter(|n| n.kind() == SyntaxKind::Column)
        .last();
    let indent = last_col
        .as_ref()
        .and_then(rowan::SyntaxNode::prev_sibling_or_token)
        .and_then(rowan::NodeOrToken::into_token)
        .filter(|tok| tok.kind() == SyntaxKind::Whitespace)
        .map_or_else(|| "  ".into(), |tok| tok.text().to_string());

    // Insere logo após a última coluna (antes de comentário de fim de linha, depois dele).
    if let Some(c) = last_col {
        let mut end = c.text_range().end();
        // consome o comentário de fim de linha, que pertence à mesma linha
        let mut next = c.next_sibling_or_token();
        while let Some(e) = next {
            match e.kind() {
                SyntaxKind::Whitespace | SyntaxKind::Comment => {
                    end = e.text_range().end();
                    next = e.next_sibling_or_token();
                }
                _ => break,
            }
        }
        let new_text = lines.iter().fold(String::new(), |mut acc, l| {
            let _ = write!(acc, "\n{indent}{l}");
            acc
        });
        return Ok(Patch {
            range: TextRange::empty(end),
            new_text,
        });
    }

    // Tabela vazia (ou só com blocos): insere antes do `}` final.
    let close = t
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|k| k.kind() == SyntaxKind::RBrace);
    let at = close.map_or(t.text_range().end(), |k| k.text_range().start());
    let before_ws = text[..usize::from(at)].ends_with('\n');
    let body = lines.iter().fold(String::new(), |mut acc, l| {
        let _ = writeln!(acc, "{indent}{l}");
        acc
    });
    let new_text = if before_ws { body } else { format!("\n{body}") };
    Ok(Patch {
        range: TextRange::empty(at),
        new_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(src: &str, table: &str) -> String {
        let cmd = Command::AddColumn {
            table: table.into(),
            name: "email".into(),
            ty: "varchar".into(),
        };
        edit(src, &cmd).unwrap().apply(src)
    }

    #[test]
    fn preserves_comments_and_formatting() {
        let src = "// topo\nTable users {\n    id int [pk] // chave\n    name text\n}\n\n// fim\n";
        insta::assert_snapshot!(add(src, "users"), @r"
        // topo
        Table users {
            id int [pk] // chave
            name text
            email varchar
        }

        // fim
        ");
    }

    #[test]
    fn empty_table() {
        insta::assert_snapshot!(add("Table t {\n}\n", "t"), @r"
        Table t {
          email varchar
        }
        ");
    }

    #[test]
    fn column_before_indexes_block_keeps_block() {
        let src = "Table t {\n  id int\n  indexes {\n    id\n  }\n}\n";
        let out = add(src, "t");
        assert!(out.contains("id int\n  email varchar\n  indexes {"));
    }

    fn preset(src: &str, table: &str, p: ColumnPreset) -> String {
        add_column_preset_text(src, table, p).unwrap()
    }

    #[test]
    fn preset_uuid_pk_snapshot() {
        let src = "Table t {\n  name text\n}\n";
        insta::assert_snapshot!(preset(src, "t", ColumnPreset::UuidPk), @r"
        Table t {
          name text
          id uuid [pk, default: `gen_random_uuid()`]
        }
        ");
    }

    #[test]
    fn preset_bigint_pk_snapshot() {
        let src = "Table t {\n  name text\n}\n";
        insta::assert_snapshot!(preset(src, "t", ColumnPreset::BigIntPk), @r"
        Table t {
          name text
          id bigserial [pk]
        }
        ");
    }

    #[test]
    fn preset_timestamps_single_patch_keeps_indent() {
        let src = "Table t {\n    id int [pk]\n}\n";
        let cmd = Command::AddColumnPreset {
            table: "t".into(),
            preset: ColumnPreset::Timestamps,
        };
        let out = edit(src, &cmd).unwrap().apply(src);
        insta::assert_snapshot!(out, @r"
        Table t {
            id int [pk]
            created_at timestamp [not null, default: `now()`]
            updated_at timestamp [not null, default: `now()`]
        }
        ");
    }

    #[test]
    fn preset_soft_delete_snapshot() {
        insta::assert_snapshot!(preset("Table t {\n  id int\n}\n", "t", ColumnPreset::SoftDelete), @r"
        Table t {
          id int
          deleted_at timestamp
        }
        ");
    }

    #[test]
    fn preset_partial_inserts_only_missing() {
        let src = "Table t {\n  \"Created_At\" timestamp\n}\n";
        let out = preset(src, "t", ColumnPreset::Timestamps);
        assert_eq!(
            out.matches("created_at").count() + out.matches("Created_At").count(),
            1
        );
        assert!(out.contains("updated_at timestamp"));
    }

    #[test]
    fn preset_all_present_is_noop() {
        let src = "Table t {\n  deleted_at timestamp\n}\n";
        assert_eq!(preset(src, "t", ColumnPreset::SoftDelete), src);
    }

    #[test]
    fn preset_empty_table_and_indexes_block() {
        insta::assert_snapshot!(preset("Table t {\n}\n", "t", ColumnPreset::SoftDelete), @r"
        Table t {
          deleted_at timestamp
        }
        ");
        let src = "Table t {\n  a int\n  indexes {\n    a\n  }\n}\n";
        let out = preset(src, "t", ColumnPreset::SoftDelete);
        assert!(out.contains("a int\n  deleted_at timestamp\n  indexes {"));
    }

    #[test]
    fn preset_tab_indent() {
        let out = preset("Table t {\n\ta int\n}\n", "t", ColumnPreset::SoftDelete);
        assert!(out.contains("\n\tdeleted_at timestamp"));
    }

    #[test]
    fn preset_unknown_table() {
        assert_eq!(
            add_column_preset_text("Table t {}", "x", ColumnPreset::UuidPk),
            None
        );
    }

    #[test]
    fn preset_ids_roundtrip() {
        for p in ColumnPreset::all() {
            assert_eq!(ColumnPreset::from_id(p.id()), Some(p));
        }
    }

    #[test]
    fn unknown_table() {
        let cmd = Command::AddColumn {
            table: "x".into(),
            name: "a".into(),
            ty: "int".into(),
        };
        assert_eq!(
            edit("Table t {}", &cmd),
            Err(EditError::TableNotFound("x".into()))
        );
    }

    proptest::proptest! {
        #[test]
        fn patch_apply_never_panics_on_arbitrary_utf8_and_bounds(
            text in ".*",
            a in 0usize..=256,
            b in 0usize..=256,
            replacement in ".*",
        ) {
            let start = rowan::TextSize::try_from(a).unwrap_or(rowan::TextSize::from(0));
            let end = rowan::TextSize::try_from(b).unwrap_or(rowan::TextSize::from(0));
            let (lo, hi) = if start <= end { (start, end) } else { (end, start) };
            let patch = Patch {
                range: rowan::TextRange::new(lo, hi),
                new_text: replacement,
            };
            let _ = patch.apply(&text);
        }
    }
}
