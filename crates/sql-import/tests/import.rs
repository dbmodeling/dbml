//! Public-API tests: snapshots, DBML validity, determinism and export round-trips.

use dbml_analysis::{Severity, SqlDialect as ExportDialect, analyze, export_sql_ddl, lint};
use sql_import::{SqlDialect, import_sql_ddl};

const PG: &str = include_str!("fixtures/postgres.sql");
const MYSQL: &str = include_str!("fixtures/mysql.sql");
const SQLITE: &str = include_str!("fixtures/sqlite.sql");

const ALL: [(SqlDialect, ExportDialect); 5] = [
    (SqlDialect::Postgres, ExportDialect::Postgres),
    (SqlDialect::Mysql, ExportDialect::Mysql),
    (SqlDialect::Sqlite, ExportDialect::Sqlite),
    (SqlDialect::SqlServer, ExportDialect::SqlServer),
    (SqlDialect::Oracle, ExportDialect::Oracle),
];

const ROUND_TRIP_SAMPLE: &str = "\
Enum role {
  admin
  member
}

Table users {
  id int [pk, increment]
  email varchar(120) [unique, not null]
  role role [not null, default: 'member']
  active bool [default: true]
  indexes {
    email [name: 'idx_users_email']
  }
}

Table memberships {
  org_id int [not null]
  user_id int [not null]
  indexes {
    (org_id, user_id) [pk]
  }
}

Table posts {
  id int [pk]
  user_id int [not null, ref: > users.id, delete: cascade, update: no action]
  title varchar(200) [not null, default: 'untitled']
}
";

fn errors(dbml: &str) -> Vec<String> {
    lint(dbml)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message)
        .collect()
}

/// Syntax-level errors only: arbitrary SQL may legitimately use types DBML does not know.
fn syntax_errors(dbml: &str) -> Vec<String> {
    errors(dbml)
        .into_iter()
        .filter(|m| !m.starts_with("Unknown type"))
        .collect()
}

#[test]
fn import_should_match_postgres_fixture_snapshot() {
    insta::assert_snapshot!(import_sql_ddl(PG, SqlDialect::Postgres).dbml);
}

#[test]
fn import_should_match_mysql_fixture_snapshot() {
    insta::assert_snapshot!(import_sql_ddl(MYSQL, SqlDialect::Mysql).dbml);
}

#[test]
fn import_should_match_sqlite_fixture_snapshot() {
    insta::assert_snapshot!(import_sql_ddl(SQLITE, SqlDialect::Sqlite).dbml);
}

#[test]
fn emitted_postgres_dbml_should_have_no_lint_errors() {
    let out = import_sql_ddl(PG, SqlDialect::Postgres).dbml;
    assert_eq!(errors(&out), Vec::<String>::new(), "{out}");
}

#[test]
fn emitted_mysql_dbml_should_have_no_lint_errors() {
    let out = import_sql_ddl(MYSQL, SqlDialect::Mysql).dbml;
    assert_eq!(errors(&out), Vec::<String>::new(), "{out}");
}

#[test]
fn emitted_sqlite_dbml_should_have_no_lint_errors() {
    let out = import_sql_ddl(SQLITE, SqlDialect::Sqlite).dbml;
    assert_eq!(errors(&out), Vec::<String>::new(), "{out}");
}

#[test]
fn import_should_be_deterministic() {
    assert_eq!(
        import_sql_ddl(PG, SqlDialect::Postgres),
        import_sql_ddl(PG, SqlDialect::Postgres)
    );
}

#[test]
fn import_should_return_empty_dbml_for_empty_script() {
    assert_eq!(import_sql_ddl("", SqlDialect::Sqlite).dbml, "");
}

#[test]
fn import_should_drop_ref_to_unknown_table_with_warning() {
    let out = import_sql_ddl(
        "CREATE TABLE a (id int, b_id int REFERENCES missing(id));",
        SqlDialect::Postgres,
    );
    assert_eq!(
        (out.dbml.contains("Ref:"), out.warnings.len()),
        (false, 1),
        "{out:?}"
    );
}

#[test]
fn import_should_drop_index_on_unknown_column_with_warning() {
    let out = import_sql_ddl(
        "CREATE TABLE a (id int); CREATE INDEX i ON a (nope);",
        SqlDialect::Postgres,
    );
    assert_eq!(
        (out.dbml.contains("indexes"), out.warnings.len()),
        (false, 1),
        "{out:?}"
    );
}

#[test]
fn import_should_keep_first_of_duplicate_tables_and_warn() {
    let out = import_sql_ddl(
        "CREATE TABLE a (x int); CREATE TABLE a (y int);",
        SqlDialect::Postgres,
    );
    assert_eq!(
        (
            out.dbml.contains("x integer") || out.dbml.contains("x int"),
            out.warnings.len()
        ),
        (true, 1),
        "{out:?}"
    );
}

#[test]
fn import_should_resolve_implicit_foreign_key_target_to_primary_key() {
    let out = import_sql_ddl(
        "CREATE TABLE a (id int PRIMARY KEY); CREATE TABLE b (a_id int REFERENCES a);",
        SqlDialect::Postgres,
    );
    assert!(out.dbml.contains("Ref: b.a_id > a.id"), "{}", out.dbml);
}

#[test]
fn import_should_emit_one_to_one_for_unique_foreign_key_column() {
    let out = import_sql_ddl(
        "CREATE TABLE a (id int PRIMARY KEY); CREATE TABLE b (a_id int UNIQUE REFERENCES a(id));",
        SqlDialect::Postgres,
    );
    assert!(out.dbml.contains("Ref: b.a_id - a.id"), "{}", out.dbml);
}

#[test]
fn import_should_drop_public_schema_qualifier_for_postgres() {
    let out = import_sql_ddl("CREATE TABLE public.a (id int);", SqlDialect::Postgres);
    assert!(out.dbml.starts_with("Table a {"), "{}", out.dbml);
}

#[test]
fn import_should_keep_non_public_schema_qualifier() {
    let out = import_sql_ddl("CREATE TABLE app.a (id int);", SqlDialect::Postgres);
    assert!(out.dbml.starts_with("Table app.a {"), "{}", out.dbml);
}

#[test]
fn import_should_quote_types_with_spaces_and_names_with_symbols() {
    let out = import_sql_ddl(
        r#"CREATE TABLE "my table" ("a b" bit varying(3));"#,
        SqlDialect::Postgres,
    );
    assert!(
        out.dbml.contains("\"my table\" {") && out.dbml.contains("\"a b\" \"bit varying(3)\""),
        "{}",
        out.dbml
    );
}

#[test]
fn import_should_use_double_quotes_for_strings_containing_apostrophes() {
    let out = import_sql_ddl(
        "CREATE TABLE a (n text DEFAULT 'it''s');",
        SqlDialect::Postgres,
    );
    assert!(out.dbml.contains("default: \"it's\""), "{}", out.dbml);
}

#[test]
fn import_should_synthesize_named_enum_for_mysql_inline_enum() {
    let out = import_sql_ddl(MYSQL, SqlDialect::Mysql).dbml;
    assert!(
        out.contains("Enum users_role {") && out.contains("role users_role"),
        "{out}"
    );
}

#[test]
fn round_trip_should_preserve_tables_columns_refs_and_enums() {
    for (import, export) in ALL {
        let sql = export_sql_ddl(ROUND_TRIP_SAMPLE, export);
        let out = import_sql_ddl(&sql, import);
        let (mut before, mut after) = (analyze(ROUND_TRIP_SAMPLE), analyze(&out.dbml));
        // The exporter orders tables topologically; compare as sets.
        before.tables.sort_by(|a, b| a.name.cmp(&b.name));
        after.tables.sort_by(|a, b| a.name.cmp(&b.name));

        let shape = |s: &schema::Schema| -> Vec<(String, Vec<String>)> {
            s.tables
                .iter()
                .map(|t| {
                    (
                        t.name.clone(),
                        t.columns.iter().map(|c| c.name.clone()).collect(),
                    )
                })
                .collect()
        };
        assert_eq!(
            shape(&before),
            shape(&after),
            "{import}\n{sql}\n{}",
            out.dbml
        );

        let refs = |s: &schema::Schema| -> Vec<(String, String, String, String)> {
            s.refs
                .iter()
                .map(|r| {
                    (
                        r.from.table.clone(),
                        r.from.column.clone(),
                        r.to.table.clone(),
                        r.to.column.clone(),
                    )
                })
                .collect()
        };
        assert_eq!(refs(&before), refs(&after), "{import}\n{}", out.dbml);

        let values = |s: &schema::Schema| -> Vec<Vec<String>> {
            s.enums
                .iter()
                .map(|e| e.values.iter().map(|v| v.name.clone()).collect())
                .collect()
        };
        assert_eq!(
            values(&before),
            values(&after),
            "{import}\n{sql}\n{}",
            out.dbml
        );
    }
}

#[test]
fn round_trip_should_preserve_composite_primary_key() {
    for (import, export) in ALL {
        let out = import_sql_ddl(&export_sql_ddl(ROUND_TRIP_SAMPLE, export), import);
        assert!(
            out.dbml.contains("(org_id, user_id) [pk]"),
            "{import}\n{}",
            out.dbml
        );
    }
}

#[test]
fn round_trip_should_preserve_referential_actions() {
    for (import, export) in ALL {
        let out = import_sql_ddl(&export_sql_ddl(ROUND_TRIP_SAMPLE, export), import);
        // Oracle não tem ON UPDATE: o exportador só deixa um comentário, então só o delete volta
        let expected = if import == SqlDialect::Oracle {
            "[delete: cascade]"
        } else {
            "[delete: cascade, update: no action]"
        };
        assert!(out.dbml.contains(expected), "{import}\n{}", out.dbml);
    }
}

#[test]
fn round_trip_output_should_have_no_lint_errors() {
    for (import, export) in ALL {
        let out = import_sql_ddl(&export_sql_ddl(ROUND_TRIP_SAMPLE, export), import);
        assert_eq!(
            errors(&out.dbml),
            Vec::<String>::new(),
            "{import}\n{}",
            out.dbml
        );
    }
}

mod never_panics {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn import_should_never_panic_and_always_emit_parseable_dbml(s in "\\PC*") {
            for (d, _) in ALL {
                let out = import_sql_ddl(&s, d);
                prop_assert!(syntax_errors(&out.dbml).is_empty(), "{}", out.dbml);
            }
        }

        #[test]
        fn import_should_never_panic_when_fixtures_are_truncated(cut in 0usize..2000) {
            for (sql, d) in [(PG, SqlDialect::Postgres), (MYSQL, SqlDialect::Mysql), (SQLITE, SqlDialect::Sqlite)] {
                let end = sql.char_indices().map(|(i, _)| i).nth(cut).unwrap_or(sql.len());
                let out = import_sql_ddl(sql.get(..end).unwrap_or(""), d);
                prop_assert!(syntax_errors(&out.dbml).is_empty(), "{}", out.dbml);
            }
        }
    }
}
