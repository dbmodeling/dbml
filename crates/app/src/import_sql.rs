//! View-model state for the "Import SQL DDL…" modal (1:1 parity with `import-sql.ts`).

use sql_import::{ImportWarning, SqlDialect, import_sql_ddl};

/// Guard limit matching the web modal (5 MB).
pub const MAX_IMPORT_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ImportSqlModel {
    pub dialect: SqlDialect,
    pub sql: String,
    pub dbml: String,
    pub warnings: Vec<ImportWarning>,
    pub too_large: bool,
    pub confirm_replace: bool,
}

impl ImportSqlModel {
    #[must_use]
    pub fn new(dialect: SqlDialect) -> Self {
        Self {
            dialect,
            sql: String::new(),
            dbml: String::new(),
            warnings: Vec::new(),
            too_large: false,
            confirm_replace: false,
        }
    }

    pub fn set_dialect(&mut self, dialect: SqlDialect) {
        self.dialect = dialect;
        self.recompute();
    }

    pub fn set_sql(&mut self, sql: impl Into<String>) {
        self.sql = sql.into();
        self.confirm_replace = false;
        self.recompute();
    }

    pub fn recompute(&mut self) {
        if self.sql.len() > MAX_IMPORT_BYTES {
            self.too_large = true;
            self.dbml.clear();
            self.warnings.clear();
            return;
        }
        self.too_large = false;
        if self.sql.trim().is_empty() {
            self.dbml.clear();
            self.warnings.clear();
            return;
        }
        let outcome = import_sql_ddl(&self.sql, self.dialect);
        self.dbml = outcome.dbml;
        self.warnings = outcome.warnings;
    }

    /// (tabelas, relações) da prévia, para o resumo do rodapé.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        let n = |prefix: &str| self.dbml.lines().filter(|l| l.starts_with(prefix)).count();
        (n("Table "), n("Ref"))
    }

    #[must_use]
    pub fn can_apply(&self) -> bool {
        !self.too_large && !self.sql.trim().is_empty() && !self.dbml.trim().is_empty()
    }

    #[must_use]
    pub fn build_merged_source(&self, existing: &str, replace: bool) -> String {
        if replace || existing.trim().is_empty() {
            return self.dbml.clone();
        }
        let trimmed = existing.trim_end();
        format!("{trimmed}\n\n{}", self.dbml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_model_recomputes_preview_on_dialect_change() {
        let mut model = ImportSqlModel::new(SqlDialect::Postgres);
        model.set_sql("CREATE TABLE users (id serial PRIMARY KEY);");
        assert!(model.dbml.contains("id integer [pk, increment]"));

        model.set_dialect(SqlDialect::Mysql);
        assert!(model.dbml.contains("id serial [pk]"));
    }

    #[test]
    fn import_model_counts_tables_and_refs() {
        let mut model = ImportSqlModel::new(SqlDialect::Postgres);
        model.set_sql(
            "CREATE TABLE a (id int PRIMARY KEY);\n\
             CREATE TABLE b (id int PRIMARY KEY, a_id int REFERENCES a(id));",
        );
        assert_eq!(model.counts(), (2, 1));
    }

    #[test]
    fn import_model_disables_apply_for_empty_or_huge_input() {
        let mut model = ImportSqlModel::new(SqlDialect::Sqlite);
        assert!(!model.can_apply());

        model.set_sql("   \n ");
        assert!(!model.can_apply());

        model.set_sql("a".repeat(MAX_IMPORT_BYTES + 1));
        assert!(model.too_large);
        assert!(!model.can_apply());
    }

    #[test]
    fn import_model_merges_insert_and_replace() {
        let mut model = ImportSqlModel::new(SqlDialect::Postgres);
        model.set_sql("CREATE TABLE b (id int);");
        let existing = "Table a {\n  id int\n}\n";

        assert_eq!(
            model.build_merged_source(existing, false),
            "Table a {\n  id int\n}\n\nTable b {\n  id int\n}\n"
        );
        assert_eq!(
            model.build_merged_source(existing, true),
            "Table b {\n  id int\n}\n"
        );
    }
}
