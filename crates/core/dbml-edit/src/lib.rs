#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Semantic editing operations producing minimal CST-preserving text patches.

mod api;
mod enums;
mod eol;
mod format;
mod groups;
mod indexes;
mod model;
mod table;

pub use api::{
    add_column_preset_text, add_column_text, add_enum, add_enum_value, add_index, add_ref,
    add_table, delete_column, delete_enum, delete_enum_value, delete_index, delete_ref,
    delete_table, format, group_tables, link_tables, rename_column, rename_enum, rename_enum_value,
    rename_table, reorder_enum_value, set_column_check, set_column_default, set_column_flag,
    set_column_note, set_column_type, set_table_color, ungroup_table, update_index, update_ref,
};
pub use enums::unique_enum_name;
pub use eol::LineEnding;
pub use format::swap_lines;
pub use groups::next_group_name;
pub use model::{RefUpdate, unique_column_name, unique_table_name};
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
    /// Atomically creates a junction table between `left_table` and `right_table`
    /// with FK columns matching their PK types, a composite `[pk]`, and both `Ref:` links.
    CreateJunctionTable {
        /// First source table name (bare or `schema.table`).
        left_table: String,
        /// Second source table name (bare or `schema.table`).
        right_table: String,
        /// Desired junction table name (auto-suggested if empty).
        junction_name: String,
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

/// Suggests a unique junction table name for `left_table` and `right_table`
/// (e.g. `users` + `roles` -> `users_roles`, preserving a shared schema prefix).
#[must_use]
pub fn suggest_junction_name(text: &str, left_table: &str, right_table: &str) -> String {
    let left_bare = model::bare_table_name(left_table);
    let right_bare = model::bare_table_name(right_table);
    let left_schema = left_table.rsplit_once('.').map(|(s, _)| s);
    let right_schema = right_table.rsplit_once('.').map(|(s, _)| s);
    let base = match (left_schema, right_schema) {
        (Some(ls), Some(rs)) if ls == rs => format!("{ls}.{left_bare}_{right_bare}"),
        _ => format!("{left_bare}_{right_bare}"),
    };
    unique_table_name(text, &base)
}

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
        Command::CreateJunctionTable {
            left_table,
            right_table,
            junction_name,
        } => create_junction_table_patch(text, left_table, right_table, junction_name),
    }
}

/// Resolves the final unique name that [`Command::CreateJunctionTable`] will create.
#[must_use]
pub fn resolve_junction_name(
    text: &str,
    left_table: &str,
    right_table: &str,
    junction_name: &str,
) -> String {
    let trimmed = junction_name.trim();
    if trimmed.is_empty() {
        suggest_junction_name(text, left_table, right_table)
    } else {
        unique_table_name(text, trimmed)
    }
}

fn create_junction_table_patch(
    text: &str,
    left_table: &str,
    right_table: &str,
    junction_name: &str,
) -> Result<Patch, EditError> {
    let root = parse(text).syntax();
    let left_node = model::find_table(&root, left_table)
        .ok_or_else(|| EditError::TableNotFound(left_table.into()))?;
    let right_node = model::find_table(&root, right_table)
        .ok_or_else(|| EditError::TableNotFound(right_table.into()))?;

    let left_q = model::table_qualified_name(&left_node).unwrap_or_else(|| left_table.to_string());
    let right_q =
        model::table_qualified_name(&right_node).unwrap_or_else(|| right_table.to_string());

    let (left_pk_col, left_pk_ty) =
        model::primary_key_of(&root, &left_q).unwrap_or_else(|| ("id".into(), "int".into()));
    let (right_pk_col, right_pk_ty) =
        model::primary_key_of(&root, &right_q).unwrap_or_else(|| ("id".into(), "int".into()));

    let mut left_fk = format!("{}_id", model::bare_table_name(&left_q));
    let mut right_fk = format!("{}_id", model::bare_table_name(&right_q));
    if left_fk == right_fk {
        left_fk = format!("{left_fk}_1");
        right_fk = format!("{right_fk}_2");
    }

    let resolved_name = resolve_junction_name(text, &left_q, &right_q, junction_name);
    let nl = LineEnding::detect(text).as_str();
    let trim_end = text.trim_end_matches(['\r', '\n']).len();
    let mut new_text = String::new();
    if trim_end > 0 {
        new_text.push_str(nl);
        new_text.push_str(nl);
    }
    let _ = write!(
        new_text,
        "Table {resolved_name} {{{nl}  {left_fk} {left_pk_ty} [not null]{nl}  {right_fk} {right_pk_ty} [not null]{nl}{nl}  indexes {{{nl}    ({left_fk}, {right_fk}) [pk]{nl}  }}{nl}}}{nl}{nl}Ref: {resolved_name}.{left_fk} > {left_q}.{left_pk_col}{nl}Ref: {resolved_name}.{right_fk} > {right_q}.{right_pk_col}{nl}"
    );
    Ok(Patch {
        range: TextRange::new(
            rowan::TextSize::try_from(trim_end).unwrap_or_default(),
            rowan::TextSize::try_from(text.len()).unwrap_or_default(),
        ),
        new_text,
    })
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
    let nl = LineEnding::detect(text).as_str();

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
        // O `\r` do CRLF é trivia colada ao fim da linha: a inserção vem antes dele.
        if text[..usize::from(end)].ends_with('\r') && text[usize::from(end)..].starts_with('\n') {
            end -= rowan::TextSize::from(1);
        }
        let new_text = lines.iter().fold(String::new(), |mut acc, l| {
            let _ = write!(acc, "{nl}{indent}{l}");
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
        let _ = write!(acc, "{indent}{l}{nl}");
        acc
    });
    let new_text = if before_ws {
        body
    } else {
        format!("{nl}{body}")
    };
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

    #[test]
    fn create_junction_table_matches_pk_types_and_preserves_comments() {
        let src = "// schema header\nTable users {\n  id uuid [pk]\n  name text\n}\n\nTable roles {\n  role_id bigint [pk]\n  label varchar\n}\n";
        let cmd = Command::CreateJunctionTable {
            left_table: "users".into(),
            right_table: "roles".into(),
            junction_name: "user_roles".into(),
        };
        let patch = edit(src, &cmd).unwrap();
        let out = patch.apply(src);
        assert!(out.starts_with("// schema header\n"));
        assert!(out.contains("Table user_roles {\n  users_id uuid [not null]\n  roles_id bigint [not null]\n\n  indexes {\n    (users_id, roles_id) [pk]\n  }\n}"));
        assert!(out.contains("Ref: user_roles.users_id > users.id"));
        assert!(out.contains("Ref: user_roles.roles_id > roles.role_id"));
    }

    #[test]
    fn create_junction_table_auto_suggests_unique_name_and_preserves_crlf() {
        let src = "Table users {\r\n  id int [pk]\r\n}\r\n\r\nTable roles {\r\n  id int [pk]\r\n}\r\n\r\nTable users_roles {\r\n  id int [pk]\r\n}\r\n";
        assert_eq!(
            suggest_junction_name(src, "users", "roles"),
            "users_roles_2"
        );
        let cmd = Command::CreateJunctionTable {
            left_table: "users".into(),
            right_table: "roles".into(),
            junction_name: String::new(),
        };
        let out = edit(src, &cmd).unwrap().apply(src);
        assert!(out.contains("Table users_roles_2 {\r\n"));
        assert!(!out.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn create_junction_table_self_reference_disambiguates_fk_columns() {
        let src = "Table users {\n  id int [pk]\n}\n";
        let cmd = Command::CreateJunctionTable {
            left_table: "users".into(),
            right_table: "users".into(),
            junction_name: "user_links".into(),
        };
        let out = edit(src, &cmd).unwrap().apply(src);
        assert!(out.contains("users_id_1 int [not null]"));
        assert!(out.contains("users_id_2 int [not null]"));
        assert!(out.contains("(users_id_1, users_id_2) [pk]"));
    }

    #[test]
    fn create_junction_table_errors_on_missing_table() {
        let src = "Table users {\n  id int [pk]\n}\n";
        let cmd = Command::CreateJunctionTable {
            left_table: "users".into(),
            right_table: "missing".into(),
            junction_name: "users_missing".into(),
        };
        assert_eq!(
            edit(src, &cmd),
            Err(EditError::TableNotFound("missing".into()))
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
