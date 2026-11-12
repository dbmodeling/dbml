#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Semantic editing operations producing minimal CST-preserving text patches.

mod format;
pub use format::{format, swap_lines};
pub use groups::{group_tables, next_group_name, set_table_color, ungroup_table};
mod groups;
mod model;
pub use model::{
    add_column as add_column_text, add_ref, add_table, delete_column, delete_ref, delete_table,
    link_tables, rename_column, rename_table, set_column_flag, set_column_type, unique_column_name,
    unique_table_name,
};

use dbml_syntax::{SyntaxKind, SyntaxNode, parse};
use rowan::TextRange;

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

/// High-level structural editing command over a DBML document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
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
        Command::AddColumn { table, name, ty } => add_column(text, table, name, ty),
    }
}

fn table_name(t: &SyntaxNode) -> Option<String> {
    t.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::Ident)
        .nth(1) // o primeiro é a palavra-chave `Table`
        .map(|t| t.text().to_string())
}

fn add_column(text: &str, table: &str, name: &str, ty: &str) -> Result<Patch, EditError> {
    let root = parse(text).syntax();
    let t = root
        .children()
        .find(|n| n.kind() == SyntaxKind::Table && table_name(n).as_deref() == Some(table))
        .ok_or_else(|| EditError::TableNotFound(table.into()))?;
    let line = format!("{name} {ty}");

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
        return Ok(Patch {
            range: TextRange::empty(end),
            new_text: format!("\n{indent}{line}"),
        });
    }

    // Tabela vazia (ou só com blocos): insere antes do `}` final.
    let close = t
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|k| k.kind() == SyntaxKind::RBrace);
    let at = close.map_or(t.text_range().end(), |k| k.text_range().start());
    let before_ws = text[..usize::from(at)].ends_with('\n');
    let new_text = if before_ws {
        format!("{indent}{line}\n")
    } else {
        format!("\n{indent}{line}\n")
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
