#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        for dialect in [
            sql_import::SqlDialect::Postgres,
            sql_import::SqlDialect::Mysql,
            sql_import::SqlDialect::Sqlite,
            sql_import::SqlDialect::SqlServer,
            sql_import::SqlDialect::Oracle,
        ] {
            let _ = sql_import::import_sql_ddl(text, dialect);
        }
    }
});
