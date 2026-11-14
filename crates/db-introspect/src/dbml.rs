//! Deterministic DBML emitter for an [`Introspected`] catalog.

use crate::model::{Column, ForeignKey, Index, Introspected, Table};
use std::fmt::Write as _;

/// Renders the catalog as DBML: enums, tables, then references.
#[must_use]
pub fn to_dbml(db: &Introspected) -> String {
    let mut blocks: Vec<String> = Vec::new();
    for e in &db.enums {
        let mut b = String::from("Enum ");
        qualified(&mut b, e.schema.as_deref(), &e.name);
        b.push_str(" {\n");
        for v in &e.values {
            let text = if is_bare(v) { v.clone() } else { literal(v) };
            let _ = writeln!(b, "  {text}");
        }
        b.push('}');
        blocks.push(b);
    }
    blocks.extend(db.tables.iter().map(table));
    blocks.extend(db.refs.iter().map(reference));
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

fn qualified(out: &mut String, schema: Option<&str>, name: &str) {
    if let Some(s) = schema {
        ident(out, s);
        out.push('.');
    }
    ident(out, name);
}

/// DBML strings have no escapes: prefer `'..'`, then `".."`, else swap the apostrophe.
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

pub(crate) fn strip_outer_parens(s: &str) -> &str {
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
                    return t;
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

fn is_number(s: &str) -> bool {
    s.parse::<f64>().is_ok()
}

fn default_value(raw: &str) -> String {
    let t = strip_outer_parens(raw);
    let lower = t.to_ascii_lowercase();
    if lower == "null" || lower == "true" || lower == "false" {
        return lower;
    }
    if is_number(t) {
        return t.to_owned();
    }
    if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') {
        return literal(&t[1..t.len() - 1].replace("''", "'"));
    }
    expression(t)
}

fn dbml_type(ty: &str) -> String {
    let ty = ty.trim();
    if ty.is_empty() {
        return "text".to_owned();
    }
    let lower = ty.to_ascii_lowercase();
    if lower.contains(char::is_whitespace) {
        format!("\"{lower}\"")
    } else {
        lower
    }
}

fn column(out: &mut String, c: &Column) {
    out.push_str("  ");
    ident(out, &c.name);
    let _ = write!(out, " {}", dbml_type(&c.ty));
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
        settings.push(format!("default: {}", default_value(d)));
    }
    if let Some(n) = &c.comment {
        settings.push(format!("note: {}", literal(n)));
    }
    if !settings.is_empty() {
        let _ = write!(out, " [{}]", settings.join(", "));
    }
    out.push('\n');
}

fn index(i: &Index) -> String {
    let cols: Vec<String> = i
        .columns
        .iter()
        .map(|c| {
            let mut s = String::new();
            ident(&mut s, c);
            s
        })
        .collect();
    let mut out = if let [only] = cols.as_slice() {
        only.clone()
    } else {
        format!("({})", cols.join(", "))
    };
    let mut settings: Vec<String> = Vec::new();
    if i.primary_key {
        settings.push("pk".to_owned());
    } else if i.unique {
        settings.push("unique".to_owned());
    }
    if let Some(n) = &i.name {
        settings.push(format!("name: {}", literal(n)));
    }
    if !settings.is_empty() {
        let _ = write!(out, " [{}]", settings.join(", "));
    }
    out
}

fn table(t: &Table) -> String {
    let mut out = String::from("Table ");
    qualified(&mut out, t.schema.as_deref(), &t.name);
    out.push_str(" {\n");
    for c in &t.columns {
        column(&mut out, c);
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

fn endpoint(out: &mut String, schema: Option<&str>, table: &str, cols: &[String]) {
    qualified(out, schema, table);
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

fn reference(r: &ForeignKey) -> String {
    let mut out = String::from("Ref: ");
    endpoint(
        &mut out,
        r.from_schema.as_deref(),
        &r.from_table,
        &r.from_cols,
    );
    out.push_str(" > ");
    endpoint(&mut out, r.to_schema.as_deref(), &r.to_table, &r.to_cols);
    let mut settings: Vec<String> = Vec::new();
    if let Some(a) = &r.on_delete {
        settings.push(format!("delete: {a}"));
    }
    if let Some(a) = &r.on_update {
        settings.push(format!("update: {a}"));
    }
    if !settings.is_empty() {
        let _ = write!(out, " [{}]", settings.join(", "));
    }
    out
}
