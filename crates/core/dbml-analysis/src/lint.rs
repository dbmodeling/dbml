//! Diagnostics (lint) with byte ranges for editor underlining.

use dbml_syntax::{SyntaxKind, SyntaxToken, descendant_tokens as sig_tokens, parse, unquote};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
    for e in crate::analyze(text).enums {
        let bare = e.name.to_ascii_lowercase();
        if let Some(s) = &e.schema {
            enums.insert(format!("{}.{bare}", s.to_ascii_lowercase()));
        }
        enums.insert(bare);
    }
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
    let (base, rest) = if ty_toks.len() >= 3
        && ty_toks[1].text() == "."
        && matches!(ty_toks[2].kind(), SyntaxKind::Ident | SyntaxKind::Str)
    {
        (
            format!(
                "{}.{}",
                unquote(ty_toks[0].text()).to_ascii_lowercase(),
                unquote(ty_toks[2].text()).to_ascii_lowercase()
            ),
            &ty_toks[3..],
        )
    } else {
        (unquote(base_tok.text()).to_ascii_lowercase(), &ty_toks[1..])
    };
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
                } else {
                    check_type_param_values(cname, base_tok, &base, inner, out);
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

fn check_type_param_values(
    cname: &str,
    base_tok: &SyntaxToken,
    base: &str,
    inner: &[SyntaxToken],
    out: &mut Vec<Diagnostic>,
) {
    let mut nums: Vec<(u64, &SyntaxToken)> = Vec::new();
    for part in inner.split(|k| k.kind() == SyntaxKind::Comma) {
        if part.len() != 1 || part[0].kind() != SyntaxKind::Number {
            let err_tok = part.first().unwrap_or(base_tok);
            out.push(diag(
                range(err_tok),
                Severity::Error,
                format!(
                    "Malformed type parameters in `{}` on column `{cname}`",
                    base_tok.text()
                ),
            ));
            return;
        }
        let Ok(n) = part[0].text().parse::<u64>() else {
            out.push(diag(
                range(&part[0]),
                Severity::Error,
                format!(
                    "Invalid integer parameter `{}` for type `{}` on column `{cname}`",
                    part[0].text(),
                    base_tok.text()
                ),
            ));
            return;
        };
        nums.push((n, &part[0]));
    }

    match base {
        "decimal" | "numeric" => {
            if nums.len() > 2 {
                out.push(diag(
                    range(nums[2].1),
                    Severity::Error,
                    format!(
                        "Type `{}` accepts at most 2 parameters (precision, scale)",
                        base_tok.text()
                    ),
                ));
                return;
            }
            let (precision, p_tok) = nums[0];
            if precision == 0 {
                out.push(diag(
                    range(p_tok),
                    Severity::Warning,
                    format!(
                        "Precision for `{}` on column `{cname}` should be greater than 0",
                        base_tok.text()
                    ),
                ));
            } else if precision > 65 {
                let hint = if nums.len() == 1 && (100..=659).contains(&precision) {
                    " — did you forget a comma (e.g., `decimal(10, 2)`)?"
                } else {
                    ""
                };
                out.push(diag(
                    range(p_tok),
                    Severity::Warning,
                    format!(
                        "Precision {precision} for `{}` on column `{cname}` exceeds SQL maximum (65){hint}",
                        base_tok.text()
                    ),
                ));
            }
            if let Some(&(scale, s_tok)) = nums.get(1)
                && scale > precision
            {
                out.push(diag(
                    range(s_tok),
                    Severity::Error,
                    format!(
                        "Scale ({scale}) cannot exceed precision ({precision}) in `{}` on column `{cname}`",
                        base_tok.text()
                    ),
                ));
            }
        }
        "varchar" | "char" | "nvarchar" | "nchar" => {
            if nums.len() > 1 {
                out.push(diag(
                    range(nums[1].1),
                    Severity::Error,
                    format!("Type `{}` accepts only 1 length parameter", base_tok.text()),
                ));
                return;
            }
            let (len, l_tok) = nums[0];
            if len == 0 {
                out.push(diag(
                    range(l_tok),
                    Severity::Warning,
                    format!(
                        "Length for `{}` on column `{cname}` should be greater than 0",
                        base_tok.text()
                    ),
                ));
            }
        }
        _ => {}
    }
}

#[derive(Default, Clone, Copy)]
struct ColumnFlags {
    is_unique_capable: bool,
    has_not_null: bool,
    has_default: bool,
}

/// Whether a raw SQL expression has a `;` outside single-quoted string literals.
fn has_statement_terminator(expr: &str) -> bool {
    let mut in_string = false;
    for ch in expr.chars() {
        match ch {
            '\'' => in_string = !in_string,
            ';' if !in_string => return true,
            _ => {}
        }
    }
    false
}

fn check_column_settings(
    cname: &str,
    ct: &[SyntaxToken],
    open_idx: usize,
    out: &mut Vec<Diagnostic>,
) -> ColumnFlags {
    let mut flags = ColumnFlags::default();
    let Some(close_rel) = ct[open_idx..]
        .iter()
        .position(|k| k.kind() == SyntaxKind::RBracket)
    else {
        out.push(diag(
            range(&ct[open_idx]),
            Severity::Error,
            "Missing `]` to close settings",
        ));
        return flags;
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
        return flags;
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
                    flags.is_unique_capable = true;
                } else if kw == "unique" {
                    flags.is_unique_capable = true;
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
                flags.is_unique_capable = true;
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
                flags.has_not_null = true;
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
            "default" | "note" | "check" | "ref" | "delete" | "update" => {
                if kw == "default" {
                    flags.has_default = true;
                }
                let has_colon = part.get(1).is_some_and(|k| k.kind() == SyntaxKind::Colon);
                let has_val = part.len() >= 3;
                if !has_colon || !has_val {
                    out.push(diag(
                        range(first),
                        Severity::Error,
                        format!("Setting `{kw}:` requires a value on column `{cname}`"),
                    ));
                } else if matches!(kw.as_str(), "delete" | "update") {
                    let val: String = part[2..]
                        .iter()
                        .map(rowan::SyntaxToken::text)
                        .collect::<Vec<_>>()
                        .join(" ");
                    if schema::RefAction::from_dbml_str(&val).is_none() {
                        out.push(diag(
                            range(&part[2]),
                            Severity::Error,
                            format!(
                                "Invalid referential action `{val}` for `{kw}:` (expected: no action, restrict, cascade, set null, set default)"
                            ),
                        ));
                    }
                } else if matches!(kw.as_str(), "default" | "check") {
                    for tok in &part[2..] {
                        let raw = tok.text();
                        let expr = raw
                            .strip_prefix('`')
                            .map_or(raw, |r| r.strip_suffix('`').unwrap_or(r));
                        let raw_expr = tok.kind() != SyntaxKind::Str || raw.starts_with('`');
                        if raw_expr && has_statement_terminator(expr) {
                            out.push(diag(
                                range(tok),
                                Severity::Warning,
                                format!(
                                    "Raw `{kw}:` expression on column `{cname}` contains a statement terminator `;`; it is exported verbatim to SQL"
                                ),
                            ));
                        }
                    }
                }
            }
            _ => {
                out.push(diag(
                    range(first),
                    Severity::Error,
                    format!(
                        "Unknown setting `{}` on column `{cname}` (expected: pk, fk, not null, unique, increment, default, check, note, ref)",
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
    flags
}

/// Analyzes `text` and returns syntax and semantic [`Diagnostic`]s.
#[must_use]
pub fn lint(text: &str) -> Vec<Diagnostic> {
    let root = parse(text).syntax();
    let mut out = Vec::new();
    let mut tables: HashMap<String, HashSet<String>> = HashMap::new();
    let mut aliases: HashMap<String, String> = HashMap::new();
    let mut col_types: HashMap<(String, String), String> = HashMap::new();
    let mut col_flags: HashMap<(String, String), ColumnFlags> = HashMap::new();
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
        let before_brace: Vec<_> = head
            .iter()
            .take_while(|k| k.kind() != SyntaxKind::LBrace)
            .cloned()
            .collect();
        let (t_schema, t_name, t_alias) = crate::parse_qualified_header(&before_brace, "table");
        let name = match (&t_schema, &t_name) {
            (Some(s), Some(n)) => format!("{s}.{n}"),
            (None, Some(n)) => n.clone(),
            _ => unquote(name_tok.text()).to_string(),
        };
        if let Some(alias) = t_alias {
            aliases.insert(alias, name.clone());
        }
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
            let flags = if let Some(open) = bracket_pos {
                check_column_settings(&cname, &ct, open, &mut out)
            } else {
                if let Some(unopened_close) =
                    ct.iter().position(|k| k.kind() == SyntaxKind::RBracket)
                {
                    out.push(diag(
                        range(&ct[unopened_close]),
                        Severity::Error,
                        "Closing bracket `]` without matching `[`",
                    ));
                }
                ColumnFlags::default()
            };
            col_flags.insert((name.clone(), cname), flags);
        }
        for b in t
            .children()
            .filter(|n| matches!(n.kind(), SyntaxKind::Block | SyntaxKind::Indexes))
        {
            collect_index_uniques(&name, &b, &mut col_flags, &mut has_pk);
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
    let mut fk_graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut fk_anchors: BTreeMap<(String, String), Range<usize>> = BTreeMap::new();
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
        let body = &ts[colon + 1..];
        let body_strs: Vec<&str> = body.iter().map(rowan::SyntaxToken::text).collect();
        let parsed_from = crate::endpoint(&body_strs);
        let op_start = parsed_from.as_ref().map_or(0, |(_, len)| *len);
        let mut op_idx = op_start;
        while op_idx < body.len() && matches!(body[op_idx].text(), "<" | ">" | "-") {
            op_idx += 1;
        }
        let has_op = parsed_from.is_some() && op_idx > op_start;
        let parsed_to = if has_op {
            crate::endpoint(&body_strs[op_idx..])
        } else {
            None
        };
        let (Some((from_ep, _)), Some((to_ep, _))) = (parsed_from, parsed_to) else {
            out.push(diag(
                range(&ts[0]),
                Severity::Error,
                "Incomplete relation: expected `Ref: table.column > table.column`",
            ));
            continue;
        };

        let from_key = check_endpoint(&from_ep, &body[0], &tables, &aliases, &mut out);
        let to_anchor = body.get(op_idx).unwrap_or(&ts[0]);
        let to_key = check_endpoint(&to_ep, to_anchor, &tables, &aliases, &mut out);

        let (from_cols, to_cols) = (from_ep.cols(), to_ep.cols());
        if from_cols.len() != to_cols.len() {
            out.push(diag(
                range(&ts[0]),
                Severity::Error,
                format!(
                    "Composite foreign key column count mismatch ({} vs {})",
                    from_cols.len(),
                    to_cols.len()
                ),
            ));
            continue;
        }
        let (Some(ft), Some(tt)) = (from_key, to_key) else {
            continue;
        };
        if ft == tt && from_cols == to_cols {
            out.push(diag(
                range(&body[0]),
                Severity::Error,
                format!(
                    "A column cannot reference itself (`{ft}.{}`)",
                    from_ep.primary_column()
                ),
            ));
        } else {
            for (&fc, &tc) in from_cols.iter().zip(to_cols.iter()) {
                if let (Some(t1), Some(t2)) = (
                    col_types.get(&(ft.to_string(), fc.to_string())),
                    col_types.get(&(tt.to_string(), tc.to_string())),
                ) && t1 != t2
                {
                    out.push(diag(
                        range(&body[0]),
                        Severity::Warning,
                        format!(
                            "Incompatible types in relation: `{ft}.{fc}` is `{t1}`, but `{tt}.{tc}` is `{t2}`"
                        ),
                    ));
                }
            }
            if from_cols.len() == 1
                && let Some(op) = RefOp::from_tokens(&body[op_start..op_idx])
                && let Some(target_tok) = body.get(op_idx + 2)
            {
                check_fk_semantics(
                    &FkRelation {
                        ft,
                        fc: from_cols[0],
                        tt,
                        tc: to_cols[0],
                        op,
                        anchor_tok: &body[0],
                        target_tok,
                    },
                    &col_flags,
                    &mut fk_graph,
                    &mut fk_anchors,
                    &mut out,
                );
            }
        }
    }
    for t in root.children().filter(|n| n.kind() == SyntaxKind::Table) {
        let t_head: Vec<_> = t
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|k| !k.kind().is_trivia())
            .take_while(|k| k.kind() != SyntaxKind::LBrace)
            .collect();
        let (t_schema, t_name, _) = crate::parse_qualified_header(&t_head, "table");
        let Some(ft_owned) = (match (&t_schema, &t_name) {
            (Some(s), Some(n)) => Some(format!("{s}.{n}")),
            (None, Some(n)) => Some(n.clone()),
            _ => None,
        }) else {
            continue;
        };
        let ft = ft_owned.as_str();
        for c in t.children().filter(|n| n.kind() == SyntaxKind::Column) {
            let ts = sig_tokens(&c);
            let Some(col_tok) = ts.first() else { continue };
            let fc = unquote(col_tok.text());
            if let Some(i) = ts.iter().position(|k| k.text().eq_ignore_ascii_case("ref")) {
                let tail: Vec<_> = ts[i + 1..]
                    .iter()
                    .take_while(|k| {
                        k.kind() != SyntaxKind::Comma && k.kind() != SyntaxKind::RBracket
                    })
                    .collect();
                if tail.first().is_some_and(|k| k.kind() == SyntaxKind::Colon) {
                    let body = &tail[1..];
                    let mut op_end = 0;
                    while op_end < body.len() && matches!(body[op_end].text(), "<" | ">" | "-") {
                        op_end += 1;
                    }
                    let body_strs: Vec<&str> = body[op_end..].iter().map(|k| k.text()).collect();
                    if op_end > 0
                        && let Some((to_ep, _)) = crate::endpoint(&body_strs)
                    {
                        let anchor = body.get(op_end).copied().unwrap_or(&ts[i]);
                        let Some(tt) = check_endpoint(&to_ep, anchor, &tables, &aliases, &mut out)
                        else {
                            continue;
                        };
                        let tc = to_ep.primary_column();
                        if ft == tt && fc == tc {
                            out.push(diag(
                                range(&ts[i]),
                                Severity::Error,
                                format!("A column cannot reference itself (`{ft}.{fc}`)"),
                            ));
                        } else if let Some(op) = RefOp::from_token_refs(&body[..op_end]) {
                            let target_tok = body.last().copied().unwrap_or(anchor);
                            check_fk_semantics(
                                &FkRelation {
                                    ft,
                                    fc,
                                    tt,
                                    tc,
                                    op,
                                    anchor_tok: &ts[i],
                                    target_tok,
                                },
                                &col_flags,
                                &mut fk_graph,
                                &mut fk_anchors,
                                &mut out,
                            );
                        }
                    }
                }
            }
        }
    }
    detect_fk_cycles(&fk_graph, &fk_anchors, &mut out);

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
            if let (Some(first), Some(tname)) = (it.first(), crate::qualified_member(&it)) {
                let found = match tname.split_once('.') {
                    Some((s, n)) => resolve_table_key(&tables, &aliases, Some(s), n).is_some(),
                    None => resolve_table_key(&tables, &aliases, None, &tname).is_some(),
                };
                if !found {
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

fn resolve_table_key<'a>(
    tables: &'a HashMap<String, HashSet<String>>,
    aliases: &'a HashMap<String, String>,
    schema: Option<&str>,
    table: &str,
) -> Option<&'a str> {
    if let Some(s) = schema {
        let qn = format!("{s}.{table}");
        if let Some((k, _)) = tables.get_key_value(&qn) {
            return Some(k.as_str());
        }
        if s == "public"
            && let Some((k, _)) = tables.get_key_value(table)
        {
            return Some(k.as_str());
        }
        return None;
    }
    if let Some(real) = aliases.get(table)
        && let Some((k, _)) = tables.get_key_value(real)
    {
        return Some(k.as_str());
    }
    if let Some((k, _)) = tables.get_key_value(table) {
        return Some(k.as_str());
    }
    let public_qn = format!("public.{table}");
    if let Some((k, _)) = tables.get_key_value(&public_qn) {
        return Some(k.as_str());
    }
    None
}

fn check_endpoint<'a>(
    ep: &schema::Endpoint,
    anchor: &SyntaxToken,
    tables: &'a HashMap<String, HashSet<String>>,
    aliases: &'a HashMap<String, String>,
    out: &mut Vec<Diagnostic>,
) -> Option<&'a str> {
    let key = resolve_table_key(tables, aliases, ep.schema.as_deref(), &ep.table);
    let Some(tkey) = key else {
        out.push(diag(
            range(anchor),
            Severity::Error,
            format!("Table `{}` does not exist", ep.qualified_table()),
        ));
        return None;
    };
    if let Some(cols) = tables.get(tkey) {
        for col in ep.cols() {
            if !cols.contains(col) {
                out.push(diag(
                    range(anchor),
                    Severity::Error,
                    format!("Column `{col}` does not exist in `{tkey}`"),
                ));
            }
        }
    }
    Some(tkey)
}

fn collect_index_uniques(
    table_name: &str,
    block: &dbml_syntax::SyntaxNode,
    col_flags: &mut HashMap<(String, String), ColumnFlags>,
    has_pk: &mut bool,
) {
    let head_toks = sig_tokens(block);
    let Some(first) = head_toks.first() else {
        return;
    };
    if !first.text().eq_ignore_ascii_case("indexes") {
        return;
    }
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
        let Some(open) = ts.iter().position(|k| k.kind() == SyntaxKind::LBracket) else {
            continue;
        };
        let Some(close_rel) = ts[open..]
            .iter()
            .position(|k| k.kind() == SyntaxKind::RBracket)
        else {
            continue;
        };
        let settings = &ts[open + 1..open + close_rel];
        let is_pk = settings.iter().any(|k| k.text().eq_ignore_ascii_case("pk"));
        let is_unique = settings
            .iter()
            .any(|k| k.text().eq_ignore_ascii_case("unique"));
        if !is_pk && !is_unique {
            continue;
        }
        let target = &ts[..open];
        let single_col = if target.len() == 1
            && matches!(target[0].kind(), SyntaxKind::Ident | SyntaxKind::Str)
        {
            Some(unquote(target[0].text()))
        } else if target.len() == 3
            && target[0].kind() == SyntaxKind::LParen
            && target[2].kind() == SyntaxKind::RParen
            && matches!(target[1].kind(), SyntaxKind::Ident | SyntaxKind::Str)
        {
            Some(unquote(target[1].text()))
        } else {
            None
        };
        if let Some(cname) = single_col
            && let Some(flags) = col_flags.get_mut(&(table_name.to_string(), cname.to_string()))
        {
            flags.is_unique_capable = true;
            if is_pk {
                *has_pk = true;
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RefOp {
    ManyToOne,
    OneToMany,
    OneToOne,
    ManyToMany,
}

impl RefOp {
    fn from_tokens(toks: &[SyntaxToken]) -> Option<Self> {
        match toks {
            [t] if t.text() == ">" => Some(Self::ManyToOne),
            [t] if t.text() == "<" => Some(Self::OneToMany),
            [t] if t.text() == "-" => Some(Self::OneToOne),
            [a, b] if a.text() == "<" && b.text() == ">" => Some(Self::ManyToMany),
            _ => None,
        }
    }

    fn from_token_refs(toks: &[&SyntaxToken]) -> Option<Self> {
        match toks {
            [t] if t.text() == ">" => Some(Self::ManyToOne),
            [t] if t.text() == "<" => Some(Self::OneToMany),
            [t] if t.text() == "-" => Some(Self::OneToOne),
            [a, b] if a.text() == "<" && b.text() == ">" => Some(Self::ManyToMany),
            _ => None,
        }
    }
}

struct FkRelation<'a> {
    ft: &'a str,
    fc: &'a str,
    tt: &'a str,
    tc: &'a str,
    op: RefOp,
    anchor_tok: &'a SyntaxToken,
    target_tok: &'a SyntaxToken,
}

fn check_fk_semantics(
    rel: &FkRelation<'_>,
    col_flags: &HashMap<(String, String), ColumnFlags>,
    fk_graph: &mut BTreeMap<String, BTreeSet<String>>,
    fk_anchors: &mut BTreeMap<(String, String), Range<usize>>,
    out: &mut Vec<Diagnostic>,
) {
    if rel.op == RefOp::ManyToMany {
        return;
    }
    let (child_t, child_c, parent_t, parent_c) = if rel.op == RefOp::OneToMany {
        (rel.tt, rel.tc, rel.ft, rel.fc)
    } else {
        (rel.ft, rel.fc, rel.tt, rel.tc)
    };

    let child_key = (child_t.to_string(), child_c.to_string());
    let parent_key = (parent_t.to_string(), parent_c.to_string());

    if let Some(parent_flags) = col_flags.get(&parent_key)
        && !parent_flags.is_unique_capable
    {
        out.push(diag(
            range(rel.target_tok),
            Severity::Warning,
            format!("Referenced column `{parent_t}.{parent_c}` is neither `pk` nor `unique`"),
        ));
    }

    if child_t == parent_t {
        if let Some(child_flags) = col_flags.get(&child_key)
            && child_flags.has_not_null
            && !child_flags.has_default
        {
            out.push(diag(
                range(rel.anchor_tok),
                Severity::Warning,
                format!(
                    "Self-referencing FK `{child_t}.{child_c}` is `not null` without a `default`, which prevents inserting the initial row"
                ),
            ));
        }
        return;
    }

    if col_flags.contains_key(&child_key) && col_flags.contains_key(&parent_key) {
        fk_graph
            .entry(child_key.0.clone())
            .or_default()
            .insert(parent_key.0.clone());
        fk_anchors
            .entry((child_key.0, parent_key.0))
            .or_insert_with(|| range(rel.anchor_tok));
    }
}

fn detect_fk_cycles(
    fk_graph: &BTreeMap<String, BTreeSet<String>>,
    fk_anchors: &BTreeMap<(String, String), Range<usize>>,
    out: &mut Vec<Diagnostic>,
) {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Visiting,
        Visited,
    }

    fn dfs<'a>(
        node: &'a str,
        fk_graph: &'a BTreeMap<String, BTreeSet<String>>,
        fk_anchors: &BTreeMap<(String, String), Range<usize>>,
        state: &mut HashMap<&'a str, State>,
        stack: &mut Vec<&'a str>,
        seen_cycles: &mut HashSet<Vec<&'a str>>,
        out: &mut Vec<Diagnostic>,
    ) {
        state.insert(node, State::Visiting);
        stack.push(node);
        if let Some(neighbors) = fk_graph.get(node) {
            for next in neighbors {
                match state.get(next.as_str()) {
                    None => dfs(next, fk_graph, fk_anchors, state, stack, seen_cycles, out),
                    Some(State::Visiting) => {
                        if let Some(pos) = stack.iter().position(|&n| n == next) {
                            let cycle_nodes = &stack[pos..];
                            let min_pos = cycle_nodes
                                .iter()
                                .enumerate()
                                .min_by(|(_, a), (_, b)| a.cmp(b))
                                .map_or(0, |(i, _)| i);
                            let canonical: Vec<&'a str> = cycle_nodes[min_pos..]
                                .iter()
                                .chain(cycle_nodes[..min_pos].iter())
                                .copied()
                                .collect();
                            if seen_cycles.insert(canonical) {
                                let mut path: Vec<&str> = cycle_nodes.to_vec();
                                path.push(next.as_str());
                                if let Some(anchor) =
                                    fk_anchors.get(&(node.to_string(), next.clone()))
                                {
                                    out.push(diag(
                                        anchor.clone(),
                                        Severity::Warning,
                                        format!(
                                            "Circular foreign key dependency detected: {}",
                                            path.join(" -> ")
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                    Some(State::Visited) => {}
                }
            }
        }
        stack.pop();
        state.insert(node, State::Visited);
    }

    let mut state = HashMap::new();
    let mut stack = Vec::new();
    let mut seen_cycles = HashSet::new();
    for node in fk_graph.keys() {
        if !state.contains_key(node.as_str()) {
            dfs(
                node,
                fk_graph,
                fk_anchors,
                &mut state,
                &mut stack,
                &mut seen_cycles,
                out,
            );
        }
    }
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
    fn raw_default_and_check_with_statement_terminator_warn() {
        let src = "Table t {\n  a int [default: `1; DROP TABLE t`]\n  b int [check: `b > 0; --`]\n  c text [default: `'a;b'`]\n  d int [default: `now()`]\n}\n";
        let m = msgs(src);
        let hits = m
            .iter()
            .filter(|s| s.contains("statement terminator"))
            .count();
        assert_eq!(hits, 2, "{m:?}");
    }

    #[test]
    fn detects_ref_type_mismatch_and_self_ref() {
        let src = "Table a {\n  id int [pk]\n}\nTable b {\n  id int [pk]\n  a_id text\n}\nRef: b.a_id > a.id\nRef: a.id > a.id\n";
        let m = msgs(src);
        assert!(m.iter().any(|s| s.contains("Incompatible types")), "{m:?}");
        assert!(m.iter().any(|s| s.contains("reference itself")), "{m:?}");
    }

    #[test]
    fn valid_type_parameters_produce_no_diagnostics() {
        let valid = "Table t {\n  id int [pk]\n  price decimal(10, 2)\n  code varchar(32)\n}\n";
        assert!(lint(valid).is_empty(), "{:?}", lint(valid));
    }

    #[test]
    fn decimal_precision_over_65_warns_with_comma_hint() {
        let m = msgs("Table t {\n  id int [pk]\n  a decimal(102)\n}\n");
        assert!(
            m.iter()
                .any(|s| s.contains("exceeds SQL maximum (65)") && s.contains("comma")),
            "{m:?}"
        );
    }

    #[test]
    fn decimal_scale_exceeding_precision_errors() {
        let m = msgs("Table t {\n  id int [pk]\n  b decimal(5, 10)\n}\n");
        assert!(
            m.iter()
                .any(|s| s.contains("Scale (10) cannot exceed precision (5)")),
            "{m:?}"
        );
    }

    #[test]
    fn zero_precision_or_length_and_extra_varchar_params_diagnose() {
        let m = msgs(
            "Table t {\n  id int [pk]\n  c decimal(0)\n  d varchar(0)\n  e varchar(10, 2)\n}\n",
        );
        assert!(
            m.iter()
                .any(|s| s.contains("Precision for `decimal`") && s.contains("greater than 0")),
            "{m:?}"
        );
        assert!(
            m.iter()
                .any(|s| s.contains("Length for `varchar`") && s.contains("greater than 0")),
            "{m:?}"
        );
        assert!(
            m.iter()
                .any(|s| s.contains("accepts only 1 length parameter")),
            "{m:?}"
        );
    }

    #[test]
    fn ref_to_non_unique_column_warns() {
        let non_unique = "Table a {\n  id int [pk]\n  slug varchar(64)\n}\nTable b {\n  id int [pk]\n  a_slug varchar(64) [ref: > a.slug]\n}\n";
        let m = msgs(non_unique);
        assert!(
            m.iter()
                .any(|s| s.contains("Referenced column `a.slug` is neither `pk` nor `unique`")),
            "{m:?}"
        );
    }

    #[test]
    fn ref_to_unique_index_column_is_clean() {
        let via_index = "Table a {\n  id int [pk]\n  slug varchar(64)\n  indexes {\n    slug [unique]\n  }\n}\nTable b {\n  id int [pk]\n  a_slug varchar(64) [ref: > a.slug]\n}\n";
        assert!(lint(via_index).is_empty(), "{:?}", lint(via_index));
    }

    #[test]
    fn fk_cycle_across_tables_warns() {
        let cycle = "Table a {\n  id int [pk]\n  b_id int [ref: > b.id]\n}\nTable b {\n  id int [pk]\n  c_id int [ref: > c.id]\n}\nTable c {\n  id int [pk]\n  a_id int [ref: > a.id]\n}\n";
        let m = msgs(cycle);
        assert!(
            m.iter()
                .any(|s| s.contains("Circular foreign key dependency detected:")),
            "{m:?}"
        );
    }

    #[test]
    fn acyclic_diamond_fks_have_no_warnings() {
        let acyclic = "Table root {\n  id int [pk]\n}\nTable left {\n  id int [pk]\n  root_id int [ref: > root.id]\n}\nTable right {\n  id int [pk]\n  root_id int [ref: > root.id]\n}\nTable leaf {\n  id int [pk]\n  left_id int [ref: > left.id]\n  right_id int [ref: > right.id]\n}\n";
        assert!(lint(acyclic).is_empty(), "{:?}", lint(acyclic));
    }

    #[test]
    fn self_ref_not_null_without_default_warns_without_cycle_warning() {
        let bad = "Table categories {\n  id int [pk]\n  parent_id int [not null, ref: > categories.id]\n}\n";
        let m = msgs(bad);
        assert!(
            m.iter()
                .any(|s| s.contains("Self-referencing FK `categories.parent_id`")
                    && s.contains("not null")),
            "{m:?}"
        );
        assert!(
            !m.iter().any(|s| s.contains("Circular foreign key")),
            "{m:?}"
        );
    }

    #[test]
    fn self_ref_nullable_or_with_default_is_clean() {
        let ok_nullable =
            "Table categories {\n  id int [pk]\n  parent_id int [ref: > categories.id]\n}\n";
        assert!(lint(ok_nullable).is_empty(), "{:?}", lint(ok_nullable));

        let ok_default = "Table categories {\n  id int [pk]\n  parent_id int [not null, default: 0, ref: > categories.id]\n}\n";
        assert!(lint(ok_default).is_empty(), "{:?}", lint(ok_default));
    }

    #[test]
    fn lint_accepts_valid_and_rejects_invalid_referential_action() {
        let valid = "Table users {\n  id int [pk]\n}\nTable posts {\n  id int [pk]\n  user_id int [ref: > users.id, delete: cascade, update: set null]\n}\n";
        assert!(lint(valid).is_empty(), "{:?}", lint(valid));

        let invalid = "Table users {\n  id int [pk]\n}\nTable posts {\n  id int [pk]\n  user_id int [ref: > users.id, delete: explode]\n}\n";
        let m = msgs(invalid);
        assert!(
            m.iter()
                .any(|s| s.contains("Invalid referential action `explode`")),
            "{m:?}"
        );
    }

    #[test]
    fn lint_supports_multiple_schemas_same_table_name_and_cross_schema_refs() {
        let src = "\
Table auth.users {
  id int [pk]
}
Table billing.users {
  id int [pk]
  auth_id int [ref: > auth.users.id]
}
Table billing.invoices {
  id int [pk]
  user_id int
}
Ref: billing.invoices.user_id > auth.users.id
TableGroup core {
  auth.users
  billing.invoices
}
";
        assert!(lint(src).is_empty(), "{:?}", lint(src));
    }
}
