//! Structural editing operations for column `default`/`check` constraints and `indexes { ... }` blocks.

use crate::model::{apply, find_column, find_table, line_of};
use dbml_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, descendant_tokens as sig, parse, unquote};
use schema::IndexDef;
use std::ops::Range;

fn format_default_literal(raw: &str) -> Option<String> {
    let v = raw.trim();
    if v.is_empty() {
        return None;
    }
    if (v.starts_with('\'') && v.ends_with('\''))
        || (v.starts_with('"') && v.ends_with('"'))
        || (v.starts_with('`') && v.ends_with('`'))
        || v.parse::<f64>().is_ok()
        || matches!(v.to_ascii_lowercase().as_str(), "true" | "false" | "null")
    {
        return Some(v.to_string());
    }
    if v.contains('(') && v.ends_with(')') {
        return Some(format!("`{v}`"));
    }
    Some(format!("'{}'", v.replace('\'', "\\'")))
}

fn format_check_literal(raw: &str) -> Option<String> {
    let v = unquote(raw.trim()).trim();
    if v.is_empty() {
        return None;
    }
    Some(format!("`{v}`"))
}

fn set_column_kv(
    text: &str,
    table: &str,
    col: &str,
    key: &str,
    formatted_val: Option<String>,
) -> Option<String> {
    let root = parse(text).syntax();
    let c = find_column(&find_table(&root, table)?, col)?;
    let toks = sig(&c);
    let open = toks.iter().position(|k| k.kind() == SyntaxKind::LBracket);
    let close = toks.iter().position(|k| k.kind() == SyntaxKind::RBracket);
    match (open, close) {
        (Some(o), Some(cl)) if o < cl => {
            let inner = &toks[o + 1..cl];
            let mut kept: Vec<String> = Vec::new();
            let mut replaced = false;
            for part in inner.split(|t| t.kind() == SyntaxKind::Comma) {
                let Some(first) = part.first() else { continue };
                let Some(last) = part.last() else { continue };
                if first.text().eq_ignore_ascii_case(key)
                    && part.get(1).is_some_and(|t| t.kind() == SyntaxKind::Colon)
                {
                    if let Some(ref val) = formatted_val
                        && !replaced
                    {
                        kept.push(format!("{key}: {val}"));
                        replaced = true;
                    }
                    continue;
                }
                let s = usize::from(first.text_range().start());
                let e = usize::from(last.text_range().end());
                kept.push(text[s..e].to_string());
            }
            if let Some(val) = formatted_val
                && !replaced
            {
                kept.push(format!("{key}: {val}"));
            }
            let whole =
                usize::from(toks[o].text_range().start())..usize::from(toks[cl].text_range().end());
            let edit = if kept.is_empty() {
                (
                    text[..whole.start].trim_end().len()..whole.end,
                    String::new(),
                )
            } else {
                (whole, format!("[{}]", kept.join(", ")))
            };
            Some(apply(text, vec![edit]))
        }
        _ => {
            if let Some(val) = formatted_val {
                let end = usize::from(toks.last()?.text_range().end());
                Some(apply(text, vec![(end..end, format!(" [{key}: {val}]"))]))
            } else {
                Some(text.to_string())
            }
        }
    }
}

/// Sets or clears the `default: ...` attribute on `table.col`, preserving other column settings.
#[must_use]
pub fn set_column_default(
    text: &str,
    table: &str,
    col: &str,
    default_val: Option<&str>,
) -> Option<String> {
    let formatted = default_val.and_then(format_default_literal);
    set_column_kv(text, table, col, "default", formatted)
}

/// Sets or clears the ``check: `...` `` attribute on `table.col`, preserving other column settings.
#[must_use]
pub fn set_column_check(
    text: &str,
    table: &str,
    col: &str,
    check_expr: Option<&str>,
) -> Option<String> {
    let formatted = check_expr.and_then(format_check_literal);
    set_column_kv(text, table, col, "check", formatted)
}

fn find_indexes_block(t_node: &SyntaxNode) -> Option<SyntaxNode> {
    t_node.children().find(|n| n.kind() == SyntaxKind::Indexes)
}

fn index_entry_spans(block: &SyntaxNode) -> Vec<Range<usize>> {
    let all_toks: Vec<SyntaxToken> = block
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|k| k.kind() == SyntaxKind::Newline || !k.kind().is_trivia())
        .collect();
    let mut spans = Vec::new();
    for line in all_toks.split(|k| k.kind() == SyntaxKind::Newline) {
        let ts: Vec<&SyntaxToken> = line
            .iter()
            .filter(|k| {
                !matches!(k.kind(), SyntaxKind::LBrace | SyntaxKind::RBrace)
                    && !k.text().eq_ignore_ascii_case("indexes")
            })
            .collect();
        if ts.is_empty() {
            continue;
        }
        let open = ts.iter().position(|k| k.kind() == SyntaxKind::LBracket);
        let col_slice = match open {
            Some(i) => &ts[..i],
            None => &ts[..],
        };
        let has_col = col_slice
            .iter()
            .any(|k| matches!(k.kind(), SyntaxKind::Ident | SyntaxKind::Str));
        if !has_col {
            continue;
        }
        if let (Some(first), Some(last)) = (ts.first(), ts.last()) {
            spans.push(
                usize::from(first.text_range().start())..usize::from(last.text_range().end()),
            );
        }
    }
    spans
}

/// Appends `index` to the `indexes { ... }` block of `table` (creating the block if absent).
#[must_use]
pub fn add_index(text: &str, table: &str, index: &IndexDef) -> Option<String> {
    if index.columns.is_empty() {
        return None;
    }
    let root = parse(text).syntax();
    let t_node = find_table(&root, table)?;
    let line = index.to_dbml_line();

    if let Some(block) = find_indexes_block(&t_node) {
        let close = block
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|k| k.kind() == SyntaxKind::RBrace)?;
        let close_start = usize::from(close.text_range().start());
        let line_start = text[..close_start]
            .rfind('\n')
            .map_or(close_start, |i| i + 1);
        let prefix = if line_start == close_start { "\n" } else { "" };
        return Some(apply(
            text,
            vec![(line_start..line_start, format!("{prefix}    {line}\n"))],
        ));
    }

    let close = t_node
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|k| k.kind() == SyntaxKind::RBrace)?;
    let close_start = usize::from(close.text_range().start());
    let line_start = text[..close_start]
        .rfind('\n')
        .map_or(close_start, |i| i + 1);
    let prefix = if line_start == close_start { "\n" } else { "" };
    Some(apply(
        text,
        vec![(
            line_start..line_start,
            format!("{prefix}  indexes {{\n    {line}\n  }}\n"),
        )],
    ))
}

/// Updates the index at `index_idx` inside `table`'s `indexes { ... }` block.
/// If `index.columns` is empty, removes the index entry instead.
#[must_use]
pub fn update_index(text: &str, table: &str, index_idx: usize, index: &IndexDef) -> Option<String> {
    if index.columns.is_empty() {
        return delete_index(text, table, index_idx);
    }
    let root = parse(text).syntax();
    let t_node = find_table(&root, table)?;
    let block = find_indexes_block(&t_node)?;
    let spans = index_entry_spans(&block);
    let span = spans.get(index_idx)?.clone();
    Some(apply(text, vec![(span, index.to_dbml_line())]))
}

/// Deletes the index at `index_idx` inside `table`'s `indexes { ... }` block, removing
/// the enclosing `indexes { ... }` block when its last entry is deleted.
#[must_use]
pub fn delete_index(text: &str, table: &str, index_idx: usize) -> Option<String> {
    let root = parse(text).syntax();
    let t_node = find_table(&root, table)?;
    let block = find_indexes_block(&t_node)?;
    let spans = index_entry_spans(&block);
    if index_idx >= spans.len() {
        return None;
    }
    if spans.len() == 1 {
        let mut r = line_of(
            text,
            usize::from(block.text_range().start())..usize::from(block.text_range().end()),
        );
        if text[..r.start].ends_with("\n\n") {
            r.start -= 1;
        }
        return Some(apply(text, vec![(r, String::new())]));
    }
    let r = line_of(text, spans[index_idx].clone());
    Some(apply(text, vec![(r, String::new())]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema::{IndexColumn, IndexType};

    const BASE: &str = "\
Table products {
  id int [pk]
  sku varchar(32) [not null]
  price decimal
}
";

    #[test]
    fn set_and_clear_column_default_and_check() {
        let s1 = set_column_default(BASE, "products", "price", Some("0")).unwrap();
        assert!(s1.contains("price decimal [default: 0]"), "{s1}");

        let s2 = set_column_check(&s1, "products", "price", Some("price >= 0")).unwrap();
        assert!(
            s2.contains("price decimal [default: 0, check: `price >= 0`]"),
            "{s2}"
        );

        let s3 = set_column_default(&s2, "products", "sku", Some("PENDING")).unwrap();
        assert!(
            s3.contains("sku varchar(32) [not null, default: 'PENDING']"),
            "{s3}"
        );

        let s4 = set_column_default(&s3, "products", "price", None).unwrap();
        assert!(s4.contains("price decimal [check: `price >= 0`]"), "{s4}");

        let s5 = set_column_check(&s4, "products", "price", Some("")).unwrap();
        assert!(s5.contains("  price decimal\n"), "{s5}");
    }

    #[test]
    fn add_update_and_delete_indexes_lifecycle() {
        let idx1 = IndexDef {
            columns: vec![IndexColumn::Column("sku".into())],
            unique: true,
            pk: false,
            name: None,
            index_type: None,
        };
        let s1 = add_index(BASE, "products", &idx1).unwrap();
        assert!(s1.contains("  indexes {\n    sku [unique]\n  }\n}"), "{s1}");

        let idx2 = IndexDef {
            columns: vec![
                IndexColumn::Column("sku".into()),
                IndexColumn::Column("price".into()),
            ],
            unique: false,
            pk: false,
            name: Some("idx_sku_price".into()),
            index_type: Some(IndexType::BTree),
        };
        let s2 = add_index(&s1, "products", &idx2).unwrap();
        assert!(
            s2.contains(
                "    sku [unique]\n    (sku, price) [type: btree, name: 'idx_sku_price']\n"
            ),
            "{s2}"
        );

        let idx1_updated = IndexDef {
            columns: vec![IndexColumn::Column("sku".into())],
            unique: true,
            pk: false,
            name: Some("uq_sku".into()),
            index_type: Some(IndexType::Hash),
        };
        let s3 = update_index(&s2, "products", 0, &idx1_updated).unwrap();
        assert!(
            s3.contains("    sku [unique, type: hash, name: 'uq_sku']\n"),
            "{s3}"
        );

        // Rename column `sku` -> `code` cascades into `indexes { ... }`
        let s4 = crate::rename_column(&s3, "products", "sku", "code").unwrap();
        assert!(
            s4.contains("    code [unique, type: hash, name: 'uq_sku']\n"),
            "{s4}"
        );
        assert!(
            s4.contains("    (code, price) [type: btree, name: 'idx_sku_price']\n"),
            "{s4}"
        );

        // Delete first index leaves second index
        let s5 = delete_index(&s4, "products", 0).unwrap();
        assert!(!s5.contains("uq_sku"), "{s5}");
        assert!(s5.contains("(code, price)"), "{s5}");

        // Delete last index removes the entire `indexes { ... }` block
        let s6 = delete_index(&s5, "products", 0).unwrap();
        assert!(!s6.contains("indexes"), "{s6}");
    }
}
