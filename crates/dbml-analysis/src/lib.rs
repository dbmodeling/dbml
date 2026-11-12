#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Tolerant CST-to-schema semantic analysis, autocompletion, and diagnostics ([`lint()`]).

mod complete;
mod lint;
pub use complete::{Completion, CompletionKind, complete};
pub use lint::{Diagnostic, Severity, lint};

use dbml_syntax::{
    SyntaxKind, SyntaxNode, SyntaxToken, descendant_tokens as tokens, parse, unquote,
};
use schema::{Column, Endpoint, Group, Ref, RefKind, Schema, Table};

/// Parses `text` into a CST and projects it into a semantic [`Schema`].
#[must_use]
pub fn analyze(text: &str) -> Schema {
    let root = parse(text).syntax();
    let mut schema = Schema::default();
    for node in root.children() {
        match node.kind() {
            SyntaxKind::Table => {
                if let Some(t) = table(&node, &mut schema.refs) {
                    schema.tables.push(t);
                }
            }
            SyntaxKind::Ref => schema.refs.extend(top_level_ref(&node)),
            SyntaxKind::TableGroup => schema.groups.extend(group(&node)),
            _ => {}
        }
    }
    // Marca `fk = true` em toda coluna que é origem (`from`) de uma `Ref`
    let fk_cols: std::collections::HashSet<(String, String)> = schema
        .refs
        .iter()
        .map(|r| (r.from.table.clone(), r.from.column.clone()))
        .collect();
    for t in &mut schema.tables {
        for c in &mut t.columns {
            if fk_cols.contains(&(t.name.clone(), c.name.clone())) {
                c.fk = true;
            }
        }
    }
    schema
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

fn table(node: &SyntaxNode, refs: &mut Vec<Ref>) -> Option<Table> {
    let head = header(node);
    let name = head
        .iter()
        .filter(|t| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::Str))
        .nth(1)
        .map(|t| unquote(t.text()).to_string())?;
    let color = setting(&head, "headercolor");
    let mut columns = Vec::new();
    for col in node.children().filter(|n| n.kind() == SyntaxKind::Column) {
        if let Some(c) = column(&col, &name, refs) {
            columns.push(c);
        }
    }
    Some(Table {
        name,
        columns,
        color,
    })
}

fn column(node: &SyntaxNode, table: &str, refs: &mut Vec<Ref>) -> Option<Column> {
    let toks = tokens(node);
    let name = unquote(toks.first()?.text()).to_string();
    // tipo: tokens após o nome até `[`, incluindo `(…)`, ex.: varchar(255)
    let mut ty = String::new();
    let mut i = 1;
    while i < toks.len() && toks[i].kind() != SyntaxKind::LBracket {
        ty.push_str(toks[i].text());
        i += 1;
    }
    let (mut pk, mut fk, mut not_null, mut unique) = (false, false, false, false);
    if i < toks.len() {
        // configurações separadas por vírgula dentro de [...]
        let settings = &toks[i + 1..];
        let mut j = 0;
        while j < settings.len() {
            let w = settings[j].text().to_ascii_lowercase();
            match w.as_str() {
                "pk" | "primary" => pk = true,
                "fk" => fk = true,
                "unique" => unique = true,
                "not"
                    if settings
                        .get(j + 1)
                        .is_some_and(|t| t.text().eq_ignore_ascii_case("null")) =>
                {
                    not_null = true;
                }
                "ref" => {
                    fk = true;
                    // ref: > tabela.coluna
                    let rest: Vec<&str> = settings[j + 1..]
                        .iter()
                        .take_while(|t| {
                            t.kind() != SyntaxKind::Comma && t.kind() != SyntaxKind::RBracket
                        })
                        .map(rowan::SyntaxToken::text)
                        .collect();
                    if let Some(r) = parse_ref_tail(
                        &rest,
                        Endpoint {
                            table: table.into(),
                            column: name.clone(),
                        },
                    ) {
                        refs.push(r);
                    }
                }
                _ => {}
            }
            j += 1;
        }
    }
    Some(Column {
        name,
        ty,
        pk,
        fk,
        not_null,
        unique,
    })
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
        .filter_map(|n| tokens(&n).first().map(|t| unquote(t.text()).to_string()))
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
    let kind = kind_of(rest.first()?)?;
    let to = endpoint(&rest[1..])?;
    Some(Ref { from, to, kind })
}

fn kind_of(s: &str) -> Option<RefKind> {
    Some(match s {
        ">" => RefKind::ManyToOne,
        "<" => RefKind::OneToMany,
        "-" => RefKind::OneToOne,
        "<>" => RefKind::ManyToMany,
        _ => return None,
    })
}

/// `tabela . coluna` (tokens `Ident . Ident`; o lexer entrega `.` como `Other`).
fn endpoint(toks: &[&str]) -> Option<Endpoint> {
    match toks {
        [t, ".", c, ..] => Some(Endpoint {
            table: unquote(t).to_string(),
            column: unquote(c).to_string(),
        }),
        _ => None,
    }
}

/// `Ref [nome]: a.b > c.d`
fn top_level_ref(node: &SyntaxNode) -> Option<Ref> {
    let toks = tokens(node);
    let texts: Vec<&str> = toks.iter().map(rowan::SyntaxToken::text).collect();
    let colon = texts.iter().position(|t| *t == ":")?;
    let body = &texts[colon + 1..];
    let from = endpoint(body)?;
    // operador logo após `a . b`; `<` `>` `-` `<>` podem vir como um ou dois tokens `Other`
    let mut k = 3;
    let mut op = String::new();
    while k < body.len() && matches!(body[k], "<" | ">" | "-") {
        op.push_str(body[k]);
        k += 1;
    }
    let kind = kind_of(&op)?;
    let to = endpoint(&body[k..])?;
    Some(Ref { from, to, kind })
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
                    table: "posts".into(),
                    column: "author_id".into()
                }
            );
            assert_eq!(
                r.to,
                Endpoint {
                    table: "users".into(),
                    column: "id".into()
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
}
