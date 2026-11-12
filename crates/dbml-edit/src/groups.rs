//! Edições de organização: agrupar/desagrupar tabelas (`TableGroup`) e cor da
//! tabela (`headercolor`). Cada função devolve o texto novo, tocando só os
//! trechos necessários (comentários e formatação do resto ficam intactos).

use dbml_syntax::{SyntaxKind, SyntaxNode, child_tokens as sig, parse, unquote};
use rowan::TextRange;
use std::fmt::Write as _;

fn name_of(n: &SyntaxNode) -> Option<String> {
    sig(n)
        .iter()
        .filter(|t| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::Str))
        .nth(1)
        .map(|t| unquote(t.text()).to_string())
}

fn member(line: &SyntaxNode) -> Option<String> {
    line.descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| !t.kind().is_trivia())
        .map(|t| unquote(t.text()).to_string())
}

/// Remove `ranges` (ordenados ou não) do texto.
fn cut(text: &str, mut ranges: Vec<std::ops::Range<usize>>) -> String {
    ranges.sort_by_key(|r| std::cmp::Reverse(r.start));
    let mut out = text.to_string();
    for r in ranges {
        out.replace_range(r, "");
    }
    out
}

/// Intervalo da linha inteira (do início da linha até depois do `\n`).
fn whole_line(text: &str, r: TextRange) -> std::ops::Range<usize> {
    let (s, e) = (usize::from(r.start()), usize::from(r.end()));
    let start = text[..s].rfind('\n').map_or(0, |i| i + 1);
    let end = text[e..].find('\n').map_or(text.len(), |i| e + i + 1);
    start..end
}

/// Tira `tables` dos grupos em que estão; grupos que ficam vazios são removidos.
fn remove_members(text: &str, tables: &[&str]) -> String {
    let root = parse(text).syntax();
    let mut cuts = Vec::new();
    for g in root
        .children()
        .filter(|n| n.kind() == SyntaxKind::TableGroup)
    {
        let lines: Vec<_> = g
            .children()
            .filter(|n| n.kind() == SyntaxKind::Column)
            .collect();
        let gone: Vec<_> = lines
            .iter()
            .filter(|l| member(l).is_some_and(|m| tables.contains(&m.as_str())))
            .collect();
        if gone.is_empty() {
            continue;
        }
        if gone.len() == lines.len() {
            let mut r = whole_line(text, g.text_range());
            // leva junto uma linha em branco que sobraria antes do bloco
            if text[..r.start].ends_with("\n\n") {
                r.start -= 1;
            }
            cuts.push(r);
        } else {
            cuts.extend(gone.iter().map(|l| whole_line(text, l.text_range())));
        }
    }
    cut(text, cuts)
}

/// Cria `TableGroup name { ... }` no fim do texto com `tables`, tirando-as de grupos antigos.
#[must_use]
pub fn group_tables(text: &str, tables: &[&str], name: &str) -> String {
    let mut out = remove_members(text, tables);
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    let _ = writeln!(out, "TableGroup {name} {{");
    for t in tables {
        let _ = writeln!(out, "  {t}");
    }
    out.push_str("}\n");
    out
}

/// Tira a tabela do grupo em que estiver.
#[must_use]
pub fn ungroup_table(text: &str, table: &str) -> String {
    remove_members(text, &[table])
}

/// First available `group_N` name.
#[must_use]
pub fn next_group_name(text: &str) -> String {
    let root = parse(text).syntax();
    let names: Vec<String> = root
        .children()
        .filter(|n| n.kind() == SyntaxKind::TableGroup)
        .filter_map(|n| name_of(&n))
        .collect();
    (1..=names.len() + 1)
        .map(|i| format!("group_{i}"))
        .find(|n| !names.contains(n))
        .unwrap_or_else(|| format!("group_{}", names.len() + 1))
}

/// Define (ou remove, com `None`) `headercolor` no cabeçalho da tabela.
#[must_use]
pub fn set_table_color(text: &str, table: &str, color: Option<&str>) -> Option<String> {
    let root = parse(text).syntax();
    let t = root
        .children()
        .filter(|n| n.kind() == SyntaxKind::Table)
        .find(|n| name_of(n).as_deref() == Some(table))?;
    let head: Vec<_> = sig(&t)
        .into_iter()
        .take_while(|k| k.kind() != SyntaxKind::LBrace)
        .collect();
    let name_tok = head
        .iter()
        .filter(|k| matches!(k.kind(), SyntaxKind::Ident | SyntaxKind::Str))
        .nth(1)?;
    let open = head.iter().position(|k| k.kind() == SyntaxKind::LBracket);
    let close = head.iter().position(|k| k.kind() == SyntaxKind::RBracket);
    let mut out = text.to_string();
    match (open, close) {
        (Some(o), Some(c)) if o < c => {
            // reescreve as opções, trocando/removendo headercolor e mantendo as demais
            let inner_start = usize::from(head[o].text_range().end());
            let inner_end = usize::from(head[c].text_range().start());
            let mut parts: Vec<String> = text[inner_start..inner_end]
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty() && !p.to_ascii_lowercase().starts_with("headercolor"))
                .collect();
            if let Some(c) = color {
                parts.push(format!("headercolor: {c}"));
            }
            let whole =
                usize::from(head[o].text_range().start())..usize::from(head[c].text_range().end());
            // sem opções restantes, remove os colchetes e o espaço antes deles
            if parts.is_empty() {
                let start = text[..whole.start].trim_end().len();
                out.replace_range(start..whole.end, "");
            } else {
                out.replace_range(whole, &format!("[{}]", parts.join(", ")));
            }
        }
        _ => {
            let c = color?;
            let at = usize::from(name_tok.text_range().end());
            out.insert_str(at, &format!(" [headercolor: {c}]"));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: &str = "// topo\nTable a {\n  id int\n}\n\nTable b {\n  id int\n}\n";

    #[test]
    fn group_and_ungroup_roundtrip() {
        let g = group_tables(S, &["a", "b"], "core");
        assert!(g.starts_with(S.trim_end()), "{g}");
        assert!(g.ends_with("TableGroup core {\n  a\n  b\n}\n"), "{g}");
        let one = ungroup_table(&g, "a");
        assert!(one.contains("TableGroup core {\n  b\n}\n"), "{one}");
        let none = ungroup_table(&one, "b");
        assert_eq!(none, S, "grupo vazio some e o texto volta ao original");
    }

    #[test]
    fn regroup_moves_table_between_groups() {
        let g = group_tables(S, &["a", "b"], "core");
        let g2 = group_tables(&g, &["b"], "extra");
        assert!(g2.contains("TableGroup core {\n  a\n}\n"), "{g2}");
        assert!(g2.contains("TableGroup extra {\n  b\n}\n"), "{g2}");
        assert_eq!(next_group_name(&g2), "group_1");
    }

    #[test]
    fn header_color_add_replace_remove() {
        let c = set_table_color(S, "a", Some("#3b82f6")).unwrap();
        assert!(c.contains("Table a [headercolor: #3b82f6] {"), "{c}");
        let c2 = set_table_color(&c, "a", Some("#22c55e")).unwrap();
        assert!(c2.contains("Table a [headercolor: #22c55e] {"), "{c2}");
        let back = set_table_color(&c2, "a", None).unwrap();
        assert_eq!(back, S);
        let keep =
            set_table_color("Table a [note: 'x', headercolor: #fff] {\n}\n", "a", None).unwrap();
        assert_eq!(keep, "Table a [note: 'x'] {\n}\n");
    }
}
