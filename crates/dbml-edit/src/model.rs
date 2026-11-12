//! Edições estruturais feitas pela UI (canvas/inspector). Cada função devolve o texto
//! novo tocando só o necessário; comentários e formatação do resto ficam intactos.

use dbml_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, descendant_tokens as sig, parse, unquote};
use std::ops::Range;

fn range(t: &SyntaxToken) -> Range<usize> {
    usize::from(t.text_range().start())..usize::from(t.text_range().end())
}

fn tables(root: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    root.children().filter(|n| n.kind() == SyntaxKind::Table)
}

/// Token do nome no cabeçalho `Table nome ...`.
fn name_token(t: &SyntaxNode) -> Option<SyntaxToken> {
    t.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|k| matches!(k.kind(), SyntaxKind::Ident | SyntaxKind::Str))
        .nth(1)
}

fn find_table(root: &SyntaxNode, name: &str) -> Option<SyntaxNode> {
    tables(root).find(|t| name_token(t).is_some_and(|k| unquote(k.text()) == name))
}

fn find_column(t: &SyntaxNode, col: &str) -> Option<SyntaxNode> {
    t.children()
        .filter(|n| n.kind() == SyntaxKind::Column)
        .find(|c| sig(c).first().is_some_and(|k| unquote(k.text()) == col))
}

/// Aplica substituições (intervalo → texto), da última para a primeira.
fn apply(text: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    edits.dedup_by_key(|(r, _)| r.start);
    let mut out = text.to_string();
    for (r, s) in edits {
        out.replace_range(r, &s);
    }
    out
}

/// Linha inteira (com o `\n`) que contém o intervalo.
fn line_of(text: &str, r: Range<usize>) -> Range<usize> {
    let start = text[..r.start].rfind('\n').map_or(0, |i| i + 1);
    let end = text[r.end..]
        .find('\n')
        .map_or(text.len(), |i| r.end + i + 1);
    start..end
}

fn append_block(text: &str, block: &str) -> String {
    let mut out = text.trim_end_matches('\n').to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(block);
    out
}

/// Primeiro nome livre `base`, `base_2`, `base_3`...
#[must_use]
pub fn unique_table_name(text: &str, base: &str) -> String {
    let root = parse(text).syntax();
    let taken: Vec<String> = tables(&root)
        .filter_map(|t| name_token(&t))
        .map(|k| unquote(k.text()).to_string())
        .collect();
    std::iter::once(base.to_string())
        .chain((2..=taken.len() + 2).map(|i| format!("{base}_{i}")))
        .find(|n| !taken.contains(n))
        .unwrap_or_else(|| format!("{base}_{}", taken.len() + 2))
}

/// Primeiro nome de coluna livre na tabela.
#[must_use]
pub fn unique_column_name(text: &str, table: &str, base: &str) -> String {
    let root = parse(text).syntax();
    let taken: Vec<String> = find_table(&root, table)
        .map(|t| {
            t.children()
                .filter(|n| n.kind() == SyntaxKind::Column)
                .filter_map(|c| sig(&c).first().map(|k| unquote(k.text()).to_string()))
                .collect()
        })
        .unwrap_or_default();
    std::iter::once(base.to_string())
        .chain((2..=taken.len() + 2).map(|i| format!("{base}_{i}")))
        .find(|n| !taken.contains(n))
        .unwrap_or_else(|| format!("{base}_{}", taken.len() + 2))
}

/// Appends a new `Table name { id int [pk] }` block to `text`.
#[must_use]
pub fn add_table(text: &str, name: &str) -> String {
    append_block(text, &format!("Table {name} {{\n  id int [pk]\n}}\n"))
}

/// Extrai os endpoints `(from_table, from_col, to_table, to_col)` de um nó `SyntaxKind::Ref`.
fn parse_top_ref_endpoints(r: &SyntaxNode) -> Option<(String, String, String, String)> {
    let toks = sig(r);
    let colon = toks.iter().position(|t| t.text() == ":")?;
    let body = &toks[colon + 1..];
    if body.len() < 7 || body[1].text() != "." {
        return None;
    }
    let from_t = unquote(body[0].text()).to_string();
    let from_c = unquote(body[2].text()).to_string();
    let mut k = 3;
    while k < body.len() && matches!(body[k].text(), "<" | ">" | "-") {
        k += 1;
    }
    if body.get(k + 1).map(rowan::SyntaxToken::text) != Some(".") || k + 2 >= body.len() {
        return None;
    }
    let to_t = unquote(body[k].text()).to_string();
    let to_c = unquote(body[k + 2].text()).to_string();
    Some((from_t, from_c, to_t, to_c))
}

/// Remove a tabela, as `Ref:` de topo que a citam, as colunas FK `id_<name>` nas tabelas filhas e a sua linha em grupos.
#[must_use]
pub fn delete_table(text: &str, name: &str) -> String {
    let root = parse(text).syntax();
    let mut edits = Vec::new();
    let mut fk_cols_to_remove: Vec<(String, String)> = Vec::new();

    if let Some(t) = find_table(&root, name) {
        let mut r = line_of(
            text,
            usize::from(t.text_range().start())..usize::from(t.text_range().end()),
        );
        if text[..r.start].ends_with("\n\n") {
            r.start -= 1;
        }
        edits.push((r, String::new()));
    }
    for r in root.children().filter(|n| n.kind() == SyntaxKind::Ref) {
        if let Some((ft, fc, tt, _)) = parse_top_ref_endpoints(&r)
            && tt == name
            && ft != name
            && fc.starts_with(&format!("id_{name}"))
        {
            fk_cols_to_remove.push((ft.clone(), fc.clone()));
        }
        let toks = sig(&r);
        if toks
            .windows(2)
            .any(|w| unquote(w[0].text()) == name && w[1].text() == ".")
        {
            edits.push((
                line_of(
                    text,
                    usize::from(r.text_range().start())..usize::from(r.text_range().end()),
                ),
                String::new(),
            ));
        }
    }
    for (ft, fc) in fk_cols_to_remove {
        if let Some(t_node) = find_table(&root, &ft)
            && let Some(c_node) = find_column(&t_node, &fc)
        {
            edits.push((
                line_of(
                    text,
                    usize::from(c_node.text_range().start())
                        ..usize::from(c_node.text_range().end()),
                ),
                String::new(),
            ));
        }
    }
    let out = apply(text, edits);
    crate::ungroup_table(&out, name)
}

/// Renomeia a tabela no cabeçalho, em `tabela.coluna` (refs de topo e inline), colunas FK `id_<old>` e nos grupos.
#[must_use]
pub fn rename_table(text: &str, old: &str, new: &str) -> String {
    let root = parse(text).syntax();
    let mut edits = Vec::new();
    if let Some(k) = find_table(&root, old).and_then(|t| name_token(&t)) {
        edits.push((range(&k), new.to_string()));
    }
    let toks: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !t.kind().is_trivia())
        .collect();
    for w in toks.windows(2) {
        if unquote(w[0].text()) == old && w[1].text() == "." {
            edits.push((range(&w[0]), new.to_string()));
        }
    }
    for g in root
        .children()
        .filter(|n| n.kind() == SyntaxKind::TableGroup)
    {
        for line in g.children().filter(|n| n.kind() == SyntaxKind::Column) {
            if let Some(k) = sig(&line).first().filter(|k| unquote(k.text()) == old) {
                edits.push((range(k), new.to_string()));
            }
        }
    }
    let mut out = apply(text, edits);
    // Se alguma tabela possui coluna `id_<old>`, renomeia para `id_<new>` atualizando também as Refs
    let old_fk = format!("id_{old}");
    let new_fk = format!("id_{new}");
    let root2 = parse(&out).syntax();
    let tables_with_fk: Vec<String> = tables(&root2)
        .filter_map(|t| {
            let tname = unquote(name_token(&t)?.text()).to_string();
            find_column(&t, &old_fk).map(|_| tname)
        })
        .collect();
    for tname in tables_with_fk {
        if let Some(updated) = rename_column(&out, &tname, &old_fk, &new_fk) {
            out = updated;
        }
    }
    out
}

/// Inserts a new column `name ty` into `table`, returning `None` if `table` is missing.
#[must_use]
pub fn add_column(text: &str, table: &str, name: &str, ty: &str) -> Option<String> {
    let cmd = crate::Command::AddColumn {
        table: table.into(),
        name: name.into(),
        ty: ty.into(),
    };
    crate::edit(text, &cmd).ok().map(|p| p.apply(text))
}

/// Remove a coluna e todas as `Ref:` de topo que referenciam `table.col`.
#[must_use]
pub fn delete_column(text: &str, table: &str, col: &str) -> Option<String> {
    let root = parse(text).syntax();
    let c = find_column(&find_table(&root, table)?, col)?;
    let mut edits = vec![(
        line_of(
            text,
            usize::from(c.text_range().start())..usize::from(c.text_range().end()),
        ),
        String::new(),
    )];
    for r in root.children().filter(|n| n.kind() == SyntaxKind::Ref) {
        if let Some((ft, fc, tt, tc)) = parse_top_ref_endpoints(&r)
            && ((ft == table && fc == col) || (tt == table && tc == col))
        {
            edits.push((
                line_of(
                    text,
                    usize::from(r.text_range().start())..usize::from(r.text_range().end()),
                ),
                String::new(),
            ));
        }
    }
    Some(apply(text, edits).trim_end().to_string() + "\n")
}

/// Renames column `old` to `new` in `table` and updates all `table.old` references.
#[must_use]
pub fn rename_column(text: &str, table: &str, old: &str, new: &str) -> Option<String> {
    let root = parse(text).syntax();
    let c = find_column(&find_table(&root, table)?, old)?;
    let mut edits = vec![(range(sig(&c).first()?), new.to_string())];
    // `tabela.coluna` em qualquer lugar
    let toks: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !t.kind().is_trivia())
        .collect();
    for w in toks.windows(3) {
        if unquote(w[0].text()) == table && w[1].text() == "." && unquote(w[2].text()) == old {
            edits.push((range(&w[2]), new.to_string()));
        }
    }
    Some(apply(text, edits))
}

/// Updates the data type of column `col` in `table` to `ty`.
#[must_use]
pub fn set_column_type(text: &str, table: &str, col: &str, ty: &str) -> Option<String> {
    let root = parse(text).syntax();
    let c = find_column(&find_table(&root, table)?, col)?;
    let toks = sig(&c);
    let ty_toks: Vec<_> = toks[1..]
        .iter()
        .take_while(|k| k.kind() != SyntaxKind::LBracket)
        .collect();
    let edit = match (ty_toks.first(), ty_toks.last()) {
        (Some(a), Some(b)) => (
            usize::from(a.text_range().start())..usize::from(b.text_range().end()),
            ty.to_string(),
        ),
        _ => (range(&toks[0]).end..range(&toks[0]).end, format!(" {ty}")),
    };
    Some(apply(text, vec![edit]))
}

/// Liga/desliga `pk`, `not null` ou `unique` nas opções `[...]` da coluna.
#[must_use]
pub fn set_column_flag(text: &str, table: &str, col: &str, flag: &str, on: bool) -> Option<String> {
    let root = parse(text).syntax();
    let c = find_column(&find_table(&root, table)?, col)?;
    let toks = sig(&c);
    let open = toks.iter().position(|k| k.kind() == SyntaxKind::LBracket);
    let close = toks.iter().position(|k| k.kind() == SyntaxKind::RBracket);
    let is_flag = |p: &str| {
        let p = p.trim().to_ascii_lowercase();
        p == flag || (flag == "pk" && p == "primary key") || (flag == "not null" && p == "null")
    };
    match (open, close) {
        (Some(o), Some(cl)) if o < cl => {
            let inner =
                usize::from(toks[o].text_range().end())..usize::from(toks[cl].text_range().start());
            let mut parts: Vec<String> = text[inner.clone()]
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty() && !is_flag(p))
                .collect();
            if on {
                parts.insert(0, flag.to_string());
            }
            let whole =
                usize::from(toks[o].text_range().start())..usize::from(toks[cl].text_range().end());
            let edit = if parts.is_empty() {
                (
                    text[..whole.start].trim_end().len()..whole.end,
                    String::new(),
                )
            } else {
                (whole, format!("[{}]", parts.join(", ")))
            };
            Some(apply(text, vec![edit]))
        }
        _ if on => {
            let end = usize::from(toks.last()?.text_range().end());
            Some(apply(text, vec![(end..end, format!(" [{flag}]"))]))
        }
        _ => Some(text.to_string()),
    }
}

/// `Ref: a.b > c.d` (muitos-para-um) no fim do texto.
#[must_use]
pub fn add_ref(text: &str, from: (&str, &str), to: (&str, &str)) -> String {
    let line = format!("Ref: {}.{} > {}.{}\n", from.0, from.1, to.0, to.1);
    let mut out = text.trim_end_matches('\n').to_string();
    out.push_str(if out.is_empty() { "" } else { "\n" });
    // junta com outras Refs se o texto já termina com uma; senão separa por linha em branco
    let last = out
        .lines()
        .last()
        .unwrap_or("")
        .trim_start()
        .to_ascii_lowercase();
    if !out.is_empty() && !last.starts_with("ref") {
        out.push('\n');
    }
    out.push_str(&line);
    out
}

/// Descobre a coluna PK (ou primeira coluna) de `table` e seu tipo `(nome_col, tipo)`.
fn primary_key_of(root: &SyntaxNode, table: &str) -> Option<(String, String)> {
    let t = find_table(root, table)?;
    let mut first_col: Option<(String, String)> = None;
    for c in t.children().filter(|n| n.kind() == SyntaxKind::Column) {
        let toks = sig(&c);
        let name = unquote(toks.first()?.text()).to_string();
        let mut ty = String::new();
        let mut i = 1;
        while i < toks.len() && toks[i].kind() != SyntaxKind::LBracket {
            ty.push_str(toks[i].text());
            i += 1;
        }
        if ty.is_empty() {
            ty = "int".to_string();
        }
        let is_pk = toks[i..].iter().any(|k| {
            k.text().eq_ignore_ascii_case("pk") || k.text().eq_ignore_ascii_case("primary")
        });
        if is_pk {
            return Some((name, ty));
        }
        if first_col.is_none() {
            first_col = Some((name, ty));
        }
    }
    first_col
}

/// Linka `source_table` à `target_table` criando automaticamente a coluna `id_<source_table>`
/// (classificada como FK) em `target_table` e adicionando `Ref: target_table.id_<source_table> > source_table.<pk>`.
#[must_use]
pub fn link_tables(text: &str, source_table: &str, target_table: &str) -> String {
    if source_table == target_table {
        return text.to_string();
    }
    let root = parse(text).syntax();
    if find_table(&root, source_table).is_none() || find_table(&root, target_table).is_none() {
        return text.to_string();
    }
    let (pk_col, pk_ty) = primary_key_of(&root, source_table)
        .unwrap_or_else(|| ("id".to_string(), "int".to_string()));

    // Idempotência: se já existe Ref entre target_table e source_table.<pk_col>, não duplica
    for r in root.children().filter(|n| n.kind() == SyntaxKind::Ref) {
        if let Some((ft, _, tt, tc)) = parse_top_ref_endpoints(&r)
            && ft == target_table
            && tt == source_table
            && tc == pk_col
        {
            return text.to_string();
        }
    }

    let fk_base = format!("id_{source_table}");
    let fk_col = if find_table(&root, target_table)
        .and_then(|t| find_column(&t, &fk_base))
        .is_some()
    {
        // Se a coluna `id_<source_table>` já existe na tabela e não está ligada a outra Ref, reutiliza
        let used_in_other_ref = root
            .children()
            .filter(|n| n.kind() == SyntaxKind::Ref)
            .filter_map(|r| parse_top_ref_endpoints(&r))
            .any(|(ft, fc, tt, _)| ft == target_table && fc == fk_base && tt != source_table);
        if used_in_other_ref {
            unique_column_name(text, target_table, &fk_base)
        } else {
            fk_base.clone()
        }
    } else {
        fk_base.clone()
    };

    let with_col = if find_table(&root, target_table)
        .and_then(|t| find_column(&t, &fk_col))
        .is_some()
    {
        text.to_string()
    } else {
        add_column(text, target_table, &fk_col, &pk_ty).unwrap_or_else(|| text.to_string())
    };

    add_ref(&with_col, (target_table, &fk_col), (source_table, &pk_col))
}

/// Remove a linha `Ref:` entre `from` e `to` (ou inline) e apaga também a coluna FK `from.1` da tabela `from.0`.
#[must_use]
pub fn delete_ref(text: &str, from: (&str, &str), to: (&str, &str)) -> String {
    let root = parse(text).syntax();
    let mut edits = Vec::new();

    for r in root.children().filter(|n| n.kind() == SyntaxKind::Ref) {
        if let Some((ft, fc, tt, tc)) = parse_top_ref_endpoints(&r)
            && ((ft == from.0 && fc == from.1 && tt == to.0 && tc == to.1)
                || (ft == to.0 && fc == to.1 && tt == from.0 && tc == from.1))
        {
            let mut lr = line_of(
                text,
                usize::from(r.text_range().start())..usize::from(r.text_range().end()),
            );
            if text[..lr.start].ends_with("\n\n") && lr.end == text.len() {
                lr.start -= 1;
            }
            edits.push((lr, String::new()));
        }
    }

    // Apaga também a coluna FK (`from.1` em `from.0`), conforme regra de negócio
    if let Some(t) = find_table(&root, from.0)
        && let Some(c) = find_column(&t, from.1)
    {
        // Só preserva a coluna se ela for a única coluna `id [pk]` original da tabela sem relação com `id_<tabela>`
        let toks = sig(&c);
        let is_sole_id_pk = from.1 == "id"
            && t.children()
                .filter(|n| n.kind() == SyntaxKind::Column)
                .count()
                == 1
            && toks.iter().any(|k| k.text().eq_ignore_ascii_case("pk"));
        if !is_sole_id_pk {
            edits.push((
                line_of(
                    text,
                    usize::from(c.text_range().start())..usize::from(c.text_range().end()),
                ),
                String::new(),
            ));
        }
    }

    let out = apply(text, edits);
    out.trim_end().to_string() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: &str = "// loja\nTable users {\n  id int [pk]\n  email text\n}\n\nTable posts {\n  id int [pk]\n  user_id int [ref: > users.id]\n}\n\nRef: posts.user_id > users.id\n\nTableGroup g {\n  users\n  posts\n}\n";

    #[test]
    fn table_lifecycle() {
        let n = unique_table_name(S, "users");
        assert_eq!(n, "users_2");
        let added = add_table(S, "tags");
        assert!(
            added.ends_with("\n\nTable tags {\n  id int [pk]\n}\n"),
            "{added}"
        );
        let renamed = rename_table(S, "users", "people");
        assert!(renamed.contains("Table people {"));
        assert!(renamed.contains("[ref: > people.id]"));
        assert!(renamed.contains("Ref: posts.user_id > people.id"));
        assert!(renamed.contains("  people\n  posts"));
        assert!(renamed.starts_with("// loja\n"));
        let deleted = delete_table(S, "users");
        assert!(!deleted.contains("Table users"), "{deleted}");
        assert!(!deleted.contains("Ref: posts.user_id"), "{deleted}");
        assert!(deleted.contains("TableGroup g {\n  posts\n}"), "{deleted}");
    }

    #[test]
    fn column_edits() {
        let r = rename_column(S, "users", "id", "uid").unwrap();
        assert!(
            r.contains("  uid int [pk]")
                && r.contains("> users.uid]")
                && r.contains("> users.uid\n"),
            "{r}"
        );
        assert!(
            r.contains("Table posts {\n  id int [pk]"),
            "não mexe em posts.id: {r}"
        );
        let t = set_column_type(S, "users", "email", "varchar(255)").unwrap();
        assert!(t.contains("  email varchar(255)\n"), "{t}");
        let d = delete_column(S, "users", "email").unwrap();
        assert!(d.contains("Table users {\n  id int [pk]\n}"), "{d}");
        assert_eq!(unique_column_name(S, "users", "email"), "email_2");
        let a = add_column(S, "users", "name", "text").unwrap();
        assert!(a.contains("  email text\n  name text\n"), "{a}");
    }

    #[test]
    fn flags() {
        let on = set_column_flag(S, "users", "email", "unique", true).unwrap();
        assert!(on.contains("  email text [unique]\n"), "{on}");
        let both = set_column_flag(&on, "users", "email", "not null", true).unwrap();
        assert!(both.contains("[not null, unique]"), "{both}");
        let off = set_column_flag(&both, "users", "email", "unique", false).unwrap();
        assert!(off.contains("[not null]"), "{off}");
        let none = set_column_flag(&off, "users", "email", "not null", false).unwrap();
        assert!(none.contains("  email text\n"), "{none}");
        let pk_off = set_column_flag(S, "users", "id", "pk", false).unwrap();
        assert!(pk_off.contains("Table users {\n  id int\n"), "{pk_off}");
    }

    #[test]
    fn refs() {
        let r = add_ref(S, ("posts", "id"), ("users", "id"));
        assert!(r.ends_with("}\n\nRef: posts.id > users.id\n"), "{r}");
        let only = add_ref("Table a {\n}\nRef: a.x > b.y\n", ("a", "z"), ("b", "w"));
        assert!(only.ends_with("Ref: a.x > b.y\nRef: a.z > b.w\n"), "{only}");
    }

    #[test]
    fn link_tables_creates_fk_and_delete_ref_removes_column() {
        let src = "Table tabela {\n  id int [pk]\n  coluna int\n}\n\nTable tabela_2 {\n  id int [pk]\n}\n";
        let linked = link_tables(src, "tabela", "tabela_2");
        assert!(
            linked.contains("Table tabela_2 {\n  id int [pk]\n  id_tabela int\n}"),
            "Deve criar id_tabela em tabela_2: {linked}"
        );
        assert!(
            linked.contains("Ref: tabela_2.id_tabela > tabela.id\n"),
            "Deve criar Ref apontando id_tabela para tabela.id: {linked}"
        );

        // Idempotência: linkar de novo não duplica
        let again = link_tables(&linked, "tabela", "tabela_2");
        assert_eq!(again, linked);

        // Permite marcar id_tabela também como PK (relacionamento 1:1 / herança)
        let with_pk_fk = set_column_flag(&linked, "tabela_2", "id_tabela", "pk", true).unwrap();
        assert!(with_pk_fk.contains("id_tabela int [pk]"), "{with_pk_fk}");

        // Excluir a linha (Ref) apaga também a coluna id_tabela
        let after_del = delete_ref(&linked, ("tabela_2", "id_tabela"), ("tabela", "id"));
        assert!(
            !after_del.contains("id_tabela"),
            "Coluna id_tabela deve ser removida ao excluir a linha: {after_del}"
        );
        assert!(
            !after_del.contains("Ref:"),
            "Ref deve ser removida: {after_del}"
        );
    }

    #[test]
    fn rename_and_delete_parent_table_cascades_to_fk_column() {
        let src = "Table tabela {\n  id int [pk]\n}\n\nTable tabela_2 {\n  id int [pk]\n}\n";
        let linked = link_tables(src, "tabela", "tabela_2");
        let renamed = rename_table(&linked, "tabela", "clientes");
        assert!(
            renamed.contains("id_clientes int"),
            "Deve renomear id_tabela para id_clientes: {renamed}"
        );
        assert!(
            renamed.contains("Ref: tabela_2.id_clientes > clientes.id"),
            "Deve atualizar a Ref: {renamed}"
        );

        let deleted = delete_table(&renamed, "clientes");
        assert!(
            !deleted.contains("id_clientes"),
            "Excluir tabela pai deve remover coluna FK na tabela filha: {deleted}"
        );
    }
}
