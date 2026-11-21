//! Cadeias de FKs muito profundas não podem depender de recursão (a pilha do Wasm é pequena).

use dbml_analysis::{SqlDialect as ExportDialect, analyze, export_sql_ddl, lint};
use sql_import::{SqlDialect, import_sql_ddl};
use std::fmt::Write as _;

const TABLES: usize = 20_000;

fn chain_sql() -> String {
    let mut sql = String::from("CREATE TABLE t0 (id int PRIMARY KEY);\n");
    for i in 1..TABLES {
        let _ = writeln!(
            sql,
            "CREATE TABLE t{i} (id int PRIMARY KEY, p int REFERENCES t{}(id));",
            i - 1
        );
    }
    sql
}

#[test]
fn import_analyze_lint_and_export_survive_20000_table_chain_on_small_stack() {
    let (tables, ddl_len) = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let outcome = import_sql_ddl(&chain_sql(), SqlDialect::Postgres);
            let schema = analyze(&outcome.dbml);
            let _ = lint(&outcome.dbml);
            let ddl = export_sql_ddl(&outcome.dbml, ExportDialect::Postgres);
            (schema.tables.len(), ddl.len())
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(tables, TABLES);
    assert!(ddl_len > 0);
}
