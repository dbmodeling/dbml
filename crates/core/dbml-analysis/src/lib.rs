#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Tolerant CST-to-schema semantic analysis, autocompletion, and diagnostics ([`lint()`]).

mod complete;
mod export_sql;
mod lint;
pub use complete::{Completion, CompletionKind, complete};
pub use export_sql::{SqlDialect, SqlExportError, export_sql_ddl};
pub use lint::{Diagnostic, Severity, lint};

use dbml_syntax::{
    SyntaxKind, SyntaxNode, SyntaxToken, descendant_tokens as tokens, parse, unquote,
};
use schema::{
    Column, Endpoint, EnumDef, EnumValue, Group, IndexColumn, IndexDef, IndexType, Nullability,
    Ref, RefAction, RefKind, Schema, Table,
};
use std::collections::{HashMap, HashSet};

/// Parses `text` into a CST and projects it into a semantic [`Schema`].
#[must_use]
pub fn analyze(text: &str) -> Schema {
    let root = parse(text).syntax();
    let mut schema = Schema::default();
    for node in root.children() {
        match node.kind() {
            SyntaxKind::Table => {
                if let Some(t) = table(&node, text, &mut schema.refs) {
                    schema.tables.push(t);
                }
            }
            SyntaxKind::Enum => {
                if let Some(e) = enum_def(&node) {
                    schema.enums.push(e);
                }
            }
            SyntaxKind::Project => {
                if schema.project_note.is_none() {
                    schema.project_note = node
                        .children()
                        .find(|n| n.kind() == SyntaxKind::Note)
                        .and_then(|n| extract_note(&n));
                }
            }
            SyntaxKind::Ref => schema.refs.extend(top_level_ref(&node)),
            SyntaxKind::TableGroup => schema.groups.extend(group(&node)),
            _ => {}
        }
    }

    resolve_aliases(&mut schema);

    // Marca `fk = true` em toda coluna que é origem (`from`) de uma `Ref` (simples ou composta)
    let fk_cols: HashSet<(String, String)> = schema
        .refs
        .iter()
        .flat_map(|r| {
            let qt = r.from.qualified_table();
            r.from
                .cols()
                .into_iter()
                .map(|c| (qt.clone(), c.to_string()))
                .collect::<Vec<_>>()
        })
        .collect();
    for t in &mut schema.tables {
        let qt = t.qualified_name();
        for c in &mut t.columns {
            if fk_cols.contains(&(qt.clone(), c.name.clone())) {
                c.fk = true;
            }
        }
    }
    schema
}

fn resolve_aliases(schema: &mut Schema) {
    let alias_map: HashMap<String, (Option<String>, String)> = schema
        .tables
        .iter()
        .filter_map(|t| {
            t.alias
                .as_ref()
                .map(|a| (a.clone(), (t.schema.clone(), t.name.clone())))
        })
        .collect();
    if alias_map.is_empty() {
        return;
    }
    for r in &mut schema.refs {
        for ep in [&mut r.from, &mut r.to] {
            if ep.schema.is_none()
                && let Some((sch, real_name)) = alias_map.get(&ep.table)
            {
                ep.schema = sch.clone();
                ep.table = real_name.clone();
            }
        }
    }
}

/// Tokens do cabeçalho (`Table nome [opções] {`), sem trivia.
fn header(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !t.kind().is_trivia())
        .take_while(|t| t.kind() != SyntaxKind::LBrace)
        .collect()
}

/// Valor de `chave: valor` dentro de `[...]` (o valor pode vir em vários tokens, ex.: `#3498DB`).
pub(crate) fn setting(toks: &[SyntaxToken], key: &str) -> Option<String> {
    let i = toks
        .iter()
        .position(|t| t.text().eq_ignore_ascii_case(key))?;
    let rest = toks.get(i + 1..)?;
    let start = usize::from(rest.first().as_ref().is_some_and(|t| t.text() == ":"));
    let v: String = rest[start..]
        .iter()
        .take_while(|t| !matches!(t.kind(), SyntaxKind::Comma | SyntaxKind::RBracket))
        .map(rowan::SyntaxToken::text)
        .collect();
    (!v.is_empty()).then(|| unquote(&v).to_string())
}

/// Parses `[schema.]name [as alias]` from a `Table` or `Enum` header (tokens before `[`).
pub(crate) fn parse_qualified_header(
    head: &[SyntaxToken],
    kw: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let before_bracket: Vec<&SyntaxToken> = head
        .iter()
        .take_while(|t| t.kind() != SyntaxKind::LBracket)
        .collect();
    let start = usize::from(
        before_bracket
            .first()
            .is_some_and(|t| t.text().eq_ignore_ascii_case(kw)),
    );
    let toks = &before_bracket[start..];
    let (schema, name, rest_idx) = match toks {
        [s, dot, n, ..]
            if matches!(s.kind(), SyntaxKind::Ident | SyntaxKind::Str)
                && dot.text() == "."
                && matches!(n.kind(), SyntaxKind::Ident | SyntaxKind::Str) =>
        {
            (
                Some(unquote(s.text()).to_string()),
                Some(unquote(n.text()).to_string()),
                3,
            )
        }
        [n, ..] if matches!(n.kind(), SyntaxKind::Ident | SyntaxKind::Str) => {
            let raw = unquote(n.text());
            if let Some((s, t)) = raw.split_once('.') {
                (Some(s.to_string()), Some(t.to_string()), 1)
            } else {
                (None, Some(raw.to_string()), 1)
            }
        }
        _ => (None, None, 0),
    };
    let alias = match &toks[rest_idx..] {
        [as_kw, a, ..]
            if as_kw.text().eq_ignore_ascii_case("as")
                && matches!(a.kind(), SyntaxKind::Ident | SyntaxKind::Str) =>
        {
            Some(unquote(a.text()).to_string())
        }
        _ => None,
    };
    (schema, name, alias)
}

fn table(node: &SyntaxNode, source: &str, refs: &mut Vec<Ref>) -> Option<Table> {
    let head = header(node);
    let (schema_name, name_opt, alias) = parse_qualified_header(&head, "table");
    let name = name_opt?;
    let color = setting(&head, "headercolor");
    let mut table_note = setting(&head, "note");
    let mut columns = Vec::new();
    let mut indexes = Vec::new();

    for child in node.children() {
        match child.kind() {
            SyntaxKind::Column => {
                if let Some(c) = column(&child, schema_name.as_deref(), &name, source, refs) {
                    columns.push(c);
                }
            }
            SyntaxKind::Indexes => extract_indexes(&child, &mut indexes),
            SyntaxKind::Block => {
                if tokens(&child)
                    .first()
                    .is_some_and(|t| t.text().eq_ignore_ascii_case("indexes"))
                {
                    extract_indexes(&child, &mut indexes);
                }
            }
            SyntaxKind::Note if table_note.is_none() => {
                table_note = extract_note(&child);
            }
            _ => {}
        }
    }
    Some(Table {
        name,
        columns,
        color,
        schema: schema_name,
        alias,
        note: table_note,
        indexes,
    })
}

fn enum_def(node: &SyntaxNode) -> Option<EnumDef> {
    let head = header(node);
    let (schema, name_opt, _) = parse_qualified_header(&head, "enum");
    let name = name_opt?;
    let mut values = Vec::new();
    for col in node.children().filter(|n| n.kind() == SyntaxKind::Column) {
        let toks = tokens(&col);
        let Some(first) = toks
            .first()
            .filter(|t| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::Str))
        else {
            continue;
        };
        let val_note = setting(&toks, "note");
        values.push(EnumValue {
            name: unquote(first.text()).to_string(),
            note: val_note,
        });
    }
    Some(EnumDef {
        schema,
        name,
        values,
    })
}

fn extract_note(node: &SyntaxNode) -> Option<String> {
    let toks = tokens(node);
    let val_tok = toks
        .iter()
        .find(|t| t.kind() == SyntaxKind::Str)
        .or_else(|| {
            toks.iter()
                .skip_while(|t| t.kind() != SyntaxKind::Colon)
                .nth(1)
        })?;
    let text = unquote(val_tok.text());
    (!text.is_empty()).then(|| text.to_string())
}

fn extract_indexes(block: &SyntaxNode, out: &mut Vec<IndexDef>) {
    let all_toks: Vec<SyntaxToken> = block
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|k| k.kind() == SyntaxKind::Newline || !k.kind().is_trivia())
        .collect();
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
        let columns: Vec<IndexColumn> = col_slice
            .iter()
            .filter(|k| matches!(k.kind(), SyntaxKind::Ident | SyntaxKind::Str))
            .map(|k| {
                let raw = k.text();
                if k.kind() == SyntaxKind::Str && raw.starts_with('`') && raw.ends_with('`') {
                    IndexColumn::Expression(unquote(raw).to_string())
                } else {
                    IndexColumn::Column(unquote(raw).to_string())
                }
            })
            .collect();
        if columns.is_empty() {
            continue;
        }

        let mut unique = false;
        let mut pk = false;
        let mut name = None;
        let mut index_type = None;
        if let Some(open_i) = open
            && let Some(close_rel) = ts[open_i..]
                .iter()
                .position(|k| k.kind() == SyntaxKind::RBracket)
        {
            for part in ts[open_i + 1..open_i + close_rel].split(|k| k.kind() == SyntaxKind::Comma)
            {
                let Some(first) = part.first() else { continue };
                let kw = first.text().to_ascii_lowercase();
                if kw == "unique" {
                    unique = true;
                } else if kw == "pk" {
                    pk = true;
                } else if kw == "name"
                    && part.get(1).is_some_and(|k| k.kind() == SyntaxKind::Colon)
                    && let Some(val) = part.get(2)
                {
                    name = Some(unquote(val.text()).to_string());
                } else if kw == "type"
                    && part.get(1).is_some_and(|k| k.kind() == SyntaxKind::Colon)
                    && let Some(val) = part.get(2)
                {
                    index_type = IndexType::from_dbml_str(unquote(val.text()));
                }
            }
        }

        out.push(IndexDef {
            columns,
            unique,
            pk,
            name,
            index_type,
        });
    }
}

fn column(
    node: &SyntaxNode,
    table_schema: Option<&str>,
    table: &str,
    source: &str,
    refs: &mut Vec<Ref>,
) -> Option<Column> {
    let toks = tokens(node);
    let name = unquote(toks.first()?.text()).to_string();
    // tipo: tokens após o nome até `[`, incluindo `(…)`, ex.: varchar(255)
    let mut ty = String::new();
    let mut i = 1;
    while i < toks.len() && toks[i].kind() != SyntaxKind::LBracket {
        ty.push_str(toks[i].text());
        i += 1;
    }
    let (mut pk, mut fk, mut not_null, mut unique, mut increment) =
        (false, false, false, false, false);
    let mut nullability = Nullability::Unspecified;
    let mut default_value = None;
    let mut col_note = None;
    let mut col_check = None;

    if i < toks.len() {
        let close_rel = toks[i..]
            .iter()
            .position(|t| t.kind() == SyntaxKind::RBracket)
            .unwrap_or(toks.len() - i);
        let settings = &toks[i + 1..i + close_rel];
        let setting_texts: Vec<&str> = settings.iter().map(rowan::SyntaxToken::text).collect();
        let (on_delete, on_update) = parse_ref_actions(&setting_texts);

        for part in settings.split(|t| t.kind() == SyntaxKind::Comma) {
            let Some(first) = part.first() else { continue };
            let kw = first.text().to_ascii_lowercase();
            match kw.as_str() {
                "pk" | "primary" => pk = true,
                "fk" => fk = true,
                "unique" => unique = true,
                "increment" => increment = true,
                "null" => nullability = Nullability::Nullable,
                "not"
                    if part
                        .get(1)
                        .is_some_and(|t| t.text().eq_ignore_ascii_case("null")) =>
                {
                    not_null = true;
                    nullability = Nullability::NotNull;
                }
                "default"
                    if part.get(1).is_some_and(|t| t.kind() == SyntaxKind::Colon)
                        && part.len() >= 3 =>
                {
                    if let (Some(vs), Some(ve)) = (part.get(2), part.last()) {
                        let s = usize::from(vs.text_range().start());
                        let e = usize::from(ve.text_range().end());
                        if s <= e && e <= source.len() {
                            default_value = Some(source[s..e].to_string());
                        }
                    }
                }
                "note"
                    if part.get(1).is_some_and(|t| t.kind() == SyntaxKind::Colon)
                        && part.len() >= 3 =>
                {
                    if let Some(val) = part.get(2) {
                        col_note = Some(unquote(val.text()).to_string());
                    }
                }
                "check"
                    if part.get(1).is_some_and(|t| t.kind() == SyntaxKind::Colon)
                        && part.len() >= 3 =>
                {
                    if let (Some(vs), Some(ve)) = (part.get(2), part.last()) {
                        let s = usize::from(vs.text_range().start());
                        let e = usize::from(ve.text_range().end());
                        if s <= e && e <= source.len() {
                            col_check = Some(unquote(source[s..e].trim()).to_string());
                        }
                    }
                }
                "ref" => {
                    fk = true;
                    let rest: Vec<&str> = part[1..].iter().map(rowan::SyntaxToken::text).collect();
                    if let Some(mut r) = parse_ref_tail(
                        &rest,
                        Endpoint {
                            schema: table_schema.map(str::to_string),
                            table: table.into(),
                            column: name.clone(),
                            columns: vec![name.clone()],
                        },
                    ) {
                        r.on_delete = on_delete;
                        r.on_update = on_update;
                        refs.push(r);
                    }
                }
                _ => {}
            }
        }
    }
    Some(Column {
        name,
        ty,
        pk,
        fk,
        not_null,
        unique,
        default_value,
        increment,
        nullability,
        note: col_note,
        check: col_check,
    })
}

/// Parses `[schema.]table` from a `TableGroup` member item.
pub(crate) fn qualified_member(toks: &[SyntaxToken]) -> Option<String> {
    match toks {
        [s, dot, t, ..]
            if matches!(s.kind(), SyntaxKind::Ident | SyntaxKind::Str)
                && dot.text() == "."
                && matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::Str) =>
        {
            Some(format!("{}.{}", unquote(s.text()), unquote(t.text())))
        }
        [first, ..] if matches!(first.kind(), SyntaxKind::Ident | SyntaxKind::Str) => {
            Some(unquote(first.text()).to_string())
        }
        _ => None,
    }
}

fn group(node: &SyntaxNode) -> Option<Group> {
    let head = header(node);
    let name = head
        .iter()
        .filter(|t| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::Str))
        .nth(1)
        .map(|t| unquote(t.text()).to_string())?;
    let color = setting(&head, "color");
    let tables = node
        .children()
        .filter(|n| n.kind() == SyntaxKind::Column)
        .filter_map(|n| qualified_member(&tokens(&n)))
        .collect();
    Some(Group {
        name,
        tables,
        color,
    })
}

/// `: > users.id` (a partir do token depois de `ref`) com a origem já conhecida.
fn parse_ref_tail(rest: &[&str], from: Endpoint) -> Option<Ref> {
    let rest: Vec<&str> = rest.iter().copied().filter(|s| *s != ":").collect();
    let kind = RefKind::from_dbml_op(rest.first()?)?;
    let (to, _) = endpoint(&rest[1..])?;
    Some(Ref {
        from,
        to,
        kind,
        on_delete: None,
        on_update: None,
    })
}

fn parse_ref_actions(toks: &[&str]) -> (Option<RefAction>, Option<RefAction>) {
    let mut on_delete = None;
    let mut on_update = None;
    for part in toks.split(|t| *t == "," || *t == "[" || *t == "]") {
        let Some((key, rest)) = part.split_first() else {
            continue;
        };
        if rest.first().copied() != Some(":") || rest.len() < 2 {
            continue;
        }
        let val = rest[1..].join(" ");
        let action = RefAction::from_dbml_str(&val);
        if key.eq_ignore_ascii_case("delete") {
            on_delete = action;
        } else if key.eq_ignore_ascii_case("update") {
            on_update = action;
        }
    }
    (on_delete, on_update)
}

/// Parses `[schema.]table.column` or `[schema.]table.(c1, c2, ...)` and returns `(Endpoint, consumed_tokens)`.
pub(crate) fn endpoint(toks: &[&str]) -> Option<(Endpoint, usize)> {
    let (schema, table, col_start) = match toks {
        [s, ".", t, ".", ..] => (Some(unquote(s).to_string()), unquote(t).to_string(), 4usize),
        [t, ".", ..] => {
            let raw = unquote(t);
            if let Some((s, tbl)) = raw.split_once('.') {
                (Some(s.to_string()), tbl.to_string(), 2usize)
            } else {
                (None, raw.to_string(), 2usize)
            }
        }
        _ => return None,
    };

    let rest = toks.get(col_start..)?;
    if rest.first().copied() == Some("(") {
        let close_rel = rest.iter().position(|t| *t == ")")?;
        let cols: Vec<String> = rest[1..close_rel]
            .iter()
            .copied()
            .filter(|t| *t != ",")
            .map(|t| unquote(t).to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let primary = cols.first()?.clone();
        Some((
            Endpoint {
                schema,
                table,
                column: primary,
                columns: cols,
            },
            col_start + close_rel + 1,
        ))
    } else {
        let col = unquote(rest.first()?).to_string();
        if col.is_empty() {
            return None;
        }
        Some((
            Endpoint {
                schema,
                table,
                column: col.clone(),
                columns: vec![col],
            },
            col_start + 1,
        ))
    }
}

/// `Ref [nome]: a.b > c.d` ou `Ref: s.a.(x, y) > s.c.(x, y)`
fn top_level_ref(node: &SyntaxNode) -> Option<Ref> {
    let toks = tokens(node);
    let texts: Vec<&str> = toks.iter().map(rowan::SyntaxToken::text).collect();
    let colon = texts.iter().position(|t| *t == ":")?;
    let body = &texts[colon + 1..];
    let (from, mut k) = endpoint(body)?;
    // operador logo após o endpoint `from`; `<` `>` `-` `<>` podem vir como um ou dois tokens `Other`
    let mut op = String::new();
    while k < body.len() && matches!(body[k], "<" | ">" | "-") {
        op.push_str(body[k]);
        k += 1;
    }
    let kind = RefKind::from_dbml_op(&op)?;
    let (to, to_len) = endpoint(&body[k..])?;
    let (on_delete, on_update) = parse_ref_actions(&body[k + to_len..]);
    Some(Ref {
        from,
        to,
        kind,
        on_delete,
        on_update,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "\
Table users {
  id int [pk]
  email varchar(255) [unique, not null]
}

Table posts {
  id int [pk]
  author_id int [ref: > users.id]
  // comentário
  title text
}

Ref: posts.author_id > users.id
";

    #[test]
    fn tables_and_columns() {
        let s = analyze(SRC);
        assert_eq!(s.tables.len(), 2);
        let users = s.table("users").unwrap();
        assert_eq!(users.columns[1].ty, "varchar(255)");
        assert!(users.columns[0].pk);
        assert!(users.columns[1].unique && users.columns[1].not_null);
        assert_eq!(s.table("posts").unwrap().columns.len(), 3);
    }

    #[test]
    fn refs_inline_and_top_level() {
        let s = analyze(SRC);
        assert_eq!(s.refs.len(), 2);
        for r in &s.refs {
            assert_eq!(
                r.from,
                Endpoint {
                    schema: None,
                    table: "posts".into(),
                    column: "author_id".into(),
                    columns: vec!["author_id".into()],
                }
            );
            assert_eq!(
                r.to,
                Endpoint {
                    schema: None,
                    table: "users".into(),
                    column: "id".into(),
                    columns: vec!["id".into()],
                }
            );
            assert_eq!(r.kind, RefKind::ManyToOne);
        }
    }

    #[test]
    fn groups_and_header_colors() {
        let s = analyze(
            "Table a [headercolor: #3498DB] {\n  id int\n}\nTable b {\n  id int\n}\nTableGroup core [color: #22c55e] {\n  a\n  b\n}\n",
        );
        assert_eq!(s.table("a").unwrap().color.as_deref(), Some("#3498DB"));
        assert_eq!(s.table("b").unwrap().color, None);
        assert_eq!(s.groups.len(), 1);
        assert_eq!(s.groups[0].tables, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(s.groups[0].color.as_deref(), Some("#22c55e"));
        assert_eq!(s.group_of("b").unwrap().name, "core");
    }

    #[test]
    fn broken_input_does_not_panic() {
        let _ = analyze("Table {\n Table x { id\n Ref: a. > \n");
    }

    #[test]
    fn parses_top_level_ref_with_delete_and_update_actions() {
        let src = "Table users {\n  id int [pk]\n}\nTable posts {\n  id int [pk]\n  user_id int\n}\nRef: posts.user_id > users.id [delete: cascade, update: set null]\n";
        let s = analyze(src);
        assert_eq!(s.refs.len(), 1);
        assert_eq!(s.refs[0].on_delete, Some(RefAction::Cascade));
        assert_eq!(s.refs[0].on_update, Some(RefAction::SetNull));
    }

    #[test]
    fn parses_inline_ref_with_delete_and_update_actions() {
        let src = "Table users {\n  id int [pk]\n}\nTable posts {\n  id int [pk]\n  user_id int [ref: > users.id, delete: restrict, update: no action]\n}\n";
        let s = analyze(src);
        assert_eq!(s.refs.len(), 1);
        assert_eq!(s.refs[0].on_delete, Some(RefAction::Restrict));
        assert_eq!(s.refs[0].on_update, Some(RefAction::NoAction));
    }

    #[test]
    fn parses_qualified_tables_aliases_composite_refs_enums_and_notes() {
        let src = "\
Project app {
  Note: 'Core database'
}
Enum auth.role {
  admin [note: 'Full access']
  user
}
Table auth.users as u {
  id int [pk, increment]
  org_id int [not null]
  bio text [null, note: 'User biography']
  Note: 'Accounts'
}
Table public.orders {
  id int [pk]
  org_id int
  user_id int
  indexes {
    (`lower(id)`) [type: hash, name: 'idx_orders_id']
  }
}
Ref: public.orders.(org_id, user_id) > u.(org_id, id) [delete: cascade]
";
        let s = analyze(src);
        assert_eq!(s.project_note.as_deref(), Some("Core database"));
        assert_eq!(s.enums.len(), 1);
        assert_eq!(s.enums[0].schema.as_deref(), Some("auth"));
        assert_eq!(s.enums[0].name, "role");
        assert_eq!(s.enums[0].values[0].note.as_deref(), Some("Full access"));

        let u = s.table("users").unwrap();
        assert_eq!(u.schema.as_deref(), Some("auth"));
        assert_eq!(u.alias.as_deref(), Some("u"));
        assert_eq!(u.note.as_deref(), Some("Accounts"));
        assert!(u.columns[0].increment);
        assert_eq!(u.columns[2].nullability, Nullability::Nullable);
        assert_eq!(u.columns[2].note.as_deref(), Some("User biography"));

        let orders = s.table("orders").unwrap();
        assert_eq!(orders.indexes.len(), 1);
        assert_eq!(orders.indexes[0].index_type, Some(IndexType::Hash));
        assert!(orders.columns[1].fk && orders.columns[2].fk);

        assert_eq!(s.refs.len(), 1);
        assert_eq!(s.refs[0].from.cols(), vec!["org_id", "user_id"]);
        assert_eq!(s.refs[0].to.schema.as_deref(), Some("auth"));
        assert_eq!(s.refs[0].to.table, "users");
        assert_eq!(s.refs[0].to.cols(), vec!["org_id", "id"]);
    }

    #[test]
    fn parses_column_check_and_default_without_lint_errors() {
        let src = "\
Table products {
  id int [pk]
  price decimal [not null, default: 0, check: `price >= 0`]
  sku varchar(32) [unique]
  indexes {
    (sku, price) [unique, type: btree, name: 'idx_products_sku_price']
  }
}
";
        let s = analyze(src);
        let diags = crate::lint::lint(src);
        assert!(diags.is_empty(), "expected no lint diagnostics: {diags:?}");
        let p = s.table("products").unwrap();
        assert_eq!(p.columns[1].default_value.as_deref(), Some("0"));
        assert_eq!(p.columns[1].check.as_deref(), Some("price >= 0"));
        assert_eq!(p.indexes.len(), 1);
        assert!(p.indexes[0].unique);
        assert_eq!(p.indexes[0].index_type, Some(IndexType::BTree));
        assert_eq!(p.indexes[0].name.as_deref(), Some("idx_products_sku_price"));
    }
}
