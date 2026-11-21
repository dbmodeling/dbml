//! `dbml diff <old.dbml> <new.dbml>`: diff estrutural entre dois arquivos DBML.
//!
//! O cálculo vive em [`schema::diff_schemas`]; este módulo só carrega os arquivos, valida a
//! sintaxe e formata o resultado (texto legível ou JSON). A formatação é pura e testável.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context as _;
use schema::{
    Change, Column, Endpoint, EnumChange, EnumValue, IndexDef, Nullability, Ref, Schema,
    SchemaDiff, Table, TableChange, TableProps, diff_schemas,
};

use crate::args::DiffFormat;

/// Código de saída quando os arquivos são estruturalmente iguais.
pub const EXIT_SAME: i32 = 0;
/// Código de saída quando há diferenças (como `diff`).
pub const EXIT_DIFFERENT: i32 = 1;
/// Código de saída para erros de leitura ou sintaxe.
pub const EXIT_ERROR: i32 = 2;

/// Resultado de [`run_diff`]: texto pronto para stdout e se houve diferenças.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffReport {
    /// Saída formatada (texto ou JSON), terminada em quebra de linha.
    pub output: String,
    /// `true` quando existe ao menos uma diferença.
    pub differs: bool,
}

impl DiffReport {
    /// Código de saída do processo: [`EXIT_SAME`] ou [`EXIT_DIFFERENT`].
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        if self.differs {
            EXIT_DIFFERENT
        } else {
            EXIT_SAME
        }
    }
}

/// Lê `path` e o converte em [`Schema`], recusando fontes com erro de sintaxe/semântica.
///
/// # Errors
///
/// Falha se o arquivo não puder ser lido ou se o lint reportar um erro (a mensagem
/// cita o arquivo, a linha e a coluna).
pub fn load_schema(path: &Path) -> anyhow::Result<Schema> {
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read `{}`", path.display()))?;
    if let Some(d) = dbml_analysis::lint(&src)
        .into_iter()
        .find(|d| d.severity == dbml_analysis::Severity::Error)
    {
        let (line, col) = line_col(&src, d.range.start);
        anyhow::bail!(
            "{}:{line}:{col}: invalid DBML: {}",
            path.display(),
            d.message
        );
    }
    Ok(dbml_analysis::analyze(&src))
}

/// Converte um offset em bytes para `(linha, coluna)` 1-based (coluna em caracteres).
fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(src.len());
    while !src.is_char_boundary(end) {
        end -= 1;
    }
    let before = &src[..end];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

/// Compara `old` com `new` e devolve a saída no formato pedido.
///
/// # Errors
///
/// Falha se algum arquivo não puder ser lido ou tiver erro de sintaxe.
pub fn run_diff(old: &Path, new: &Path, format: DiffFormat) -> anyhow::Result<DiffReport> {
    let base = load_schema(old)?;
    let head = load_schema(new)?;
    let diff = diff_schemas(&base, &head);
    let output = match format {
        DiffFormat::Text => format_text(&diff),
        DiffFormat::Json => {
            let mut json =
                serde_json::to_string_pretty(&diff).context("failed to serialize the diff")?;
            json.push('\n');
            json
        }
    };
    Ok(DiffReport {
        output,
        differs: !diff.is_empty(),
    })
}

/// Formata o diff como texto legível, agrupado em tabelas, enums e refs.
///
/// `+` adicionado, `-` removido, `~` modificado (com `antes -> depois`).
#[must_use]
pub fn format_text(diff: &SchemaDiff) -> String {
    if diff.is_empty() {
        return "No differences.\n".to_owned();
    }
    let mut out = String::new();
    if !diff.tables.is_empty() {
        out.push_str("Tables\n");
        for change in &diff.tables {
            write_table_change(&mut out, change);
        }
    }
    if !diff.enums.is_empty() {
        out.push_str("Enums\n");
        for change in &diff.enums {
            write_enum_change(&mut out, change);
        }
    }
    if !diff.refs.is_empty() {
        out.push_str("Refs\n");
        for change in &diff.refs {
            write_ref_change(&mut out, change);
        }
    }
    let s = diff.summary();
    let _ = writeln!(
        out,
        "Summary: {} added, {} removed, {} modified",
        s.added, s.removed, s.modified
    );
    out
}

fn qualified(schema: Option<&str>, name: &str) -> String {
    match schema {
        Some(s) if !s.is_empty() => format!("{s}.{name}"),
        _ => name.to_owned(),
    }
}

fn write_table_change(out: &mut String, change: &TableChange) {
    match change {
        TableChange::Added(t) => {
            let _ = writeln!(out, "  + {}", table_label(t));
        }
        TableChange::Removed(t) => {
            let _ = writeln!(out, "  - {}", table_label(t));
        }
        TableChange::Modified(m) => {
            let _ = writeln!(out, "  ~ {}", qualified(m.schema.as_deref(), &m.name));
            if let Some(p) = &m.props {
                let _ = writeln!(
                    out,
                    "      ~ properties: {} -> {}",
                    props_sig(&p.before),
                    props_sig(&p.after)
                );
            }
            for c in &m.columns {
                write_change(out, "column", c, column_label, column_modified);
            }
            for c in &m.indexes {
                write_change(out, "index", c, index_sig, |b, a| {
                    format!("{} -> {}", index_sig(b), index_sig(a))
                });
            }
        }
    }
}

fn table_label(t: &Table) -> String {
    format!("{} ({} columns)", t.qualified_name(), t.columns.len())
}

fn write_enum_change(out: &mut String, change: &EnumChange) {
    match change {
        EnumChange::Added(e) => {
            let _ = writeln!(
                out,
                "  + {} ({} values)",
                e.qualified_name(),
                e.values.len()
            );
        }
        EnumChange::Removed(e) => {
            let _ = writeln!(
                out,
                "  - {} ({} values)",
                e.qualified_name(),
                e.values.len()
            );
        }
        EnumChange::Modified(m) => {
            let _ = writeln!(out, "  ~ {}", qualified(m.schema.as_deref(), &m.name));
            for c in &m.values {
                write_change(out, "value", c, value_label, value_modified);
            }
        }
    }
}

fn write_ref_change(out: &mut String, change: &Change<Ref>) {
    match change {
        Change::Added(r) => {
            let _ = writeln!(out, "  + {}", ref_line(r));
        }
        Change::Removed(r) => {
            let _ = writeln!(out, "  - {}", ref_line(r));
        }
        Change::Modified { before, after } => {
            let _ = writeln!(
                out,
                "  ~ ref {} to {}: {} -> {}",
                endpoint(&after.from),
                endpoint(&after.to),
                ref_sig(before),
                ref_sig(after)
            );
        }
    }
}

/// Escreve uma linha `+`/`-`/`~` para um item aninhado. `label` descreve o item em
/// adições/remoções; `modified` descreve `antes -> depois` de uma modificação.
fn write_change<T>(
    out: &mut String,
    kind: &str,
    change: &Change<T>,
    label: impl Fn(&T) -> String,
    modified: impl Fn(&T, &T) -> String,
) {
    match change {
        Change::Added(v) => {
            let _ = writeln!(out, "      + {kind} {}", label(v));
        }
        Change::Removed(v) => {
            let _ = writeln!(out, "      - {kind} {}", label(v));
        }
        Change::Modified { before, after } => {
            let _ = writeln!(out, "      ~ {kind} {}", modified(before, after));
        }
    }
}

fn column_label(c: &Column) -> String {
    format!("{}: {}", c.name, column_sig(c))
}

fn column_modified(before: &Column, after: &Column) -> String {
    format!(
        "{}: {} -> {}",
        after.name,
        column_sig(before),
        column_sig(after)
    )
}

/// Tipo e atributos da coluna, ex.: `varchar(50) [pk, not null, default: 1]`.
fn column_sig(c: &Column) -> String {
    let mut flags: Vec<String> = Vec::new();
    if c.pk {
        flags.push("pk".into());
    }
    if c.fk {
        flags.push("fk".into());
    }
    match c.nullability {
        Nullability::NotNull => flags.push("not null".into()),
        Nullability::Nullable => flags.push("null".into()),
        Nullability::Unspecified => {
            if c.not_null {
                flags.push("not null".into());
            }
        }
    }
    if c.unique {
        flags.push("unique".into());
    }
    if c.increment {
        flags.push("increment".into());
    }
    if let Some(d) = &c.default_value {
        flags.push(format!("default: {d}"));
    }
    if let Some(ck) = &c.check {
        flags.push(format!("check: {ck}"));
    }
    if let Some(n) = &c.note {
        flags.push(format!("note: {n}"));
    }
    if flags.is_empty() {
        c.ty.clone()
    } else {
        format!("{} [{}]", c.ty, flags.join(", "))
    }
}

/// Colunas e atributos do índice, ex.: `(a, b) [unique, name: ix]`.
fn index_sig(i: &IndexDef) -> String {
    let cols: Vec<String> = i
        .columns
        .iter()
        .map(schema::IndexColumn::as_dbml_str)
        .collect();
    let mut s = format!("({})", cols.join(", "));
    let mut flags: Vec<String> = Vec::new();
    if i.pk {
        flags.push("pk".into());
    }
    if i.unique {
        flags.push("unique".into());
    }
    if let Some(n) = &i.name {
        flags.push(format!("name: {n}"));
    }
    if let Some(t) = i.index_type {
        flags.push(format!("type: {}", t.as_sql_str()));
    }
    if !flags.is_empty() {
        let _ = write!(s, " [{}]", flags.join(", "));
    }
    s
}

fn value_label(v: &EnumValue) -> String {
    v.name.clone()
}

fn value_modified(before: &EnumValue, after: &EnumValue) -> String {
    let note = |v: &EnumValue| {
        v.note
            .as_ref()
            .map_or_else(|| "[no note]".to_owned(), |n| format!("[note: {n}]"))
    };
    format!("{}: {} -> {}", after.name, note(before), note(after))
}

fn props_sig(p: &TableProps) -> String {
    let mut flags: Vec<String> = Vec::new();
    if let Some(c) = &p.color {
        flags.push(format!("color: {c}"));
    }
    if let Some(a) = &p.alias {
        flags.push(format!("alias: {a}"));
    }
    if let Some(n) = &p.note {
        flags.push(format!("note: {n}"));
    }
    format!("[{}]", flags.join(", "))
}

fn endpoint(e: &Endpoint) -> String {
    let cols = e.cols();
    let cols = match cols.as_slice() {
        [one] => (*one).to_owned(),
        many => format!("({})", many.join(", ")),
    };
    format!("{}.{cols}", e.qualified_table())
}

fn ref_actions(r: &Ref) -> String {
    let mut flags: Vec<String> = Vec::new();
    if let Some(a) = r.on_delete {
        flags.push(format!("delete: {}", a.as_dbml_str()));
    }
    if let Some(a) = r.on_update {
        flags.push(format!("update: {}", a.as_dbml_str()));
    }
    if flags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", flags.join(", "))
    }
}

/// Operador e ações, ex.: `> [delete: cascade]`.
fn ref_sig(r: &Ref) -> String {
    format!("{}{}", r.kind.as_dbml_op(), ref_actions(r))
}

/// Ref completa, ex.: `orders.user_id > users.id [delete: cascade]`.
fn ref_line(r: &Ref) -> String {
    format!(
        "{} {} {}{}",
        endpoint(&r.from),
        r.kind.as_dbml_op(),
        endpoint(&r.to),
        ref_actions(r)
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn diff_of(old: &str, new: &str) -> SchemaDiff {
        diff_schemas(&dbml_analysis::analyze(old), &dbml_analysis::analyze(new))
    }

    #[test]
    fn identical_schemas_report_no_differences() {
        let src = "Table users { id int [pk] }";
        let d = diff_of(src, src);
        assert_eq!(format_text(&d), "No differences.\n");
    }

    #[test]
    fn text_groups_tables_enums_refs_and_summarizes() {
        let old = "Table users { id int [pk] }\n\
                   Table legacy { id int }\n\
                   Table orders { id int [pk]\n status varchar [not null]\n old_col text }\n\
                   Enum kind { a\n b }";
        let new = "Table users { id int [pk] }\n\
                   Table orders { id int [pk]\n status varchar(50) [not null]\n total numeric }\n\
                   Table audit { id int }\n\
                   Enum kind { a\n c }\n\
                   Ref: orders.id > users.id [delete: cascade]";
        let text = format_text(&diff_of(old, new));
        let expect = "\
Tables
  - legacy (1 columns)
  ~ orders
      ~ column status: varchar [not null] -> varchar(50) [not null]
      - column old_col: text
      + column total: numeric
  + audit (1 columns)
Enums
  ~ kind
      - value b
      + value c
Refs
  + orders.id > users.id [delete: cascade]
Summary: 4 added, 3 removed, 1 modified
";
        assert_eq!(text, expect);
    }

    #[test]
    fn text_shows_index_and_modified_ref_and_props() {
        let old = "Table a [headercolor: #111111] { x int\n y int\n indexes { x } }\n\
                   Table b { id int }\n\
                   Ref: a.y > b.id";
        let new = "Table a [headercolor: #222222] { x int\n y int\n indexes { (x, y) [unique] } }\n\
                   Table b { id int }\n\
                   Ref: a.y < b.id [delete: cascade]";
        let text = format_text(&diff_of(old, new));
        assert!(
            text.contains("~ properties: [color: #111111] -> [color: #222222]"),
            "{text}"
        );
        assert!(text.contains("- index (x)\n"), "{text}");
        assert!(text.contains("+ index (x, y) [unique]\n"), "{text}");
        assert!(
            text.contains("~ ref a.y to b.id: > -> < [delete: cascade]"),
            "{text}"
        );
    }

    #[test]
    fn json_is_the_serialized_diff() {
        let dir = temp_dir("json");
        let (old, new) = write_pair(
            &dir,
            "Table a { id int }",
            "Table a { id int }\nTable b { id int }",
        );
        let report = run_diff(&old, &new, DiffFormat::Json).unwrap();
        assert!(report.differs);
        let v: serde_json::Value = serde_json::from_str(&report.output).unwrap();
        assert_eq!(v["tables"][0]["kind"], "added");
        assert_eq!(v["tables"][0]["value"]["name"], "b");
        assert_eq!(v["enums"].as_array().map(Vec::len), Some(0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_diff_exit_codes_follow_differences() {
        let dir = temp_dir("exit");
        let (old, new) = write_pair(&dir, "Table a { id int }", "Table a { id int }");
        let same = run_diff(&old, &new, DiffFormat::Text).unwrap();
        assert!(!same.differs);
        assert_eq!(same.exit_code(), EXIT_SAME);

        std::fs::write(&new, "Table a { id bigint }").unwrap();
        let diff = run_diff(&old, &new, DiffFormat::Text).unwrap();
        assert_eq!(diff.exit_code(), EXIT_DIFFERENT);
        assert!(
            diff.output.contains("~ column id: int -> bigint"),
            "{}",
            diff.output
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn syntax_error_names_the_offending_file() {
        let dir = temp_dir("syntax");
        let (old, new) = write_pair(&dir, "Table a { id int }", "Table a { id int");
        let err = run_diff(&old, &new, DiffFormat::Text)
            .unwrap_err()
            .to_string();
        assert!(err.contains(&new.display().to_string()), "{err}");
        assert!(!err.contains(&old.display().to_string()), "{err}");

        let err = run_diff(&new, &old, DiffFormat::Text)
            .unwrap_err()
            .to_string();
        assert!(err.contains(&new.display().to_string()), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_an_error_naming_the_path() {
        let dir = temp_dir("missing");
        let (old, _) = write_pair(&dir, "Table a { id int }", "");
        let gone = dir.join("nope.dbml");
        let err = format!("{:#}", run_diff(&old, &gone, DiffFormat::Text).unwrap_err());
        assert!(err.contains("nope.dbml"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn line_col_is_one_based_and_utf8_safe() {
        assert_eq!(line_col("ab\ncd", 0), (1, 1));
        assert_eq!(line_col("ab\ncd", 4), (2, 2));
        assert_eq!(line_col("é\nx", 1), (1, 1));
        assert_eq!(line_col("abc", 99), (1, 4));
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dbml_cli_diff_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_pair(dir: &Path, old: &str, new: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let (o, n) = (dir.join("old.dbml"), dir.join("new.dbml"));
        std::fs::write(&o, old).unwrap();
        std::fs::write(&n, new).unwrap();
        (o, n)
    }
}
