//! Structural, comment-preserving edits for DBML `Enum` blocks and values.

use dbml_syntax::{
    SyntaxKind, SyntaxNode, SyntaxToken, child_tokens as sig, descendant_tokens as tokens, parse,
    unquote,
};
use rowan::TextRange;
use std::collections::HashSet;
use std::fmt::Write as _;

fn enum_name_tok(node: &SyntaxNode) -> Option<SyntaxToken> {
    sig(node)
        .into_iter()
        .filter(|t| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::Str))
        .nth(1)
}

fn find_enum(root: &SyntaxNode, name: &str) -> Option<SyntaxNode> {
    root.children().find(|n| {
        n.kind() == SyntaxKind::Enum && enum_name_tok(n).is_some_and(|t| unquote(t.text()) == name)
    })
}

fn whole_line(text: &str, range: TextRange) -> std::ops::Range<usize> {
    let s = usize::from(range.start()).min(text.len());
    let e = usize::from(range.end()).min(text.len());
    let start = text[..s].rfind('\n').map_or(0, |i| i + 1);
    let end = text[e..].find('\n').map_or(text.len(), |i| e + i + 1);
    start..end
}

fn value_tok(col: &SyntaxNode) -> Option<SyntaxToken> {
    tokens(col)
        .into_iter()
        .find(|t| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::Str))
}

fn format_ident(name: &str) -> String {
    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.is_empty() {
        name.to_string()
    } else {
        format!("\"{name}\"")
    }
}

/// Returns a unique `Enum` identifier based on `base` that does not collide with existing enums.
#[must_use]
pub fn unique_enum_name(text: &str, base: &str) -> String {
    let root = parse(text).syntax();
    let existing: HashSet<String> = root
        .children()
        .filter(|n| n.kind() == SyntaxKind::Enum)
        .filter_map(|n| enum_name_tok(&n).map(|t| unquote(t.text()).to_string()))
        .collect();
    if !existing.contains(base) {
        return base.to_string();
    }
    let mut n = 2usize;
    loop {
        let candidate = format!("{base}_{n}");
        if !existing.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Appends a new `Enum <name> { value_1 }` block to `text`.
#[must_use]
pub fn add_enum(text: &str, name: &str) -> String {
    let ident = format_ident(name);
    let mut out = text.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
    let _ = writeln!(out, "Enum {ident} {{\n  value_1\n}}");
    out
}

/// Deletes the `Enum <name> { ... }` block from `text`.
#[must_use]
pub fn delete_enum(text: &str, name: &str) -> String {
    let root = parse(text).syntax();
    let Some(node) = find_enum(&root, name) else {
        return text.to_string();
    };
    let range = whole_line(text, node.text_range());
    let mut out = text.to_string();
    out.replace_range(range, "");
    out
}

/// Renames `Enum <old_name>` to `new_name` and cascades the type rename to table columns.
#[must_use]
pub fn rename_enum(text: &str, old_name: &str, new_name: &str) -> String {
    if old_name == new_name {
        return text.to_string();
    }
    let root = parse(text).syntax();
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let formatted_new = format_ident(new_name);

    if let Some(node) = find_enum(&root, old_name)
        && let Some(tok) = enum_name_tok(&node)
    {
        let r = tok.text_range();
        edits.push((
            usize::from(r.start())..usize::from(r.end()),
            formatted_new.clone(),
        ));
    }

    for table in root.children().filter(|n| n.kind() == SyntaxKind::Table) {
        for col in table.children().filter(|n| n.kind() == SyntaxKind::Column) {
            let toks = tokens(&col);
            let mut ty_toks = toks
                .iter()
                .skip(1)
                .take_while(|t| t.kind() != SyntaxKind::LBracket);
            if let Some(first_ty) = ty_toks.next()
                && ty_toks.next().is_none()
                && unquote(first_ty.text()) == old_name
            {
                let r = first_ty.text_range();
                edits.push((
                    usize::from(r.start())..usize::from(r.end()),
                    formatted_new.clone(),
                ));
            }
        }
    }

    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = text.to_string();
    for (range, rep) in edits {
        if range.start <= range.end && range.end <= out.len() {
            out.replace_range(range, &rep);
        }
    }
    out
}

/// Inserts a new `value` at the end of `Enum <enum_name>`.
#[must_use]
pub fn add_enum_value(text: &str, enum_name: &str, value: &str) -> Option<String> {
    let root = parse(text).syntax();
    let node = find_enum(&root, enum_name)?;
    let rbrace = sig(&node)
        .into_iter()
        .rfind(|t| t.kind() == SyntaxKind::RBrace)?;
    let last_col = node
        .children()
        .filter(|n| n.kind() == SyntaxKind::Column)
        .last();
    let indent = last_col
        .as_ref()
        .and_then(|c| {
            let start = usize::from(c.text_range().start());
            let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
            let slice = &text[line_start..start];
            slice
                .chars()
                .all(|ch| ch == ' ' || ch == '\t')
                .then_some(slice)
        })
        .unwrap_or("  ");
    let pos = usize::from(rbrace.text_range().start());
    let line_start = text[..pos].rfind('\n').map_or(0, |i| i + 1);
    let insert_at = if text[line_start..pos].trim().is_empty() {
        line_start
    } else {
        pos
    };
    let val_str = format_ident(value);
    let insertion = if insert_at == pos && !text[..pos].ends_with('\n') {
        format!("\n{indent}{val_str}\n")
    } else {
        format!("{indent}{val_str}\n")
    };
    let mut out = text.to_string();
    out.insert_str(insert_at, &insertion);
    Some(out)
}

/// Deletes `value` from `Enum <enum_name>`.
#[must_use]
pub fn delete_enum_value(text: &str, enum_name: &str, value: &str) -> Option<String> {
    let root = parse(text).syntax();
    let node = find_enum(&root, enum_name)?;
    let col = node
        .children()
        .filter(|n| n.kind() == SyntaxKind::Column)
        .find(|c| value_tok(c).is_some_and(|t| unquote(t.text()) == value))?;
    let range = whole_line(text, col.text_range());
    let mut out = text.to_string();
    out.replace_range(range, "");
    Some(out)
}

/// Renames `old_val` to `new_val` inside `Enum <enum_name>`.
#[must_use]
pub fn rename_enum_value(
    text: &str,
    enum_name: &str,
    old_val: &str,
    new_val: &str,
) -> Option<String> {
    if old_val == new_val {
        return Some(text.to_string());
    }
    let root = parse(text).syntax();
    let node = find_enum(&root, enum_name)?;
    let col = node
        .children()
        .filter(|n| n.kind() == SyntaxKind::Column)
        .find(|c| value_tok(c).is_some_and(|t| unquote(t.text()) == old_val))?;
    let tok = value_tok(&col)?;
    let r = tok.text_range();
    let mut out = text.to_string();
    out.replace_range(
        usize::from(r.start())..usize::from(r.end()),
        &format_ident(new_val),
    );
    Some(out)
}

/// Moves `value` one position up (`up = true`) or down (`up = false`) inside `Enum <enum_name>`.
#[must_use]
pub fn reorder_enum_value(text: &str, enum_name: &str, value: &str, up: bool) -> Option<String> {
    let root = parse(text).syntax();
    let node = find_enum(&root, enum_name)?;
    let cols: Vec<SyntaxNode> = node
        .children()
        .filter(|n| n.kind() == SyntaxKind::Column)
        .collect();
    let idx = cols
        .iter()
        .position(|c| value_tok(c).is_some_and(|t| unquote(t.text()) == value))?;
    let swap_idx = if up {
        idx.checked_sub(1)?
    } else if idx + 1 < cols.len() {
        idx + 1
    } else {
        return None;
    };
    let (a, b) = if idx < swap_idx {
        (
            whole_line(text, cols[idx].text_range()),
            whole_line(text, cols[swap_idx].text_range()),
        )
    } else {
        (
            whole_line(text, cols[swap_idx].text_range()),
            whole_line(text, cols[idx].text_range()),
        )
    };
    if a.end > b.start {
        return None;
    }
    let slice_a = text[a.clone()].to_string();
    let slice_b = text[b.clone()].to_string();
    let mut out = text.to_string();
    out.replace_range(b, &slice_a);
    out.replace_range(a, &slice_b);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
// user roles
Enum role {
  admin [note: 'full']
  member
}

Table users {
  id int [pk]
  role role [not null]
}
";

    #[test]
    fn add_and_unique_enum_name() {
        assert_eq!(unique_enum_name(SAMPLE, "role"), "role_2");
        assert_eq!(unique_enum_name(SAMPLE, "status"), "status");
        let added = add_enum(SAMPLE, "status");
        assert!(added.contains("Enum status {\n  value_1\n}\n"));
    }

    #[test]
    fn rename_enum_cascades_column_types_and_preserves_comments() {
        let renamed = rename_enum(SAMPLE, "role", "user_role");
        assert!(renamed.contains("// user roles"));
        assert!(renamed.contains("Enum user_role {"));
        assert!(renamed.contains("role user_role [not null]"));
    }

    #[test]
    fn value_crud_and_reorder() {
        let added = add_enum_value(SAMPLE, "role", "guest").unwrap();
        assert!(added.contains("  guest\n}"));
        let renamed = rename_enum_value(&added, "role", "guest", "viewer").unwrap();
        assert!(renamed.contains("  viewer\n}"));
        let moved = reorder_enum_value(&renamed, "role", "viewer", true).unwrap();
        let m_pos = moved.find("  member\n").unwrap();
        let v_pos = moved.find("  viewer\n").unwrap();
        assert!(v_pos < m_pos, "{moved}");
        let deleted = delete_enum_value(&moved, "role", "viewer").unwrap();
        assert!(!deleted.contains("viewer"));
        let gone = delete_enum(&deleted, "role");
        assert!(!gone.contains("Enum role"));
    }
}
