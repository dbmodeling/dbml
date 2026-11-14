//! Deterministic DBML emitter for a [`Resolved`] schema.

use crate::SqlDialect;
use crate::model::{DefaultExpr, FkAction, IndexCol, Name};
use crate::resolve::{IndexKind, RIndex, RRef, RTable, Resolved};
use std::fmt::Write as _;

/// `(sql type in lowercase, DBML type)`; applied to the part of the type before any `(` or `[`.
const TYPE_MAP: &[(&str, &str)] = &[
    ("character varying", "varchar"),
    ("character", "char"),
    ("int2", "smallint"),
    ("int4", "integer"),
    ("int8", "bigint"),
    ("bool", "boolean"),
    ("float4", "real"),
    ("float8", "double"),
    ("double precision", "double"),
    ("timestamp with time zone", "timestamptz"),
    ("timestamp without time zone", "timestamp"),
    ("time with time zone", "timetz"),
    ("time without time zone", "time"),
];

/// Fallback for SQLite columns declared without a type.
const UNTYPED_FALLBACK: &str = "text";

/// Renders `resolved` as DBML: enums, tables, then references.
pub(crate) fn emit(resolved: &Resolved<'_>, dialect: SqlDialect) -> String {
    let mut blocks: Vec<String> = Vec::new();
    let enum_names: Vec<String> = resolved
        .enums
        .iter()
        .flat_map(|e| {
            let plain = e.name.name.to_ascii_lowercase();
            match &e.name.schema {
                Some(s) => vec![plain.clone(), format!("{}.{plain}", s.to_ascii_lowercase())],
                None => vec![plain],
            }
        })
        .collect();
    for e in &resolved.enums {
        let mut b = String::from("Enum ");
        name(&mut b, &e.name);
        b.push_str(" {\n");
        for v in &e.values {
            let text = if is_bare(v) { v.clone() } else { literal(v) };
            let _ = writeln!(b, "  {text}");
        }
        b.push('}');
        blocks.push(b);
    }
    blocks.extend(
        resolved
            .tables
            .iter()
            .map(|t| table(t, dialect, &enum_names)),
    );
    blocks.extend(resolved.refs.iter().map(reference));
    if blocks.is_empty() {
        return String::new();
    }
    let mut out = blocks.join("\n\n");
    out.push('\n');
    out
}

fn is_bare(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Identifier: bare when DBML allows it, otherwise double-quoted. DBML has no escape sequences,
/// so characters that cannot appear inside quotes are replaced by `_`.
fn ident(out: &mut String, s: &str) {
    if is_bare(s) {
        out.push_str(s);
    } else {
        out.push('"');
        out.extend(s.chars().map(|c| {
            if matches!(c, '"' | '\n' | '\r') {
                '_'
            } else {
                c
            }
        }));
        out.push('"');
    }
}

fn name(out: &mut String, n: &Name<'_>) {
    if let Some(schema) = &n.schema {
        ident(out, schema);
        out.push('.');
    }
    ident(out, &n.name);
}

/// String literal. The DBML lexer has no escapes: prefer `'..'`, fall back to `".."`, and as a
/// last resort replace `'` by a typographic apostrophe. Line breaks become spaces.
fn literal(s: &str) -> String {
    let flat: String = s
        .chars()
        .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
        .collect();
    if !flat.contains('\'') {
        format!("'{flat}'")
    } else if !flat.contains('"') {
        format!("\"{flat}\"")
    } else {
        format!("'{}'", flat.replace('\'', "\u{2019}"))
    }
}

/// Backtick expression; DBML has no escape for a backtick, so it is replaced by `'`.
fn expression(s: &str) -> String {
    let flat: String = s
        .chars()
        .map(|c| match c {
            '`' => '\'',
            '\n' | '\r' => ' ',
            c => c,
        })
        .collect();
    format!("`{flat}`")
}

fn strip_outer_parens(s: &str) -> &str {
    let t = s.trim();
    let Some(inner) = t.strip_prefix('(').and_then(|r| r.strip_suffix(')')) else {
        return t;
    };
    let mut depth = 0usize;
    for c in inner.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return t; // the first `(` closes before the end: not a wrapping pair
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    if depth == 0 {
        strip_outer_parens(inner)
    } else {
        t
    }
}

/// SQL Server / Oracle spellings that the exporter emits, mapped back to canonical DBML types so
/// `import(export(x))` is stable. `None` leaves the type to the generic mapping.
fn proprietary_type(ty: &str, dialect: SqlDialect) -> Option<String> {
    let compact: String = ty
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    let (base, params) = match compact.find('(') {
        Some(i) => compact.split_at(i),
        None => (compact.as_str(), ""),
    };
    let mapped = match dialect {
        SqlDialect::SqlServer => match (base, params) {
            ("nvarchar" | "varchar", "(max)") | ("ntext", _) => "text".to_owned(),
            ("nvarchar", p) => format!("varchar{p}"),
            ("nchar", p) => format!("char{p}"),
            ("datetime2" | "datetime" | "smalldatetime", _) => "timestamp".to_owned(),
            ("bit", _) => "boolean".to_owned(),
            ("uniqueidentifier", _) => "uuid".to_owned(),
            _ => return None,
        },
        SqlDialect::Oracle => match (base, params) {
            ("varchar2" | "nvarchar2", p) => format!("varchar{p}"),
            ("clob" | "nclob", _) => "text".to_owned(),
            ("number", "(10)") => "integer".to_owned(),
            ("number", "(19)") => "bigint".to_owned(),
            ("number", "(1)") => "boolean".to_owned(),
            ("raw", "(16)") => "uuid".to_owned(),
            _ => return None,
        },
        _ => return None,
    };
    Some(mapped)
}

fn dbml_type(ty: &str, dialect: SqlDialect, enum_names: &[String]) -> String {
    let ty = ty.trim();
    if ty.is_empty() {
        return UNTYPED_FALLBACK.to_owned();
    }
    if dialect == SqlDialect::Mysql && ty.eq_ignore_ascii_case("tinyint(1)") {
        return "boolean".to_owned();
    }
    if let Some(mapped) = proprietary_type(ty, dialect) {
        return mapped;
    }
    let split = ty.find(['(', '[']).unwrap_or(ty.len());
    let (base, suffix) = ty.split_at(split);
    let base = base.trim();
    let mapped = TYPE_MAP
        .iter()
        .find(|(sql, _)| sql.eq_ignore_ascii_case(base))
        .map_or_else(
            || {
                // Enum names keep their spelling; built-in types are lowercased (DBML convention).
                if enum_names.contains(&base.to_ascii_lowercase()) {
                    base.to_owned()
                } else {
                    base.to_ascii_lowercase()
                }
            },
            |(_, dbml)| (*dbml).to_owned(),
        );
    let full = format!("{mapped}{suffix}");
    if full.contains(char::is_whitespace) {
        format!("\"{full}\"")
    } else {
        full
    }
}

fn default_value(d: &DefaultExpr, dbml_ty: &str) -> String {
    match d {
        DefaultExpr::Null => "null".to_owned(),
        DefaultExpr::Bool(b) => b.to_string(),
        // MySQL / SQLite spell booleans as 1 and 0.
        DefaultExpr::Number(n) | DefaultExpr::Str(n)
            if dbml_ty == "boolean" && matches!(n.as_str(), "0" | "1") =>
        {
            (n == "1").to_string()
        }
        DefaultExpr::Number(n) => n.clone(),
        DefaultExpr::Str(s) => literal(s),
        DefaultExpr::Raw(r) => expression(strip_outer_parens(r)),
    }
}

fn action(a: FkAction) -> &'static str {
    match a {
        FkAction::NoAction => "no action",
        FkAction::Restrict => "restrict",
        FkAction::Cascade => "cascade",
        FkAction::SetNull => "set null",
        FkAction::SetDefault => "set default",
    }
}

fn table(t: &RTable<'_>, dialect: SqlDialect, enum_names: &[String]) -> String {
    let mut out = String::from("Table ");
    name(&mut out, &t.name);
    out.push_str(" {\n");
    for c in &t.columns {
        out.push_str("  ");
        ident(&mut out, &c.name);
        let ty = dbml_type(&c.ty, dialect, enum_names);
        let _ = write!(out, " {ty}");
        let mut settings: Vec<String> = Vec::new();
        if c.primary_key {
            settings.push("pk".to_owned());
        }
        if c.auto_increment {
            settings.push("increment".to_owned());
        }
        if c.not_null && !c.primary_key {
            settings.push("not null".to_owned());
        }
        if c.unique && !c.primary_key {
            settings.push("unique".to_owned());
        }
        if let Some(d) = &c.default {
            settings.push(format!("default: {}", default_value(d, &ty)));
        }
        if let Some(n) = &c.comment {
            settings.push(format!("note: {}", literal(n)));
        }
        if !settings.is_empty() {
            let _ = write!(out, " [{}]", settings.join(", "));
        }
        out.push('\n');
    }
    if !t.indexes.is_empty() {
        out.push_str("  indexes {\n");
        for i in &t.indexes {
            let _ = writeln!(out, "    {}", index(i));
        }
        out.push_str("  }\n");
    }
    if let Some(n) = &t.comment {
        let _ = writeln!(out, "  Note: {}", literal(n));
    }
    out.push('}');
    out
}

fn index(i: &RIndex<'_>) -> String {
    let cols: Vec<String> = i
        .cols
        .iter()
        .map(|c| match c {
            IndexCol::Column(c) => {
                let mut s = String::new();
                ident(&mut s, c);
                s
            }
            IndexCol::Expr(e) => expression(e),
        })
        .collect();
    let mut out = match cols.as_slice() {
        [only] if matches!(i.cols.first(), Some(IndexCol::Column(_))) => only.clone(),
        _ => format!("({})", cols.join(", ")),
    };
    let mut settings: Vec<String> = Vec::new();
    match i.kind {
        IndexKind::PrimaryKey => settings.push("pk".to_owned()),
        IndexKind::Unique => settings.push("unique".to_owned()),
        IndexKind::Plain => {}
    }
    if let Some(n) = &i.name {
        settings.push(format!("name: {}", literal(n)));
    }
    if !settings.is_empty() {
        let _ = write!(out, " [{}]", settings.join(", "));
    }
    out
}

fn endpoint(out: &mut String, table: &Name<'_>, cols: &[crate::model::Text<'_>]) {
    name(out, table);
    out.push('.');
    if let [only] = cols {
        ident(out, only);
    } else {
        out.push('(');
        for (i, c) in cols.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            ident(out, c);
        }
        out.push(')');
    }
}

fn reference(r: &RRef<'_>) -> String {
    let mut out = String::from("Ref: ");
    endpoint(&mut out, &r.from_table, &r.from_cols);
    out.push_str(if r.one_to_one { " - " } else { " > " });
    endpoint(&mut out, &r.to_table, &r.to_cols);
    let mut settings: Vec<String> = Vec::new();
    if let Some(a) = r.on_delete {
        settings.push(format!("delete: {}", action(a)));
    }
    if let Some(a) = r.on_update {
        settings.push(format!("update: {}", action(a)));
    }
    if !settings.is_empty() {
        let _ = write!(out, " [{}]", settings.join(", "));
    }
    out
}
