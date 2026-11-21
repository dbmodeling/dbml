//! Table-level edits: note, column order, duplication and block range lookup.

use crate::enums::format_ident;
use crate::indexes::kv_edit;
use crate::model::{
    apply, bare_table_name, find_table, line_of, table_header_name, table_qualified_name,
    unique_table_name,
};
use crate::{EditError, Patch};
use dbml_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, child_tokens, parse, unquote};
use rowan::{TextRange, TextSize};
use std::ops::Range;

fn node_range(n: &SyntaxNode) -> Range<usize> {
    usize::from(n.text_range().start())..usize::from(n.text_range().end())
}

fn lookup(root: &SyntaxNode, table: &str) -> Result<SyntaxNode, EditError> {
    find_table(root, table).ok_or_else(|| EditError::TableNotFound(table.into()))
}

fn noop() -> Patch {
    Patch {
        range: TextRange::empty(0.into()),
        new_text: String::new(),
    }
}

/// Smallest single patch turning `old` into `new` (common prefix/suffix trimmed).
fn diff_patch(old: &str, new: &str) -> Patch {
    if old == new {
        return noop();
    }
    let mut pre = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(pre) || !new.is_char_boundary(pre) {
        pre -= 1;
    }
    let max_suf = old.len().min(new.len()) - pre;
    let mut suf = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(max_suf)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suf) || !new.is_char_boundary(new.len() - suf) {
        suf -= 1;
    }
    let to = |n: usize| TextSize::try_from(n).unwrap_or_default();
    Patch {
        range: TextRange::new(to(pre), to(old.len() - suf)),
        new_text: new[pre..new.len() - suf].to_string(),
    }
}

fn newline(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

fn brace(t: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    t.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|k| k.kind() == kind)
}

fn header(t: &SyntaxNode) -> Vec<SyntaxToken> {
    child_tokens(t)
        .into_iter()
        .take_while(|k| k.kind() != SyntaxKind::LBrace)
        .collect()
}

fn has_kv(toks: &[SyntaxToken], key: &str) -> bool {
    toks.windows(2)
        .any(|w| w[0].text().eq_ignore_ascii_case(key) && w[1].kind() == SyntaxKind::Colon)
}

// shortcut: a note that starts/ends with `'` or contains `'''` cannot round-trip (no escapes in
// the lexer); add `\'` support to `dbml-syntax` if users hit it.
fn note_literal(s: &str) -> String {
    if s.contains(['\'', '\n']) {
        format!("'''{s}'''")
    } else {
        format!("'{s}'")
    }
}

/// Byte range of the whole `Table ... { ... }` block of `table`, from `Table` to `}`.
#[must_use]
pub fn table_range(text: &str, table: &str) -> Option<Range<usize>> {
    find_table(&parse(text).syntax(), table).map(|t| node_range(&t))
}

/// Sets, replaces or removes (`None` / empty) the note of `table`.
///
/// An existing `Note: '...'` / `Note { ... }` body line or `[note: ...]` header setting is
/// edited where it is; otherwise a `Note: '...'` line is appended as the last body line.
/// Removing deletes the whole body line and drops empty header brackets.
///
/// # Errors
///
/// Returns [`EditError::TableNotFound`] if `table` does not exist.
pub fn set_table_note(text: &str, table: &str, note: Option<&str>) -> Result<Patch, EditError> {
    let root = parse(text).syntax();
    let t = lookup(&root, table)?;
    let lit = note.filter(|s| !s.is_empty()).map(note_literal);
    let head = header(&t);
    let in_head = has_kv(&head, "note");
    // `note text` is a column named note, not a note: require `Note:` or `Note {`.
    let body = t.children().find(|n| {
        n.kind() == SyntaxKind::Note
            && child_tokens(n)
                .get(1)
                .is_some_and(|k| matches!(k.kind(), SyntaxKind::Colon | SyntaxKind::LBrace))
    });
    let mut edits = Vec::new();
    if in_head && let Some(e) = kv_edit(text, &head, "note", lit.as_deref()) {
        edits.push(e);
    }
    match (lit, body) {
        (None, Some(n)) => edits.push((line_of(text, node_range(&n)), String::new())),
        (Some(l), Some(n)) if !in_head => {
            let toks = child_tokens(&n);
            match toks.iter().find(|k| k.kind() == SyntaxKind::Str) {
                Some(s) => edits.push((crate::model::range(s), l)),
                None => edits.push((node_range(&n), format!("Note: {l}"))),
            }
        }
        (Some(l), None) if !in_head => {
            let close = brace(&t, SyntaxKind::RBrace)
                .ok_or_else(|| EditError::TableNotFound(table.into()))?;
            let close_start = usize::from(close.text_range().start());
            let indent = t
                .children()
                .filter(|n| n.kind() == SyntaxKind::Column)
                .last()
                .or_else(|| t.children().last())
                .and_then(|n| n.prev_sibling_or_token())
                .and_then(rowan::NodeOrToken::into_token)
                .filter(|k| k.kind() == SyntaxKind::Whitespace)
                .map_or_else(|| "  ".to_string(), |k| k.text().to_string());
            let nl = newline(text);
            let line_start = text[..close_start].rfind('\n').map_or(0, |i| i + 1);
            if text[line_start..close_start].trim().is_empty() {
                edits.push((line_start..line_start, format!("{indent}Note: {l}{nl}")));
            } else {
                let at = text[..close_start].trim_end_matches([' ', '\t']).len();
                edits.push((at..close_start, format!("{nl}{indent}Note: {l}{nl}")));
            }
        }
        _ => {}
    }
    Ok(diff_patch(text, &apply(text, edits)))
}

/// Moves `column` `delta` positions among the columns of `table` (negative = up).
///
/// The column's end-of-line comment and the comment lines directly above it move with it.
/// The move never crosses a `Note`, an `indexes { }` block or the end of the body; an unknown
/// column or an out-of-range target yields an empty no-op patch.
///
/// # Errors
///
/// Returns [`EditError::TableNotFound`] if `table` does not exist.
pub fn move_column(
    text: &str,
    table: &str,
    column: &str,
    delta: isize,
) -> Result<Patch, EditError> {
    let root = parse(text).syntax();
    let t = lookup(&root, table)?;
    let (Some(open), Some(close)) = (brace(&t, SyntaxKind::LBrace), brace(&t, SyntaxKind::RBrace))
    else {
        return Ok(noop());
    };
    let open_end = usize::from(open.text_range().end());
    let close_start = usize::from(close.text_range().start());
    let body_start = text[open_end..]
        .find('\n')
        .map_or(text.len(), |i| open_end + i + 1);
    let kids: Vec<SyntaxNode> = t.children().collect();
    let is_col = |n: &SyntaxNode| n.kind() == SyntaxKind::Column;
    let Some(i) = kids.iter().position(|n| {
        is_col(n)
            && child_tokens(n)
                .first()
                .is_some_and(|k| unquote(k.text()) == column)
    }) else {
        return Ok(noop());
    };
    let (mut lo, mut hi) = (i, i);
    while lo > 0 && is_col(&kids[lo - 1]) {
        lo -= 1;
    }
    while hi + 1 < kids.len() && is_col(&kids[hi + 1]) {
        hi += 1;
    }
    let target = i
        .checked_add_signed(delta)
        .filter(|j| (lo..=hi).contains(j));
    let Some(target) = target.filter(|_| delta != 0) else {
        return Ok(noop());
    };
    let units: Vec<Range<usize>> = kids[lo..=hi]
        .iter()
        .map(|c| {
            let mut r = line_of(text, node_range(c));
            while r.start > body_start {
                let prev = text[..r.start - 1].rfind('\n').map_or(0, |p| p + 1);
                if prev < body_start || !text[prev..r.start].trim_start().starts_with("//") {
                    break;
                }
                r.start = prev;
            }
            r
        })
        .collect();
    // single-line bodies (`{ a int }`) have no whole lines to move
    if units
        .iter()
        .any(|u| u.start < body_start || u.end > close_start)
    {
        return Ok(noop());
    }
    let mut order: Vec<&str> = units.iter().map(|u| &text[u.clone()]).collect();
    let moved = order.remove(i - lo);
    order.insert(target - lo, moved);
    let mut out = String::new();
    for (k, u) in units.iter().enumerate() {
        out.push_str(order[k]);
        if let Some(next) = units.get(k + 1) {
            out.push_str(&text[u.end..next.start]);
        }
    }
    let span = units[0].start..units[units.len() - 1].end;
    Ok(diff_patch(text, &apply(text, vec![(span, out)])))
}

/// Inserts a copy of `table` right after its block (separated by a blank line), named by
/// [`unique_table_name`] from `<table>_copy`; returns the patch and the new qualified name.
///
/// Inline `ref: ...` column settings and the `as alias` are dropped from the copy so no
/// relation or alias gets duplicated; everything else is copied verbatim.
///
/// # Errors
///
/// Returns [`EditError::TableNotFound`] if `table` does not exist.
pub fn duplicate_table(text: &str, table: &str) -> Result<(Patch, String), EditError> {
    let root = parse(text).syntax();
    let t = lookup(&root, table)?;
    let not_found = || EditError::TableNotFound(table.into());
    let qual = table_qualified_name(&t).ok_or_else(not_found)?;
    let (_, name_tok) = table_header_name(&t).ok_or_else(not_found)?;
    let new_name = unique_table_name(text, &format!("{qual}_copy"));
    let head = header(&t);
    let ni = head
        .iter()
        .position(|k| k.text_range() == name_tok.text_range())
        .ok_or_else(not_found)?;
    let mut name_end = name_tok.text_range().end();
    if let [as_kw, alias, ..] = &head[ni + 1..]
        && as_kw.text().eq_ignore_ascii_case("as")
        && matches!(alias.kind(), SyntaxKind::Ident | SyntaxKind::Str)
    {
        name_end = alias.text_range().end();
    }
    let block = node_range(&t);
    let mut edits = vec![(
        usize::from(name_tok.text_range().start())..usize::from(name_end),
        format_ident(bare_table_name(&new_name)),
    )];
    for c in t.children().filter(|n| n.kind() == SyntaxKind::Column) {
        let toks = child_tokens(&c);
        if has_kv(&toks, "ref")
            && let Some(e) = kv_edit(text, &toks, "ref", None)
        {
            edits.push(e);
        }
    }
    let shifted = edits
        .into_iter()
        .map(|(r, s)| (r.start - block.start..r.end - block.start, s))
        .collect();
    let copy = apply(&text[block.clone()], shifted);
    let nl = newline(text);
    let at = TextSize::try_from(block.end).unwrap_or_default();
    Ok((
        Patch {
            range: TextRange::empty(at),
            new_text: format!("{nl}{nl}{copy}"),
        },
        new_name,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbml_analysis::{Severity, analyze, lint};

    fn clean(out: &str) {
        let errs: Vec<_> = lint(out)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errs.is_empty(), "{errs:?}\n{out}");
        assert_eq!(dbml_syntax::parse(out).syntax().text().to_string(), out);
    }

    fn note_of(out: &str, table: &str) -> Option<String> {
        analyze(out)
            .tables
            .into_iter()
            .find(|t| t.name == table)
            .and_then(|t| t.note)
    }

    fn cols(out: &str, table: &str) -> Vec<String> {
        analyze(out)
            .tables
            .into_iter()
            .find(|t| t.name == table)
            .map(|t| t.columns.into_iter().map(|c| c.name).collect())
            .unwrap_or_default()
    }

    fn note(src: &str, table: &str, n: Option<&str>) -> String {
        let out = set_table_note(src, table, n).unwrap().apply(src);
        clean(&out);
        out
    }

    // ---------- table_range ----------

    #[test]
    fn range_covers_block_only() {
        let src = "// c\nTable a {\n  id int\n}\n\nTable s.\"b x\" {\n  id int\n}\n";
        let r = table_range(src, "a").unwrap();
        assert_eq!(&src[r], "Table a {\n  id int\n}");
        let r = table_range(src, "s.b x").unwrap();
        assert_eq!(&src[r], "Table s.\"b x\" {\n  id int\n}");
        assert_eq!(table_range(src, "zz"), None);
    }

    // ---------- set_table_note ----------

    #[test]
    fn note_inserted_as_last_body_line_with_column_indent() {
        let src = "// topo\nTable t {\n    id int // pk\n    indexes {\n      id\n    }\n}\n";
        let out = note(src, "t", Some("Clientes"));
        assert_eq!(
            out,
            "// topo\nTable t {\n    id int // pk\n    indexes {\n      id\n    }\n    Note: 'Clientes'\n}\n"
        );
        assert_eq!(note_of(&out, "t").as_deref(), Some("Clientes"));
    }

    #[test]
    fn note_body_replaced_in_place_and_removed_with_line() {
        let src = "Table t {\n  id int\n  Note: 'old' // c\n  name text\n}\n";
        let out = note(src, "t", Some("new"));
        assert_eq!(
            out,
            "Table t {\n  id int\n  Note: 'new' // c\n  name text\n}\n"
        );
        let gone = note(&out, "t", None);
        assert_eq!(gone, "Table t {\n  id int\n  name text\n}\n");
        assert_eq!(note(src, "t", Some("")), gone);
    }

    #[test]
    fn note_block_form_replaced_in_place() {
        let src = "Table t {\n  id int\n  Note {\n    'old'\n  }\n}\n";
        let out = note(src, "t", Some("new"));
        assert_eq!(out, "Table t {\n  id int\n  Note {\n    'new'\n  }\n}\n");
        assert_eq!(note(src, "t", None), "Table t {\n  id int\n}\n");
    }

    #[test]
    fn note_header_setting_kept_in_header() {
        let src = "Table t [headercolor: #fff, note: 'old'] {\n  id int\n}\n";
        let out = note(src, "t", Some("new"));
        assert_eq!(
            out,
            "Table t [headercolor: #fff, note: 'new'] {\n  id int\n}\n"
        );
        assert_eq!(
            note(src, "t", None),
            "Table t [headercolor: #fff] {\n  id int\n}\n"
        );
        let only = "Table t [note: 'x'] {\n  id int\n}\n";
        assert_eq!(note(only, "t", None), "Table t {\n  id int\n}\n");
    }

    #[test]
    fn note_with_quote_or_newline_uses_triple_quotes() {
        let src = "Table t {\n  id int\n}\n";
        let out = note(src, "t", Some("it's\nmulti"));
        assert!(out.contains("  Note: '''it's\nmulti'''\n}"), "{out}");
        assert_eq!(note_of(&out, "t").as_deref(), Some("it's\nmulti"));
    }

    #[test]
    fn note_qualified_quoted_and_crlf() {
        let src = "Table s.\"my t\" {\r\n  id int\r\n}\r\n";
        let out = note(src, "s.my t", Some("x"));
        assert_eq!(
            out,
            "Table s.\"my t\" {\r\n  id int\r\n  Note: 'x'\r\n}\r\n"
        );
        assert_eq!(note(&out, "s.my t", None), src);
    }

    #[test]
    fn note_on_empty_and_single_line_table() {
        assert_eq!(
            note("Table t {\n}\n", "t", Some("x")),
            "Table t {\n  Note: 'x'\n}\n"
        );
        assert_eq!(
            note("Table t {}\n", "t", Some("x")),
            "Table t {\n  Note: 'x'\n}\n"
        );
    }

    #[test]
    fn note_removal_without_note_is_noop() {
        let src = "Table t {\n  id int\n}\n";
        let p = set_table_note(src, "t", None).unwrap();
        assert_eq!(p.apply(src), src);
        assert!(p.new_text.is_empty() && p.range.is_empty());
    }

    #[test]
    fn note_column_named_note_is_not_a_note() {
        let src = "Table t {\n  note text\n}\n";
        let out = note(src, "t", Some("x"));
        assert_eq!(out, "Table t {\n  note text\n  Note: 'x'\n}\n");
    }

    #[test]
    fn note_unknown_table() {
        assert_eq!(
            set_table_note("Table t {}", "x", Some("a")),
            Err(EditError::TableNotFound("x".into()))
        );
    }

    // ---------- move_column ----------

    fn mv(src: &str, table: &str, col: &str, d: isize) -> String {
        let out = move_column(src, table, col, d).unwrap().apply(src);
        clean(&out);
        out
    }

    #[test]
    fn move_down_and_up_carries_comments() {
        let src = "Table t {\n  // a doc\n  a int // ea\n  b int\n  c int\n}\n";
        let out = mv(src, "t", "a", 1);
        assert_eq!(
            out,
            "Table t {\n  b int\n  // a doc\n  a int // ea\n  c int\n}\n"
        );
        assert_eq!(mv(&out, "t", "a", -1), src);
        assert_eq!(
            mv(src, "t", "c", -2),
            "Table t {\n  c int\n  // a doc\n  a int // ea\n  b int\n}\n"
        );
    }

    #[test]
    fn move_out_of_bounds_is_noop() {
        let src = "Table t {\n  a int\n  b int\n}\n";
        for (c, d) in [("a", -1), ("b", 1), ("a", 5), ("a", 0)] {
            let p = move_column(src, "t", c, d).unwrap();
            assert!(p.new_text.is_empty() && p.range.is_empty(), "{c} {d}");
        }
        let p = move_column(src, "t", "zz", 1).unwrap();
        assert!(p.new_text.is_empty() && p.range.is_empty());
    }

    #[test]
    fn move_never_crosses_note_or_indexes() {
        let src = "Table t {\n  a int\n  Note: 'n'\n  b int\n  indexes {\n    a\n  }\n  c int\n}\n";
        for (c, d) in [("a", 1), ("b", -1), ("b", 1), ("c", -1)] {
            assert_eq!(mv(src, "t", c, d), src, "{c} {d}");
        }
    }

    #[test]
    fn move_keeps_blank_lines_in_place_qualified_and_crlf() {
        let src = "Table s.\"t x\" {\r\n  a int\r\n\r\n  b int\r\n}\r\n";
        let out = mv(src, "s.t x", "b", -1);
        assert_eq!(out, "Table s.\"t x\" {\r\n  b int\r\n\r\n  a int\r\n}\r\n");
        assert_eq!(cols(&out, "t x"), ["b", "a"]);
    }

    #[test]
    fn move_unknown_table() {
        assert_eq!(
            move_column("Table t {}", "x", "a", 1),
            Err(EditError::TableNotFound("x".into()))
        );
    }

    // ---------- duplicate_table ----------

    fn dup(src: &str, table: &str) -> (String, String) {
        let (p, name) = duplicate_table(src, table).unwrap();
        let out = p.apply(src);
        clean(&out);
        (out, name)
    }

    #[test]
    fn duplicate_strips_inline_refs_and_keeps_the_rest() {
        let src = "// c\nTable orders [headercolor: #fff] {\n  id int [pk] // k\n  user_id int [ref: > users.id]\n  total int [not null, ref: > x.id, default: 0, note: 'n']\n  indexes {\n    user_id\n  }\n  Note: 'o'\n}\n\nTable users {\n  id int [pk]\n}\n\nTable x {\n  id int [pk]\n}\n\nRef: orders.id - users.id\n\nTableGroup g {\n  orders\n}\n";
        let (out, name) = dup(src, "orders");
        assert_eq!(name, "orders_copy");
        let copy = "Table orders_copy [headercolor: #fff] {\n  id int [pk] // k\n  user_id int\n  total int [not null, default: 0, note: 'n']\n  indexes {\n    user_id\n  }\n  Note: 'o'\n}";
        assert!(
            out.contains(&format!("  Note: 'o'\n}}\n\n{copy}\n\nTable users")),
            "{out}"
        );
        let s = analyze(&out);
        assert_eq!(s.refs.len(), analyze(src).refs.len());
        let c = s.tables.iter().find(|t| t.name == "orders_copy").unwrap();
        assert_eq!(c.indexes.len(), 1);
        assert_eq!(c.note.as_deref(), Some("o"));
        assert!(out.ends_with("TableGroup g {\n  orders\n}\n"));
    }

    #[test]
    fn duplicate_twice_generates_unique_names() {
        let src = "Table orders {\n  id int\n}\n";
        let (once, _) = dup(src, "orders");
        let (twice, name) = dup(&once, "orders");
        assert_eq!(name, "orders_copy_2");
        assert_eq!(cols(&twice, "orders_copy_2"), ["id"]);
    }

    #[test]
    fn duplicate_qualified_quoted_alias_and_crlf() {
        let src = "Table s.\"my t\" as M {\r\n  id int\r\n}\r\n";
        let (out, name) = dup(src, "s.my t");
        assert_eq!(name, "s.my t_copy");
        assert_eq!(
            out,
            "Table s.\"my t\" as M {\r\n  id int\r\n}\r\n\r\nTable s.\"my t_copy\" {\r\n  id int\r\n}\r\n"
        );
        assert!(table_range(&out, &name).is_some());
    }

    #[test]
    fn duplicate_unknown_table() {
        assert_eq!(
            duplicate_table("Table t {}", "x"),
            Err(EditError::TableNotFound("x".into()))
        );
    }
}
