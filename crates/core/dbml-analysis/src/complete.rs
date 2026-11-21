//! Autocomplete sensível ao contexto: palavras-chave no topo, tipos depois do nome
//! da coluna, configurações dentro de `[...]` e `tabela.coluna` em referências.

use crate::analyze;

/// Category of a DBML autocompletion suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    /// Top-level DBML keyword (`Table`, `Ref`, `TableGroup`, `Enum`).
    Keyword,
    /// Built-in or user-defined column data type.
    Type,
    /// Bracketed column or table setting (`pk`, `unique`, `not null`, etc.).
    Setting,
    /// Table name in a group or relationship reference.
    Table,
    /// Column name after `table.`.
    Column,
}

/// A single context-aware autocompletion item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Display label shown in the completion menu.
    pub label: String,
    /// Text inserted when accepted (may differ from `label`, e.g., `Ref: `).
    pub insert: String,
    /// Semantic kind of this completion.
    pub kind: CompletionKind,
    /// Short description or type detail shown alongside `label`.
    pub detail: String,
    /// Byte range in the source text replaced by `insert`.
    pub replace: std::ops::Range<usize>,
}

fn item(label: &str, insert: &str, kind: CompletionKind, detail: &str) -> Completion {
    Completion {
        label: label.into(),
        insert: insert.into(),
        kind,
        detail: detail.into(),
        replace: 0..0,
    }
}

const TYPES: [&str; 14] = [
    "int",
    "bigint",
    "smallint",
    "serial",
    "varchar(255)",
    "text",
    "boolean",
    "uuid",
    "timestamp",
    "date",
    "decimal(10,2)",
    "float",
    "json",
    "jsonb",
];

const SETTINGS: [(&str, &str, &str); 8] = [
    ("pk", "pk", "primary key"),
    ("not null", "not null", "required"),
    ("unique", "unique", "unique value"),
    ("increment", "increment", "auto-increment"),
    ("default", "default: ", "default value"),
    ("note", "note: ''", "comment"),
    ("ref", "ref: > ", "foreign key"),
    ("null", "null", "nullable"),
];

/// Retorna sugestões para o cursor em `offset` (bytes). O prefixo já digitado filtra a lista.
#[must_use]
pub fn complete(text: &str, offset: usize) -> Vec<Completion> {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
    // palavra em digitação (inclui `.` para tabela.coluna)
    let word_start = line
        .char_indices()
        .rfind(|&(_, c)| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let word = &line[word_start..];
    let ctx = &line[..word_start];

    let depth = before
        .matches('{')
        .count()
        .saturating_sub(before.matches('}').count());
    let in_brackets = line.matches('[').count() > line.matches(']').count();
    let schema = analyze(text);

    // dentro de `TableGroup nome { ... }`: sugere tabelas que ainda não estão no grupo
    if let Some(members) = group_block(before) {
        let needle = word.to_ascii_lowercase();
        return schema
            .tables
            .iter()
            .filter(|t| {
                let qn = t.qualified_name();
                !members.contains(&qn)
                    && !members.contains(&t.name)
                    && qn.to_ascii_lowercase().starts_with(&needle)
            })
            .map(|t| {
                let qn = t.qualified_name();
                let mut c = item(
                    &qn,
                    &qn,
                    CompletionKind::Table,
                    &format!("{} columns", t.columns.len()),
                );
                c.replace = offset - word.len()..offset;
                c
            })
            .collect();
    }

    let refs_target =
        ctx.trim_end().ends_with(['>', '<', '-']) && (ctx.contains("ref") || ctx.contains("Ref"));
    let items: Vec<Completion> =
        if refs_target || (word.contains('.') && ctx.to_ascii_lowercase().contains("ref")) {
            match word.rsplit_once('.') {
                Some((prefix, _)) => {
                    if let Some(t) = schema.table(prefix) {
                        let qn = t.qualified_name();
                        t.columns
                            .iter()
                            .map(|c| {
                                item(
                                    &c.name,
                                    &format!("{qn}.{}", c.name),
                                    CompletionKind::Column,
                                    &c.ty,
                                )
                            })
                            .collect()
                    } else {
                        schema
                            .tables
                            .iter()
                            .filter(|t| t.schema.as_deref() == Some(prefix))
                            .map(|t| {
                                let qn = t.qualified_name();
                                item(&qn, &format!("{qn}."), CompletionKind::Table, "table")
                            })
                            .collect()
                    }
                }
                None => schema
                    .tables
                    .iter()
                    .map(|t| {
                        let qn = t.qualified_name();
                        item(&qn, &format!("{qn}."), CompletionKind::Table, "table")
                    })
                    .collect(),
            }
        } else if in_brackets {
            SETTINGS
                .iter()
                .map(|(l, i, d)| item(l, i, CompletionKind::Setting, d))
                .collect()
        } else if depth > 0 && ctx.split_whitespace().count() == 1 {
            TYPES
                .iter()
                .map(|t| item(t, t, CompletionKind::Type, "type"))
                .collect()
        } else if depth == 0 && ctx.trim().is_empty() {
            vec![
                item("Table", "Table ", CompletionKind::Keyword, "new table"),
                item("Ref", "Ref: ", CompletionKind::Keyword, "new relation"),
                item("Enum", "Enum ", CompletionKind::Keyword, "enumeration"),
                item(
                    "TableGroup",
                    "TableGroup ",
                    CompletionKind::Keyword,
                    "table group",
                ),
                item(
                    "Project",
                    "Project ",
                    CompletionKind::Keyword,
                    "project metadata",
                ),
                item("Note", "Note: ", CompletionKind::Keyword, "note"),
            ]
        } else {
            Vec::new()
        };
    let needle = word.rsplit('.').next().unwrap_or(word).to_ascii_lowercase();
    items
        .into_iter()
        .filter(|c| c.label.to_ascii_lowercase().starts_with(&needle))
        .map(|mut c| {
            c.replace = offset - word.len()..offset;
            c
        })
        .collect()
}

/// Se o cursor está dentro de um bloco `TableGroup { ... }`, devolve os membros já
/// listados (para não sugeri-los de novo).
fn group_block(before: &str) -> Option<Vec<String>> {
    let mut open: Vec<usize> = Vec::new();
    for (i, ch) in before.char_indices() {
        match ch {
            '{' => open.push(i),
            '}' => {
                open.pop();
            }
            _ => {}
        }
    }
    let brace = *open.last()?;
    let line_start = before[..brace].rfind('\n').map_or(0, |i| i + 1);
    let header = before[line_start..brace].trim_start().to_ascii_lowercase();
    if !header.starts_with("tablegroup") {
        return None;
    }
    let body = &before[brace + 1..];
    let done = body.rfind('\n').map_or("", |i| &body[..i]);
    Some(
        done.lines()
            .map(|l| l.trim().trim_end_matches(',').trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with("//"))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: &str = "Table users {\n  id int [pk]\n  email text\n}\n";

    fn labels(text: &str) -> Vec<String> {
        complete(text, text.len())
            .into_iter()
            .map(|c| c.label)
            .collect()
    }

    #[test]
    fn keywords_at_top_level() {
        assert!(labels(&format!("{S}Ta")).contains(&"Table".to_string()));
    }

    #[test]
    fn types_after_column_name() {
        let l = labels("Table t {\n  name var");
        assert_eq!(l, vec!["varchar(255)".to_string()]);
    }

    #[test]
    fn settings_inside_brackets() {
        let l = labels("Table t {\n  id int [p");
        assert_eq!(l, vec!["pk".to_string()]);
    }

    #[test]
    fn tables_inside_table_group() {
        let base = "Table orders {\n  id int\n}\nTable order_items {\n  id int\n}\nTable users {\n  id int\n}\n";
        // já tem order_items; ao digitar `o`, sugere só orders
        let t = format!("{base}TableGroup vendas [color: #3b82f6] {{\n  order_items,\n  o");
        assert_eq!(labels(&t), vec!["orders".to_string()]);
        // linha vazia: todas as que faltam
        let t = format!("{base}TableGroup vendas {{\n  orders\n  ");
        assert_eq!(
            labels(&t),
            vec!["order_items".to_string(), "users".to_string()]
        );
    }

    #[test]
    fn tables_then_columns_in_refs() {
        let t = format!("{S}Ref: posts.user_id > us");
        assert_eq!(labels(&t), vec!["users".to_string()]);
        let c = format!("{S}Ref: posts.user_id > users.e");
        assert_eq!(labels(&c), vec!["email".to_string()]);
        let inline = "Table users {\n  id int [pk]\n}\nTable p {\n  u int [ref: > ";
        assert!(labels(inline).contains(&"users".to_string()));

        let multi = "Table auth.users {\n  id int [pk]\n}\nTable billing.invoices {\n  id int [pk]\n}\nRef: billing.invoices.id > auth.users.";
        assert_eq!(labels(multi), vec!["id".to_string()]);
    }

    proptest::proptest! {
        #[test]
        fn complete_and_lint_never_panic_on_arbitrary_utf8_offsets(
            text in ".*",
            offset in proptest::prelude::any::<usize>(),
        ) {
            let _ = complete(&text, offset);
            let _ = crate::lint(&text);
        }
    }
}
