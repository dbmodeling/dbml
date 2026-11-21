//! Edições estruturais feitas pela UI (canvas/inspector). Cada função devolve o texto
//! novo tocando só o necessário; comentários e formatação do resto ficam intactos.

use dbml_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, descendant_tokens as sig, parse, unquote};
use schema::{RefAction, RefKind};
use std::ops::Range;

pub(crate) fn range(t: &SyntaxToken) -> Range<usize> {
    usize::from(t.text_range().start())..usize::from(t.text_range().end())
}

fn tables(root: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    root.children().filter(|n| n.kind() == SyntaxKind::Table)
}

pub(crate) fn bare_table_name(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(_, t)| t)
}

/// Returns `(optional_schema_token, table_name_token)` from a `Table` header.
pub(crate) fn table_header_name(t: &SyntaxNode) -> Option<(Option<SyntaxToken>, SyntaxToken)> {
    let mut header = t
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|k| !k.kind().is_trivia())
        .take_while(|k| k.kind() != SyntaxKind::LBrace);
    // Skip the `Table` keyword.
    header.find(|k| matches!(k.kind(), SyntaxKind::Ident | SyntaxKind::Str))?;
    let first = header.next()?;
    if !matches!(first.kind(), SyntaxKind::Ident | SyntaxKind::Str) {
        return None;
    }
    match (header.next(), header.next()) {
        (Some(dot), Some(second))
            if dot.text() == "."
                && matches!(second.kind(), SyntaxKind::Ident | SyntaxKind::Str) =>
        {
            Some((Some(first), second))
        }
        _ => Some((None, first)),
    }
}

pub(crate) fn table_qualified_name(t: &SyntaxNode) -> Option<String> {
    let (schema_tok, name_tok) = table_header_name(t)?;
    let name = unquote(name_tok.text());
    Some(match schema_tok {
        Some(s) => format!("{}.{name}", unquote(s.text())),
        None => name.to_string(),
    })
}

pub(crate) fn table_name_range(t: &SyntaxNode) -> Option<Range<usize>> {
    let (schema_tok, name_tok) = table_header_name(t)?;
    let start = schema_tok.as_ref().map_or_else(
        || usize::from(name_tok.text_range().start()),
        |s| usize::from(s.text_range().start()),
    );
    Some(start..usize::from(name_tok.text_range().end()))
}

pub(crate) fn find_table(root: &SyntaxNode, name: &str) -> Option<SyntaxNode> {
    if let Some(exact) = tables(root).find(|t| table_qualified_name(t).as_deref() == Some(name)) {
        return Some(exact);
    }
    if !name.contains('.') {
        return tables(root)
            .find(|t| table_header_name(t).is_some_and(|(_, tbl)| unquote(tbl.text()) == name));
    }
    None
}

/// Nomes (sem aspas) das colunas de uma tabela, na ordem do texto.
pub(crate) fn column_names(t: &SyntaxNode) -> Vec<String> {
    t.children()
        .filter(|n| n.kind() == SyntaxKind::Column)
        .filter_map(|c| sig(&c).first().map(|k| unquote(k.text()).to_string()))
        .collect()
}

pub(crate) fn find_column(t: &SyntaxNode, col: &str) -> Option<SyntaxNode> {
    t.children()
        .filter(|n| n.kind() == SyntaxKind::Column)
        .find(|c| sig(c).first().is_some_and(|k| unquote(k.text()) == col))
}

/// Aplica substituições (intervalo → texto), da última para a primeira.
pub(crate) fn apply(text: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    edits.dedup_by_key(|(r, _)| r.start);
    let mut out = text.to_string();
    for (r, s) in edits {
        out.replace_range(r, &s);
    }
    out
}

/// Linha inteira (com o `\n`) que contém o intervalo.
pub(crate) fn line_of(text: &str, r: Range<usize>) -> Range<usize> {
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
        .filter_map(|t| table_qualified_name(&t))
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
pub(crate) fn add_table(text: &str, name: &str) -> String {
    append_block(text, &format!("Table {name} {{\n  id int [pk]\n}}\n"))
}

fn parse_endpoint(toks: &[SyntaxToken]) -> Option<(String, String, usize)> {
    if toks.len() >= 5 && toks[1].text() == "." && toks[3].text() == "." {
        let table = format!("{}.{}", unquote(toks[0].text()), unquote(toks[2].text()));
        let col = unquote(toks[4].text()).to_string();
        Some((table, col, 5))
    } else if toks.len() >= 3 && toks[1].text() == "." {
        let table = unquote(toks[0].text()).to_string();
        let col = unquote(toks[2].text()).to_string();
        Some((table, col, 3))
    } else {
        None
    }
}

/// Extrai os endpoints `(from_table, from_col, to_table, to_col)` de um nó `SyntaxKind::Ref`.
fn parse_top_ref_endpoints(r: &SyntaxNode) -> Option<(String, String, String, String)> {
    let toks = sig(r);
    let colon = toks.iter().position(|t| t.text() == ":")?;
    let body = &toks[colon + 1..];
    let (from_t, from_c, mut k) = parse_endpoint(body)?;
    while k < body.len() && matches!(body[k].text(), "<" | ">" | "-") {
        k += 1;
    }
    let (to_t, to_c, _) = parse_endpoint(body.get(k..)?)?;
    Some((from_t, from_c, to_t, to_c))
}

/// Remove a tabela, as `Ref:` de topo que a citam, as colunas FK `id_<name>` nas tabelas filhas e a sua linha em grupos.
#[must_use]
pub(crate) fn delete_table(text: &str, name: &str) -> String {
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
    let bare_name = bare_table_name(name);
    for r in root.children().filter(|n| n.kind() == SyntaxKind::Ref) {
        if let Some((ft, fc, tt, _)) = parse_top_ref_endpoints(&r) {
            if tt == name && ft != name && fc.starts_with(&format!("id_{bare_name}")) {
                fk_cols_to_remove.push((ft.clone(), fc.clone()));
            }
            if ft == name || tt == name {
                edits.push((
                    line_of(
                        text,
                        usize::from(r.text_range().start())..usize::from(r.text_range().end()),
                    ),
                    String::new(),
                ));
            }
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
pub(crate) fn rename_table(text: &str, old: &str, new: &str) -> String {
    let root = parse(text).syntax();
    let mut edits = Vec::new();
    if let Some(r) = find_table(&root, old).and_then(|t| table_name_range(&t)) {
        edits.push((r, new.to_string()));
    }
    let toks: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !t.kind().is_trivia())
        .collect();
    if let Some((old_s, old_t)) = old.split_once('.') {
        for w in toks.windows(5) {
            if unquote(w[0].text()) == old_s
                && w[1].text() == "."
                && unquote(w[2].text()) == old_t
                && w[3].text() == "."
            {
                let start = usize::from(w[0].text_range().start());
                let end = usize::from(w[2].text_range().end());
                edits.push((start..end, new.to_string()));
            }
        }
    } else {
        for (i, w) in toks.windows(2).enumerate() {
            let prev_is_dot = i > 0 && toks[i - 1].text() == ".";
            let next_after_col_is_dot = toks.get(i + 3).is_some_and(|t| t.text() == ".");
            if !prev_is_dot
                && !next_after_col_is_dot
                && unquote(w[0].text()) == old
                && w[1].text() == "."
            {
                edits.push((range(&w[0]), new.to_string()));
            }
        }
    }
    for g in root
        .children()
        .filter(|n| n.kind() == SyntaxKind::TableGroup)
    {
        for line in g.children().filter(|n| n.kind() == SyntaxKind::Column) {
            let ltoks = sig(&line);
            if let Some((old_s, old_t)) = old.split_once('.') {
                if ltoks.len() >= 3
                    && unquote(ltoks[0].text()) == old_s
                    && ltoks[1].text() == "."
                    && unquote(ltoks[2].text()) == old_t
                {
                    let start = usize::from(ltoks[0].text_range().start());
                    let end = usize::from(ltoks[2].text_range().end());
                    edits.push((start..end, new.to_string()));
                }
            } else if let Some(k) = ltoks.first().filter(|k| unquote(k.text()) == old) {
                edits.push((range(k), new.to_string()));
            }
        }
    }
    let mut out = apply(text, edits);
    // Se alguma tabela possui coluna `id_<old>`, renomeia para `id_<new>` atualizando também as Refs
    let old_bare = bare_table_name(old);
    let new_bare = bare_table_name(new);
    if old_bare != new_bare {
        let old_fk = format!("id_{old_bare}");
        let new_fk = format!("id_{new_bare}");
        let root2 = parse(&out).syntax();
        let tables_with_fk: Vec<String> = tables(&root2)
            .filter_map(|t| {
                let tname = table_qualified_name(&t)?;
                find_column(&t, &old_fk).map(|_| tname)
            })
            .collect();
        for tname in tables_with_fk {
            if let Some(updated) = rename_column(&out, &tname, &old_fk, &new_fk) {
                out = updated;
            }
        }
    }
    out
}

/// Aplica um [`crate::ColumnPreset`] a `table`; `None` se a tabela não existe.
/// Retorna o texto inalterado quando todas as colunas do preset já existem.
#[must_use]
pub(crate) fn add_column_preset(
    text: &str,
    table: &str,
    preset: crate::ColumnPreset,
) -> Option<String> {
    let cmd = crate::Command::AddColumnPreset {
        table: table.into(),
        preset,
    };
    crate::edit(text, &cmd).ok().map(|p| p.apply(text))
}

/// Inserts a new column `name ty` into `table`, returning `None` if `table` is missing.
#[must_use]
pub(crate) fn add_column(text: &str, table: &str, name: &str, ty: &str) -> Option<String> {
    let cmd = crate::Command::AddColumn {
        table: table.into(),
        name: name.into(),
        ty: ty.into(),
    };
    crate::edit(text, &cmd).ok().map(|p| p.apply(text))
}

/// Remove a coluna e todas as `Ref:` de topo que referenciam `table.col`.
#[must_use]
pub(crate) fn delete_column(text: &str, table: &str, col: &str) -> Option<String> {
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

/// Renames column `old` to `new` in `table` and updates all `table.old` and `indexes` references.
#[must_use]
pub(crate) fn rename_column(text: &str, table: &str, old: &str, new: &str) -> Option<String> {
    let root = parse(text).syntax();
    let t_node = find_table(&root, table)?;
    let c = find_column(&t_node, old)?;
    let mut edits = vec![(range(sig(&c).first()?), new.to_string())];
    for idx_block in t_node
        .children()
        .filter(|n| n.kind() == SyntaxKind::Indexes)
    {
        let idx_toks: Vec<_> = idx_block
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|k| k.kind() == SyntaxKind::Newline || !k.kind().is_trivia())
            .collect();
        for line in idx_toks.split(|k| k.kind() == SyntaxKind::Newline) {
            let before_bracket = line.iter().take_while(|k| k.kind() != SyntaxKind::LBracket);
            for tok in before_bracket {
                if matches!(tok.kind(), SyntaxKind::Ident | SyntaxKind::Str)
                    && !tok.text().starts_with('`')
                    && !tok.text().eq_ignore_ascii_case("indexes")
                    && unquote(tok.text()) == old
                {
                    edits.push((range(tok), new.to_string()));
                }
            }
        }
    }
    // `tabela.coluna` ou `schema.tabela.coluna` em qualquer lugar
    let toks: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !t.kind().is_trivia())
        .collect();
    if let Some((sch, tbl)) = table.split_once('.') {
        for w in toks.windows(5) {
            if unquote(w[0].text()) == sch
                && w[1].text() == "."
                && unquote(w[2].text()) == tbl
                && w[3].text() == "."
                && unquote(w[4].text()) == old
            {
                edits.push((range(&w[4]), new.to_string()));
            }
        }
    } else {
        for (i, w) in toks.windows(3).enumerate() {
            let prev_is_dot = i > 0 && toks[i - 1].text() == ".";
            if !prev_is_dot
                && unquote(w[0].text()) == table
                && w[1].text() == "."
                && unquote(w[2].text()) == old
            {
                edits.push((range(&w[2]), new.to_string()));
            }
        }
    }
    Some(apply(text, edits))
}

/// Updates the data type of column `col` in `table` to `ty`.
#[must_use]
pub(crate) fn set_column_type(text: &str, table: &str, col: &str, ty: &str) -> Option<String> {
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
pub(crate) fn set_column_flag(
    text: &str,
    table: &str,
    col: &str,
    flag: &str,
    on: bool,
) -> Option<String> {
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
            let mut parts: Vec<String> = text[inner]
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
pub(crate) fn add_ref(text: &str, from: (&str, &str), to: (&str, &str)) -> String {
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
pub(crate) fn link_tables(text: &str, source_table: &str, target_table: &str) -> String {
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

    let fk_base = format!("id_{}", bare_table_name(source_table));
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
            fk_base
        }
    } else {
        fk_base
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
pub(crate) fn delete_ref(text: &str, from: (&str, &str), to: (&str, &str)) -> String {
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

/// Desired cardinality and referential actions for [`update_ref`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefUpdate {
    /// Target cardinality operator (`>`, `<`, `-`, `<>`).
    pub kind: RefKind,
    /// Desired `ON DELETE` action (`None` clears `delete:`).
    pub on_delete: Option<RefAction>,
    /// Desired `ON UPDATE` action (`None` clears `update:`).
    pub on_update: Option<RefAction>,
}

fn format_actions_parts(on_delete: Option<RefAction>, on_update: Option<RefAction>) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(d) = on_delete {
        parts.push(format!("delete: {}", d.as_dbml_str()));
    }
    if let Some(u) = on_update {
        parts.push(format!("update: {}", u.as_dbml_str()));
    }
    parts
}

fn rewrite_bracket_actions(
    text: &str,
    toks: &[SyntaxToken],
    open_idx: usize,
    close_idx: usize,
    on_delete: Option<RefAction>,
    on_update: Option<RefAction>,
) -> String {
    let inner = &toks[open_idx + 1..close_idx];
    let mut kept: Vec<String> = Vec::new();
    for part in inner.split(|t| t.kind() == SyntaxKind::Comma) {
        let Some(first) = part.first() else { continue };
        let kw = first.text().to_ascii_lowercase();
        if matches!(kw.as_str(), "delete" | "update") {
            continue;
        }
        let Some(last) = part.last() else { continue };
        let start = usize::from(first.text_range().start());
        let end = usize::from(last.text_range().end());
        kept.push(text[start..end].to_string());
    }
    kept.extend(format_actions_parts(on_delete, on_update));
    if kept.is_empty() {
        String::new()
    } else {
        format!("[{}]", kept.join(", "))
    }
}

/// Updates cardinality and `delete:` / `update:` actions on an existing top-level or inline `Ref`.
#[must_use]
pub(crate) fn update_ref(
    text: &str,
    from: (&str, &str),
    to: (&str, &str),
    update: RefUpdate,
) -> String {
    let root = parse(text).syntax();
    let mut edits = Vec::new();

    for r in root.children().filter(|n| n.kind() == SyntaxKind::Ref) {
        let toks = sig(&r);
        let Some(colon) = toks.iter().position(|t| t.text() == ":") else {
            continue;
        };
        let body = &toks[colon + 1..];
        let Some(op_idx) = (3..body.len()).find(|&k| !matches!(body[k].text(), "<" | ">" | "-"))
        else {
            continue;
        };
        if op_idx == 3
            || body.get(1).is_none_or(|t| t.text() != ".")
            || body.get(op_idx + 1).is_none_or(|t| t.text() != ".")
            || body.get(op_idx + 2).is_none()
        {
            continue;
        }
        let (ft, fc) = (unquote(body[0].text()), unquote(body[2].text()));
        let (tt, tc) = (
            unquote(body[op_idx].text()),
            unquote(body[op_idx + 2].text()),
        );
        let forward = ft == from.0 && fc == from.1 && tt == to.0 && tc == to.1;
        let reverse = ft == to.0 && fc == to.1 && tt == from.0 && tc == from.1;
        if !forward && !reverse {
            continue;
        }
        let effective_kind = if reverse {
            match update.kind {
                RefKind::ManyToOne => RefKind::OneToMany,
                RefKind::OneToMany => RefKind::ManyToOne,
                other => other,
            }
        } else {
            update.kind
        };
        let op_start = usize::from(body[3].text_range().start());
        let op_end = usize::from(body[op_idx - 1].text_range().end());
        edits.push((op_start..op_end, effective_kind.as_dbml_op().to_string()));

        let after = op_idx + 3;
        if let Some(open_rel) = body[after..]
            .iter()
            .position(|t| t.kind() == SyntaxKind::LBracket)
            && let Some(close_rel) = body[after + open_rel..]
                .iter()
                .position(|t| t.kind() == SyntaxKind::RBracket)
        {
            let open_i = after + open_rel;
            let close_i = open_i + close_rel;
            let rewritten = rewrite_bracket_actions(
                text,
                body,
                open_i,
                close_i,
                update.on_delete,
                update.on_update,
            );
            let mut b_start = usize::from(body[open_i].text_range().start());
            if rewritten.is_empty() && text[..b_start].ends_with(' ') {
                b_start -= 1;
            }
            let b_end = usize::from(body[close_i].text_range().end());
            edits.push((b_start..b_end, rewritten));
        } else {
            let parts = format_actions_parts(update.on_delete, update.on_update);
            if !parts.is_empty() {
                let insert_at = usize::from(body[op_idx + 2].text_range().end());
                edits.push((insert_at..insert_at, format!(" [{}]", parts.join(", "))));
            }
        }
    }

    if let Some(t) = find_table(&root, from.0)
        && let Some(c) = find_column(&t, from.1)
    {
        let toks = sig(&c);
        if let Some(open_i) = toks.iter().position(|k| k.kind() == SyntaxKind::LBracket)
            && let Some(close_rel) = toks[open_i..]
                .iter()
                .position(|k| k.kind() == SyntaxKind::RBracket)
        {
            let close_i = open_i + close_rel;
            let inner = &toks[open_i + 1..close_i];
            let mut matches_inline_ref = false;
            for part in inner.split(|k| k.kind() == SyntaxKind::Comma) {
                if part
                    .first()
                    .is_some_and(|k| k.text().eq_ignore_ascii_case("ref"))
                    && part.get(1).is_some_and(|k| k.kind() == SyntaxKind::Colon)
                {
                    let mut op_start_i = 2;
                    while op_start_i < part.len()
                        && matches!(part[op_start_i].text(), "<" | ">" | "-")
                    {
                        op_start_i += 1;
                    }
                    if op_start_i > 2
                        && part.get(op_start_i + 1).is_some_and(|k| k.text() == ".")
                        && part.get(op_start_i + 2).is_some()
                        && unquote(part[op_start_i].text()) == to.0
                        && unquote(part[op_start_i + 2].text()) == to.1
                    {
                        matches_inline_ref = true;
                        let op_s = usize::from(part[2].text_range().start());
                        let op_e = usize::from(part[op_start_i - 1].text_range().end());
                        edits.push((op_s..op_e, update.kind.as_dbml_op().to_string()));
                    }
                }
            }
            if matches_inline_ref {
                // Reconstruct the bracket using the updated operator slice if actions changed
                let mut has_action_tokens = false;
                for part in inner.split(|k| k.kind() == SyntaxKind::Comma) {
                    if let Some(first) = part.first()
                        && (first.text().eq_ignore_ascii_case("delete")
                            || first.text().eq_ignore_ascii_case("update"))
                    {
                        has_action_tokens = true;
                        break;
                    }
                }
                if has_action_tokens || update.on_delete.is_some() || update.on_update.is_some() {
                    // Apply operator edit first on a local string if needed, or replace the whole bracket cleanly
                    let b_start = usize::from(toks[open_i].text_range().start());
                    let b_end = usize::from(toks[close_i].text_range().end());
                    let mut kept: Vec<String> = Vec::new();
                    for part in inner.split(|k| k.kind() == SyntaxKind::Comma) {
                        let Some(first) = part.first() else { continue };
                        let kw = first.text().to_ascii_lowercase();
                        if matches!(kw.as_str(), "delete" | "update") {
                            continue;
                        }
                        if kw == "ref" && part.get(1).is_some_and(|k| k.kind() == SyntaxKind::Colon)
                        {
                            kept.push(format!(
                                "ref: {} {}.{}",
                                update.kind.as_dbml_op(),
                                to.0,
                                to.1
                            ));
                        } else if let Some(last) = part.last() {
                            let s = usize::from(first.text_range().start());
                            let e = usize::from(last.text_range().end());
                            kept.push(text[s..e].to_string());
                        }
                    }
                    kept.extend(format_actions_parts(update.on_delete, update.on_update));
                    // Remove the earlier operator-only edit inside this bracket to avoid overlapping ranges
                    edits.retain(|(r, _)| r.start < b_start || r.end > b_end);
                    edits.push((b_start..b_end, format!("[{}]", kept.join(", "))));
                }
            }
        }
    }

    if edits.is_empty() {
        return text.to_string();
    }
    apply(text, edits)
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

    #[test]
    fn update_ref_changes_top_level_cardinality_and_keeps_comment() {
        let src = "Table users {\n  id int [pk]\n}\nTable posts {\n  id int [pk]\n  user_id int\n}\nRef: posts.user_id > users.id // keep comment\n";
        let updated = update_ref(
            src,
            ("posts", "user_id"),
            ("users", "id"),
            RefUpdate {
                kind: RefKind::OneToOne,
                on_delete: None,
                on_update: None,
            },
        );
        assert!(updated.contains("Ref: posts.user_id - users.id // keep comment"));
    }

    #[test]
    fn update_ref_sets_and_clears_actions_on_top_level_ref() {
        let src = "Table users {\n  id int [pk]\n}\nTable posts {\n  id int [pk]\n  user_id int\n}\nRef: posts.user_id > users.id\n";
        let with_actions = update_ref(
            src,
            ("posts", "user_id"),
            ("users", "id"),
            RefUpdate {
                kind: RefKind::ManyToOne,
                on_delete: Some(RefAction::Cascade),
                on_update: Some(RefAction::NoAction),
            },
        );
        assert!(
            with_actions
                .contains("Ref: posts.user_id > users.id [delete: cascade, update: no action]")
        );

        let cleared = update_ref(
            &with_actions,
            ("posts", "user_id"),
            ("users", "id"),
            RefUpdate {
                kind: RefKind::ManyToOne,
                on_delete: None,
                on_update: None,
            },
        );
        assert!(cleared.ends_with("Ref: posts.user_id > users.id\n"));
    }

    #[test]
    fn update_ref_updates_inline_ref_and_preserves_other_settings() {
        let src = "Table users {\n  id int [pk]\n}\nTable posts {\n  id int [pk]\n  user_id int [not null, ref: > users.id]\n}\n";
        let updated = update_ref(
            src,
            ("posts", "user_id"),
            ("users", "id"),
            RefUpdate {
                kind: RefKind::OneToMany,
                on_delete: Some(RefAction::SetNull),
                on_update: None,
            },
        );
        assert!(updated.contains("user_id int [not null, ref: < users.id, delete: set null]"));
    }

    #[test]
    fn qualified_schema_table_operations_preserve_namespaces() {
        let src = "\
Table auth.users {
  id int [pk]
}

Table billing.users {
  id int [pk]
}

Table billing.invoices {
  id int [pk]
}
";
        let linked = link_tables(src, "auth.users", "billing.invoices");
        assert!(linked.contains("id_users int"), "{linked}");
        assert!(
            linked.contains("Ref: billing.invoices.id_users > auth.users.id"),
            "{linked}"
        );

        let renamed_col =
            rename_column(&linked, "auth.users", "id", "user_pk").expect("rename_column");
        assert!(renamed_col.contains("user_pk int [pk]"), "{renamed_col}");
        assert!(
            renamed_col.contains("Ref: billing.invoices.id_users > auth.users.user_pk"),
            "{renamed_col}"
        );

        let grouped = crate::group_tables(&renamed_col, &["auth.users", "billing.users"], "ident");
        assert!(
            grouped.contains("TableGroup ident {\n  auth.users\n  billing.users\n}\n"),
            "{grouped}"
        );

        let colored = crate::set_table_color(&grouped, "auth.users", Some("#3b82f6"))
            .expect("set_table_color");
        assert!(
            colored.contains("Table auth.users [headercolor: #3b82f6] {"),
            "{colored}"
        );

        let renamed_tbl = rename_table(&colored, "auth.users", "auth.accounts");
        assert!(
            renamed_tbl.contains("Table auth.accounts [headercolor: #3b82f6] {"),
            "{renamed_tbl}"
        );
        assert!(
            renamed_tbl.contains("Table billing.users {"),
            "other schema table must remain untouched: {renamed_tbl}"
        );
        assert!(
            renamed_tbl.contains("id_accounts int"),
            "FK column should update to id_accounts: {renamed_tbl}"
        );
        assert!(
            renamed_tbl.contains("Ref: billing.invoices.id_accounts > auth.accounts.user_pk"),
            "{renamed_tbl}"
        );
        assert!(
            renamed_tbl.contains("TableGroup ident {\n  auth.accounts\n  billing.users\n}\n"),
            "{renamed_tbl}"
        );

        let ungrouped = crate::ungroup_table(&renamed_tbl, "auth.accounts");
        assert!(
            ungrouped.contains("TableGroup ident {\n  billing.users\n}\n"),
            "{ungrouped}"
        );

        let deleted_tbl = delete_table(&ungrouped, "auth.accounts");
        assert!(!deleted_tbl.contains("auth.accounts"), "{deleted_tbl}");
        assert!(
            !deleted_tbl.contains("id_accounts"),
            "FK column should be removed when parent table is deleted: {deleted_tbl}"
        );
        assert!(
            deleted_tbl.contains("Table billing.users {"),
            "{deleted_tbl}"
        );
    }
}
