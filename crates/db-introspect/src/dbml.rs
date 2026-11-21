//! Deterministic DBML emitter for an [`Introspected`] catalog.

use crate::model::{Column, ForeignKey, Index, Introspected, Table};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// Renders the catalog as DBML: enums, tables, then references.
#[must_use]
pub fn to_dbml(db: &Introspected) -> String {
    let (db, notes) = disambiguate(db);
    let db = &db;
    let mut blocks: Vec<String> = Vec::new();
    if !notes.is_empty() {
        blocks.push(
            notes
                .iter()
                .map(|n| format!("// {n}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
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

/// Names that `ident` cannot quote verbatim (`"` and line breaks become `_`).
fn needs_cleaning(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '"' | '\n' | '\r'))
}

fn cleaned(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, '"' | '\n' | '\r') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// Maps each name that needs cleaning to a unique replacement within `names`.
///
/// Names that are already valid keep their spelling; a cleaned name that would collide with
/// another gets a numeric suffix. Returns the map and a note per collision resolved.
fn unique_names<'a>(
    names: impl Iterator<Item = &'a str> + Clone,
    what: &str,
    owner: &str,
) -> (HashMap<String, String>, Vec<String>) {
    let mut taken: HashSet<String> = names
        .clone()
        .filter(|n| !needs_cleaning(n))
        .map(str::to_owned)
        .collect();
    let mut map = HashMap::new();
    let mut notes = Vec::new();
    for name in names.filter(|n| needs_cleaning(n)) {
        if map.contains_key(name) {
            continue;
        }
        let base = cleaned(name);
        let mut candidate = base.clone();
        let mut n = 2;
        while taken.contains(&candidate) {
            candidate = format!("{base}_{n}");
            n += 1;
        }
        if candidate != base {
            notes.push(format!(
                "name collision: {what} {owner}{} exported as {candidate}",
                cleaned(name)
            ));
        }
        taken.insert(candidate.clone());
        map.insert(name.to_owned(), candidate);
    }
    (map, notes)
}

/// Returns a catalog whose table and column names are unique after `ident` cleaning, so two
/// distinct database names never collapse into the same DBML identifier.
fn disambiguate(db: &Introspected) -> (Introspected, Vec<String>) {
    let mut out = db.clone();
    let mut notes = Vec::new();
    let mut schemas: Vec<Option<&str>> = db.tables.iter().map(|t| t.schema.as_deref()).collect();
    schemas.sort_unstable();
    schemas.dedup();
    let mut table_maps: HashMap<Option<String>, HashMap<String, String>> = HashMap::new();
    for schema in schemas {
        let names = db
            .tables
            .iter()
            .filter(|t| t.schema.as_deref() == schema)
            .map(|t| t.name.as_str());
        let (map, n) = unique_names(names, "table", "");
        notes.extend(n);
        table_maps.insert(schema.map(str::to_owned), map);
    }
    let mut column_maps: HashMap<(Option<String>, String), HashMap<String, String>> =
        HashMap::new();
    for t in &db.tables {
        let owner = format!("{}.", t.name);
        let (map, n) = unique_names(t.columns.iter().map(|c| c.name.as_str()), "column", &owner);
        notes.extend(n);
        column_maps.insert((t.schema.clone(), t.name.clone()), map);
    }
    let col = |schema: &Option<String>, table: &str, name: &str| -> String {
        column_maps
            .get(&(schema.clone(), table.to_owned()))
            .and_then(|m| m.get(name))
            .cloned()
            .unwrap_or_else(|| name.to_owned())
    };
    let tbl = |schema: &Option<String>, name: &str| -> String {
        table_maps
            .get(schema)
            .and_then(|m| m.get(name))
            .cloned()
            .unwrap_or_else(|| name.to_owned())
    };
    for t in &mut out.tables {
        let (schema, orig) = (t.schema.clone(), t.name.clone());
        for c in &mut t.columns {
            c.name = col(&schema, &orig, &c.name);
        }
        for i in &mut t.indexes {
            for c in &mut i.columns {
                *c = col(&schema, &orig, c);
            }
        }
        t.name = tbl(&schema, &orig);
    }
    for r in &mut out.refs {
        for c in &mut r.from_cols {
            *c = col(&r.from_schema, &r.from_table, c);
        }
        for c in &mut r.to_cols {
            *c = col(&r.to_schema, &r.to_table, c);
        }
        r.from_table = tbl(&r.from_schema, &r.from_table);
        r.to_table = tbl(&r.to_schema, &r.to_table);
    }
    (out, notes)
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
    if let Some(check) = &c.check {
        settings.push(format!("check: `{check}`"));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str) -> Column {
        Column {
            name: name.into(),
            ty: "int".into(),
            not_null: false,
            primary_key: false,
            unique: false,
            auto_increment: false,
            default: None,
            comment: None,
            check: None,
        }
    }

    fn tbl(name: &str, cols: &[&str]) -> Table {
        Table {
            schema: None,
            name: name.into(),
            columns: cols.iter().map(|c| col(c)).collect(),
            indexes: Vec::new(),
            comment: None,
        }
    }

    #[test]
    fn distinct_table_names_never_collapse_into_one_identifier() {
        let db = Introspected {
            tables: vec![tbl("a\"b", &["id"]), tbl("a_b", &["id"])],
            refs: vec![ForeignKey {
                from_schema: None,
                from_table: "a\"b".into(),
                from_cols: vec!["id".into()],
                to_schema: None,
                to_table: "a_b".into(),
                to_cols: vec!["id".into()],
                on_delete: None,
                on_update: None,
            }],
            ..Default::default()
        };
        let dbml = to_dbml(&db);
        assert!(dbml.contains("Table a_b_2 {"), "{dbml}");
        assert!(dbml.contains("Table a_b {"), "{dbml}");
        assert!(dbml.contains("Ref: a_b_2.id > a_b.id"), "{dbml}");
        assert!(dbml.contains("// name collision"), "{dbml}");
    }

    #[test]
    fn distinct_column_names_never_collapse_into_one_identifier() {
        let db = Introspected {
            tables: vec![tbl("t", &["x\"y", "x_y"])],
            ..Default::default()
        };
        let dbml = to_dbml(&db);
        assert!(dbml.contains("  x_y_2 int\n  x_y int\n"), "{dbml}");
    }

    #[test]
    fn names_without_collision_are_unchanged_and_silent() {
        let db = Introspected {
            tables: vec![tbl("a\"b", &["id"]), tbl("c", &["id"])],
            ..Default::default()
        };
        let dbml = to_dbml(&db);
        assert!(dbml.contains("Table a_b {"), "{dbml}");
        assert!(!dbml.contains("// name collision"), "{dbml}");
    }
}
