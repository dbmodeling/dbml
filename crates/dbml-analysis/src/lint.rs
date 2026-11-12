//! Diagnostics (lint) with byte ranges for editor underlining.

use dbml_syntax::{SyntaxKind, SyntaxToken, descendant_tokens as sig_tokens, parse, unquote};
use std::collections::{HashMap, HashSet};
use std::ops::Range;

/// Severity level of a [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Syntax or semantic error that breaks schema validity.
    Error,
    /// Non-fatal issue such as a missing primary key or type mismatch.
    Warning,
    /// Informational hint.
    Info,
}

/// A lint diagnostic with a byte range in the DBML source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Byte range in the source text spanned by this diagnostic.
    pub range: Range<usize>,
    /// Diagnostic severity.
    pub severity: Severity,
    /// Human-readable diagnostic message.
    pub message: String,
}

fn diag(range: Range<usize>, severity: Severity, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        range,
        severity,
        message: message.into(),
    }
}

fn range(t: &SyntaxToken) -> Range<usize> {
    usize::from(t.text_range().start())..usize::from(t.text_range().end())
}

/// Top-level keywords not yet modeled in the AST, but valid in DBML.
const KNOWN_TOP: [&str; 5] = ["enum", "project", "tablegroup", "note", "records"];

/// Catalog of recognized SQL/DBML data types.
const KNOWN_TYPES: &[&str] = &[
    "int",
    "integer",
    "tinyint",
    "smallint",
    "mediumint",
    "bigint",
    "int2",
    "int4",
    "int8",
    "serial",
    "smallserial",
    "bigserial",
    "decimal",
    "numeric",
    "real",
    "float",
    "float4",
    "float8",
    "double",
    "money",
    "varchar",
    "char",
    "character",
    "nvarchar",
    "nchar",
    "text",
    "tinytext",
    "mediumtext",
    "longtext",
    "clob",
    "uuid",
    "boolean",
    "bool",
    "bit",
    "date",
    "datetime",
    "datetime2",
    "smalldatetime",
    "time",
    "timetz",
    "timestamp",
    "timestamptz",
    "interval",
    "year",
    "json",
    "jsonb",
    "xml",
    "bytea",
    "blob",
    "binary",
    "varbinary",
    "inet",
    "cidr",
    "macaddr",
    "point",
    "line",
    "polygon",
    "geometry",
    "geography",
    "tsvector",
    "tsquery",
    "array",
    "enum",
    "set",
];

fn levenshtein(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut costs: Vec<usize> = (0..=b_chars.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut last = i;
        costs[0] = i + 1;
        for (j, &cb) in b_chars.iter().enumerate() {
            let old = costs[j + 1];
            costs[j + 1] = if ca == cb {
                last
            } else {
                1 + last.min(old).min(costs[j])
            };
            last = old;
        }
    }
    costs[b_chars.len()]
}

fn suggest_type(input: &str) -> Option<&'static str> {
    let low = input.to_ascii_lowercase();
    KNOWN_TYPES
        .iter()
        .copied()
        .filter_map(|t| {
            let d = levenshtein(&low, t);
            (d <= 2 || (low.len() >= 4 && t.starts_with(&low))).then_some((d, t))
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, t)| t)
}

fn collect_enums(text: &str) -> HashSet<String> {
    let mut enums = HashSet::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("Enum ")
            .or_else(|| trimmed.strip_prefix("enum "))
        {
            let name = rest
                .split(|c: char| c.is_whitespace() || c == '{')
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                enums.insert(unquote(name).to_ascii_lowercase());
            }
        }
    }
    enums
}

fn check_column_type(
    cname: &str,
    ty_toks: &[SyntaxToken],
    enums: &HashSet<String>,
    out: &mut Vec<Diagnostic>,
) {
    let base_tok = &ty_toks[0];
    let base = unquote(base_tok.text()).to_ascii_lowercase();
    let is_known = KNOWN_TYPES.contains(&base.as_str()) || enums.contains(&base);
    if !is_known {
        let msg = match suggest_type(&base) {
            Some(sug) => format!(
                "Unknown type `{}` on column `{cname}` (did you mean `{sug}`?)",
                base_tok.text()
            ),
            None => format!("Unknown type `{}` on column `{cname}`", base_tok.text()),
        };
        out.push(diag(range(base_tok), Severity::Error, msg));
    }

    // Valida parênteses de precisão/tamanho e tokens extras antes de `[`
    let rest = &ty_toks[1..];
    if !rest.is_empty() {
        let lparen = rest.iter().position(|k| k.kind() == SyntaxKind::LParen);
        let rparen = rest.iter().position(|k| k.kind() == SyntaxKind::RParen);
        match (lparen, rparen) {
            (Some(0), Some(r)) => {
                let inner = &rest[1..r];
                if inner.is_empty() {
                    out.push(diag(
                        range(&rest[0]),
                        Severity::Error,
                        format!("Empty type parameters `()` in type `{}`", base_tok.text()),
                    ));
                } else if inner
                    .iter()
                    .any(|k| !matches!(k.kind(), SyntaxKind::Number | SyntaxKind::Comma))
                {
                    out.push(diag(
                        range(&inner[0]),
                        Severity::Error,
                        format!(
                            "Invalid size/precision for type `{}` (expected number)",
                            base_tok.text()
                        ),
                    ));
                }
                if let Some(extra) = rest.get(r + 1) {
                    out.push(diag(
                        range(extra),
                        Severity::Error,
                        format!(
                            "Unexpected token `{}` after column type on `{cname}`",
                            extra.text()
                        ),
                    ));
                }
            }
            (Some(l), None) => {
                out.push(diag(
                    range(&rest[l]),
                    Severity::Error,
                    format!("Missing `)` in type parameters for `{}`", base_tok.text()),
                ));
            }
            (None, Some(r)) => {
                out.push(diag(
                    range(&rest[r]),
                    Severity::Error,
                    format!("Unexpected `)` in column type on `{cname}`"),
                ));
            }
            _ => {
                out.push(diag(
                    range(&rest[0]),
                    Severity::Error,
                    format!(
                        "Unexpected token `{}` after type `{}` on column `{cname}`",
                        rest[0].text(),
                        base_tok.text()
                    ),
                ));
            }
        }
    }
}

fn check_column_settings(
    cname: &str,
    ct: &[SyntaxToken],
    open_idx: usize,
    out: &mut Vec<Diagnostic>,
) {
    let Some(close_rel) = ct[open_idx..]
        .iter()
        .position(|k| k.kind() == SyntaxKind::RBracket)
    else {
        out.push(diag(
            range(&ct[open_idx]),
            Severity::Error,
            "Missing `]` to close settings",
        ));
        return;
    };
    let close_idx = open_idx + close_rel;
    if let Some(extra) = ct.get(close_idx + 1) {
        out.push(diag(
            range(extra),
            Severity::Error,
            format!(
                "Unexpected token `{}` after `]` on column `{cname}`",
                extra.text()
            ),
        ));
    }
    let inner = &ct[open_idx + 1..close_idx];
    if inner.is_empty() {
        out.push(diag(
            range(&ct[open_idx]),
            Severity::Warning,
            format!("Empty settings block `[]` on column `{cname}`"),
        ));
        return;
    }

    let mut has_null = false;
    let mut has_not_null = false;
    let mut has_pk = false;

    for part in inner.split(|k| k.kind() == SyntaxKind::Comma) {
        if part.is_empty() {
            continue;
        }
        let first = &part[0];
        let kw = first.text().to_ascii_lowercase();
        match kw.as_str() {
            "pk" | "fk" | "unique" | "increment" => {
                if kw == "pk" {
                    has_pk = true;
                }
                if let Some(extra) = part.get(1) {
                    out.push(diag(
                        range(extra),
                        Severity::Error,
                        format!(
                            "Unexpected token `{}` after `{kw}` (use a comma to separate options)",
                            extra.text()
                        ),
                    ));
                }
            }
            "primary" => {
                has_pk = true;
                let ok = part
                    .get(1)
                    .is_some_and(|k| k.text().eq_ignore_ascii_case("key"))
                    && part.len() == 2;
                if !ok {
                    out.push(diag(
                        range(first),
                        Severity::Error,
                        "Incomplete setting: expected `primary key` or `pk`",
                    ));
                }
            }
            "null" => {
                has_null = true;
                if let Some(extra) = part.get(1) {
                    out.push(diag(
                        range(extra),
                        Severity::Error,
                        format!("Unexpected token `{}` after `null`", extra.text()),
                    ));
                }
            }
            "not" => {
                has_not_null = true;
                let ok = part
                    .get(1)
                    .is_some_and(|k| k.text().eq_ignore_ascii_case("null"))
                    && part.len() == 2;
                if !ok {
                    out.push(diag(
                        range(first),
                        Severity::Error,
                        "Incomplete setting: expected `not null`",
                    ));
                }
            }
            "default" | "note" | "ref" => {
                let has_colon = part.get(1).is_some_and(|k| k.kind() == SyntaxKind::Colon);
                let has_val = part.len() >= 3;
                if !has_colon || !has_val {
                    out.push(diag(
                        range(first),
                        Severity::Error,
                        format!("Setting `{kw}:` requires a value on column `{cname}`"),
                    ));
                }
            }
            _ => {
                out.push(diag(
                    range(first),
                    Severity::Error,
                    format!(
                        "Unknown setting `{}` on column `{cname}` (expected: pk, fk, not null, unique, increment, default, note, ref)",
                        first.text()
                    ),
                ));
            }
        }
    }

    if has_null && (has_not_null || has_pk) {
        out.push(diag(
            range(&ct[open_idx]),
            Severity::Error,
            format!("Conflicting constraints on column `{cname}`: cannot be both `null` and `not null`/`pk`"),
        ));
    }
}

/// Analyzes `text` and returns syntax and semantic [`Diagnostic`]s.
#[must_use]
pub fn lint(text: &str) -> Vec<Diagnostic> {
    let root = parse(text).syntax();
    let mut out = Vec::new();
    let mut tables: HashMap<String, HashSet<String>> = HashMap::new();
    let mut col_types: HashMap<(String, String), String> = HashMap::new();
    let enums = collect_enums(text);

    // 1ª passada: tabelas, colunas, duplicatas, estrutura
    for t in root.children().filter(|n| n.kind() == SyntaxKind::Table) {
        let toks = sig_tokens(&t);
        let head: Vec<_> = t
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|k| !k.kind().is_trivia())
            .collect();
        let Some(name_tok) = head
            .iter()
            .find(|k| k.kind() != SyntaxKind::Ident || !k.text().eq_ignore_ascii_case("table"))
            .filter(|k| matches!(k.kind(), SyntaxKind::Ident | SyntaxKind::Str))
        else {
            out.push(diag(
                range(&toks[0]),
                Severity::Error,
                "Table without a name",
            ));
            continue;
        };
        let name = unquote(name_tok.text()).to_string();
        if !head.iter().any(|k| k.kind() == SyntaxKind::LBrace) {
            out.push(diag(
                range(name_tok),
                Severity::Error,
                format!("Missing `{{` after `Table {name}`"),
            ));
        } else if !head.iter().any(|k| k.kind() == SyntaxKind::RBrace) {
            out.push(diag(
                range(name_tok),
                Severity::Error,
                format!("Table `{name}` was not closed with `}}`"),
            ));
        }
        if tables.contains_key(&name) {
            out.push(diag(
                range(name_tok),
                Severity::Error,
                format!("Duplicate table `{name}`"),
            ));
        }
        let mut cols = HashSet::new();
        let mut has_pk = false;
        for c in t.children().filter(|n| n.kind() == SyntaxKind::Column) {
            let ct = sig_tokens(&c);
            let Some(first) = ct.first() else { continue };
            let cname = unquote(first.text()).to_string();
            let bracket_pos = ct.iter().position(|k| k.kind() == SyntaxKind::LBracket);
            let ty_toks = match bracket_pos {
                Some(b) => &ct[1..b],
                None => &ct[1..],
            };
            if ty_toks.is_empty() {
                out.push(diag(
                    range(first),
                    Severity::Error,
                    format!("Column `{cname}` has no type"),
                ));
            } else {
                check_column_type(&cname, ty_toks, &enums, &mut out);
                col_types.insert(
                    (name.clone(), cname.clone()),
                    unquote(ty_toks[0].text()).to_ascii_lowercase(),
                );
            }
            if !cols.insert(cname.clone()) {
                out.push(diag(
                    range(first),
                    Severity::Error,
                    format!("Duplicate column `{cname}` in `{name}`"),
                ));
            }
            if ct.iter().any(|k| {
                k.text().eq_ignore_ascii_case("pk") || k.text().eq_ignore_ascii_case("primary")
            }) {
                has_pk = true;
            }
            if let Some(open) = bracket_pos {
                check_column_settings(&cname, &ct, open, &mut out);
            } else if let Some(unopened_close) =
                ct.iter().position(|k| k.kind() == SyntaxKind::RBracket)
            {
                out.push(diag(
                    range(&ct[unopened_close]),
                    Severity::Error,
                    "Closing bracket `]` without matching `[`",
                ));
            }
        }
        if !has_pk && !cols.is_empty() {
            out.push(diag(
                range(name_tok),
                Severity::Info,
                format!("`{name}` has no primary key"),
            ));
        }
        tables.insert(name, cols);
    }

    // 2ª passada: referências (topo e inline)
    let check = |ts: &[SyntaxToken], out: &mut Vec<Diagnostic>| {
        // procura padrões `tabela . coluna`
        for w in ts.windows(3) {
            if w[1].text() == "." && matches!(w[0].kind(), SyntaxKind::Ident | SyntaxKind::Str) {
                let (tn, cn) = (unquote(w[0].text()), unquote(w[2].text()));
                match tables.get(tn) {
                    None => out.push(diag(
                        range(&w[0]),
                        Severity::Error,
                        format!("Table `{tn}` does not exist"),
                    )),
                    Some(cols) if !cols.contains(cn) => out.push(diag(
                        range(&w[2]),
                        Severity::Error,
                        format!("Column `{cn}` does not exist in `{tn}`"),
                    )),
                    _ => {}
                }
            }
        }
    };
    for r in root.children().filter(|n| n.kind() == SyntaxKind::Ref) {
        let ts = sig_tokens(&r);
        let Some(colon) = ts.iter().position(|k| k.text() == ":") else {
            out.push(diag(
                range(&ts[0]),
                Severity::Error,
                "Expected `Ref: table.column > table.column`",
            ));
            continue;
        };
        check(&ts, &mut out);
        let body = &ts[colon + 1..];
        let has_dot1 = body.get(1).is_some_and(|k| k.text() == ".");
        let mut op_idx = 3;
        while op_idx < body.len() && matches!(body[op_idx].text(), "<" | ">" | "-") {
            op_idx += 1;
        }
        let has_op = op_idx > 3;
        let has_dot2 =
            body.get(op_idx + 1).is_some_and(|k| k.text() == ".") && body.get(op_idx + 2).is_some();
        if !has_dot1 || !has_op || !has_dot2 {
            out.push(diag(
                range(&ts[0]),
                Severity::Error,
                "Incomplete relation: expected `Ref: table.column > table.column`",
            ));
        } else {
            let (ft, fc) = (unquote(body[0].text()), unquote(body[2].text()));
            let (tt, tc) = (
                unquote(body[op_idx].text()),
                unquote(body[op_idx + 2].text()),
            );
            if ft == tt && fc == tc {
                out.push(diag(
                    range(&body[0]),
                    Severity::Error,
                    format!("A column cannot reference itself (`{ft}.{fc}`)"),
                ));
            } else if let (Some(t1), Some(t2)) = (
                col_types.get(&(ft.to_string(), fc.to_string())),
                col_types.get(&(tt.to_string(), tc.to_string())),
            ) && t1 != t2
            {
                out.push(diag(
                    range(&body[2]),
                    Severity::Warning,
                    format!(
                        "Incompatible types in relation: `{ft}.{fc}` is `{t1}`, but `{tt}.{tc}` is `{t2}`"
                    ),
                ));
            }
        }
    }
    for c in root
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::Column)
    {
        let ts = sig_tokens(&c);
        if let Some(i) = ts.iter().position(|k| k.text().eq_ignore_ascii_case("ref")) {
            check(&ts[i..], &mut out);
        }
    }

    // Validação de TableGroup (tabelas inexistentes ou cabeçalho malformado)
    for g in root
        .children()
        .filter(|n| n.kind() == SyntaxKind::TableGroup)
    {
        let toks = sig_tokens(&g);
        let head: Vec<_> = g
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|k| !k.kind().is_trivia())
            .collect();
        let Some(gname_tok) = head
            .iter()
            .find(|k| k.kind() != SyntaxKind::Ident || !k.text().eq_ignore_ascii_case("tablegroup"))
            .filter(|k| matches!(k.kind(), SyntaxKind::Ident | SyntaxKind::Str))
        else {
            out.push(diag(
                range(&toks[0]),
                Severity::Error,
                "TableGroup without a name",
            ));
            continue;
        };
        let gname = unquote(gname_tok.text());
        if !head.iter().any(|k| k.kind() == SyntaxKind::RBrace) {
            out.push(diag(
                range(gname_tok),
                Severity::Error,
                format!("TableGroup `{gname}` was not closed with `}}`"),
            ));
        }
        for item in g.children().filter(|n| n.kind() == SyntaxKind::Column) {
            let it = sig_tokens(&item);
            if let Some(first) = it.first() {
                let tname = unquote(first.text());
                if !tables.contains_key(tname) {
                    out.push(diag(
                        range(first),
                        Severity::Error,
                        format!("Table `{tname}` listed in group `{gname}` does not exist"),
                    ));
                }
            }
        }
    }

    // 3ª passada: texto solto no topo que não é construção conhecida
    let mut line_start = true;
    for e in root.children_with_tokens() {
        match e {
            rowan::NodeOrToken::Token(t) => {
                if t.kind() == SyntaxKind::Newline {
                    line_start = true;
                } else if !t.kind().is_trivia() {
                    if line_start
                        && !(t.kind() == SyntaxKind::Ident
                            && KNOWN_TOP.contains(&t.text().to_ascii_lowercase().as_str()))
                    {
                        out.push(diag(
                            range(&t),
                            Severity::Warning,
                            format!("Unrecognized construct: `{}`", t.text()),
                        ));
                    }
                    line_start = false;
                }
            }
            rowan::NodeOrToken::Node(_) => line_start = false,
        }
    }
    out.sort_by_key(|d| d.range.start);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msgs(src: &str) -> Vec<String> {
        lint(src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn clean_schema_has_only_infos() {
        let src = "Table a {\n  id int [pk]\n}\nTable b {\n  id int [pk]\n  a_id int [ref: > a.id]\n}\nRef: b.a_id > a.id\n";
        assert!(lint(src).is_empty(), "{:?}", lint(src));
    }

    #[test]
    fn unknown_ref_targets() {
        let m = msgs("Table a {\n  id int [pk]\n}\nRef: a.id > x.id\nRef: a.nope > a.id\n");
        assert!(m.contains(&"Table `x` does not exist".to_string()), "{m:?}");
        assert!(
            m.contains(&"Column `nope` does not exist in `a`".to_string()),
            "{m:?}"
        );
    }

    #[test]
    fn structural_errors() {
        let m = msgs("Table a {\n  id\n  id int [pk\n");
        assert!(m.iter().any(|s| s.contains("was not closed")), "{m:?}");
        assert!(m.iter().any(|s| s.contains("has no type")), "{m:?}");
        assert!(m.iter().any(|s| s.contains("Duplicate")), "{m:?}");
        assert!(m.iter().any(|s| s.contains("Missing `]`")), "{m:?}");
    }

    #[test]
    fn stray_text_and_missing_pk() {
        let m = msgs("blah\nEnum status {\n}\nTable a {\n  x int\n}\n");
        assert!(
            m.iter()
                .any(|s| s.contains("Unrecognized construct: `blah`")),
            "{m:?}"
        );
        assert!(!m.iter().any(|s| s.contains("`Enum`")), "{m:?}");
        assert!(m.iter().any(|s| s.contains("has no primary key")), "{m:?}");
    }

    #[test]
    fn ranges_point_at_tokens() {
        let src = "Ref: a.id > x.id\n";
        let d = lint(src);
        let r = d.iter().find(|d| d.message.contains("`a`")).unwrap();
        assert_eq!(&src[r.range.clone()], "a");
    }

    #[test]
    fn detects_unknown_column_types_and_suggests_fix() {
        let src = "Table users {\n  id int [pk]\n  created_at timestam}\n";
        let m = msgs(src);
        assert!(
            m.iter()
                .any(|s| s.contains("Unknown type `timestam`") && s.contains("`timestamp`")),
            "Should report error on timestam and suggest timestamp: {m:?}"
        );
    }

    #[test]
    fn detects_malformed_type_params_and_settings() {
        let src = "Table users {\n  id int [pk, null]\n  email varchar() [uniq]\n  bio text [default:]\n}\n";
        let m = msgs(src);
        assert!(
            m.iter().any(|s| s.contains("Conflicting constraints")),
            "{m:?}"
        );
        assert!(
            m.iter().any(|s| s.contains("Empty type parameters")),
            "{m:?}"
        );
        assert!(
            m.iter().any(|s| s.contains("Unknown setting `uniq`")),
            "{m:?}"
        );
        assert!(m.iter().any(|s| s.contains("requires a value")), "{m:?}");
    }

    #[test]
    fn detects_ref_type_mismatch_and_self_ref() {
        let src = "Table a {\n  id int [pk]\n}\nTable b {\n  id int [pk]\n  a_id text\n}\nRef: b.a_id > a.id\nRef: a.id > a.id\n";
        let m = msgs(src);
        assert!(m.iter().any(|s| s.contains("Incompatible types")), "{m:?}");
        assert!(m.iter().any(|s| s.contains("reference itself")), "{m:?}");
    }
}
