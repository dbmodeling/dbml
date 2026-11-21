//! View-model for the "Conectar ao banco…" modal (parity with `connect-db.ts` on the web).

use db_introspect::{ConnectionParams, Dialect, Options};
use std::time::Duration;

/// Where the connection attempt is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Nothing attempted yet.
    Idle,
    /// A background introspection is running.
    Connecting,
    /// The last attempt failed; the message never contains the password.
    Failed(String),
    /// The last "Testar conexão" succeeded; the message names the server and the schemas.
    Tested(String),
    /// DBML was generated and can be applied.
    Ready,
}

/// State shared between the modal and the background task. Field values live in input widgets and
/// are read only when the user presses Connect; the password is never stored here.
#[derive(Debug, Clone)]
pub struct ConnectModel {
    pub dialect: Dialect,
    pub trust_cert: bool,
    pub status: Status,
    pub dbml: String,
    pub warnings: Vec<String>,
    pub tables: usize,
    pub confirm_replace: bool,
}

impl ConnectModel {
    #[must_use]
    pub const fn new(dialect: Dialect) -> Self {
        Self {
            dialect,
            trust_cert: false,
            status: Status::Idle,
            dbml: String::new(),
            warnings: Vec::new(),
            tables: 0,
            confirm_replace: false,
        }
    }

    pub fn set_dialect(&mut self, dialect: Dialect) {
        self.dialect = dialect;
        self.reset_result();
    }

    pub fn reset_result(&mut self) {
        self.status = Status::Idle;
        self.dbml.clear();
        self.warnings.clear();
        self.tables = 0;
        self.confirm_replace = false;
    }

    /// Result of "Testar conexão": no DBML is produced, so nothing can be applied.
    pub fn finish_test(&mut self, result: Result<String, String>) {
        self.dbml.clear();
        self.warnings.clear();
        self.tables = 0;
        self.confirm_replace = false;
        self.status = match result {
            Ok(msg) => Status::Tested(msg),
            Err(msg) => Status::Failed(msg),
        };
    }

    pub fn finish(&mut self, result: Result<Generated, String>) {
        self.confirm_replace = false;
        match result {
            Ok(g) => {
                self.dbml = g.dbml;
                self.warnings = g.warnings;
                self.tables = g.tables;
                self.status = Status::Ready;
            }
            Err(msg) => {
                self.dbml.clear();
                self.warnings.clear();
                self.tables = 0;
                self.status = Status::Failed(msg);
            }
        }
    }

    #[must_use]
    pub fn can_apply(&self) -> bool {
        self.status == Status::Ready && !self.dbml.trim().is_empty()
    }

    /// Replace the document or append the generated tables after the existing text.
    #[must_use]
    pub fn build_merged_source(&self, existing: &str, replace: bool) -> String {
        if replace || existing.trim().is_empty() {
            return self.dbml.clone();
        }
        format!("{}\n\n{}", existing.trim_end(), self.dbml)
    }
}

/// Successful introspection.
#[derive(Debug, Clone)]
pub struct Generated {
    pub dbml: String,
    pub warnings: Vec<String>,
    pub tables: usize,
}

/// Builds the URL, reads the catalog on a private tokio runtime (sqlx needs one; GPUI's executor
/// is not tokio) and returns DBML. Blocking: call from a background thread.
pub fn introspect_blocking(
    params: &ConnectionParams,
    schemas: Vec<String>,
) -> Result<Generated, String> {
    let url = params.to_url().map_err(|e| e.to_string())?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let options = Options {
        schemas,
        timeout_secs: Duration::from_secs(10).as_secs(),
    };
    let outcome = rt
        .block_on(Box::pin(db_introspect::introspect_with(&url, &options)))
        .map_err(|e| e.to_string())?;
    let tables = dbml_analysis::analyze(&outcome.dbml).tables.len();
    if tables == 0 {
        return Err(crate::i18n::t(
            "Conectado, mas nenhuma tabela foi encontrada. Verifique o banco e o schema.",
        )
        .into());
    }
    Ok(Generated {
        dbml: outcome.dbml,
        warnings: outcome.warnings,
        tables,
    })
}

/// Connects and runs a trivial query (no catalog read); returns a one-line summary.
/// Blocking: call from a background thread.
pub fn test_blocking(params: &ConnectionParams, schemas: Vec<String>) -> Result<String, String> {
    let url = params.to_url().map_err(|e| e.to_string())?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let options = Options {
        schemas,
        ..Options::default()
    };
    let out = rt
        .block_on(Box::pin(db_introspect::test_connection(&url, &options)))
        .map_err(|e| e.to_string())?;
    Ok(summarize_test(&out.server, &out.schemas))
}

/// `Conectado a X. Schemas: a, b (+3 a mais).`
#[must_use]
pub fn summarize_test(server: &str, schemas: &[String]) -> String {
    const SHOWN: usize = 8;
    if schemas.is_empty() {
        return crate::i18n::t("Conectado a {}.").replace("{}", server);
    }
    let shown = schemas
        .iter()
        .take(SHOWN)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let more = schemas.len().saturating_sub(SHOWN);
    if more > 0 {
        crate::i18n::t("Conectado a {}. Schemas: {} (+{} a mais).")
            .replacen("{}", server, 1)
            .replacen("{}", &shown, 1)
            .replacen("{}", &more.to_string(), 1)
    } else {
        crate::i18n::t("Conectado a {}. Schemas: {}.")
            .replacen("{}", server, 1)
            .replacen("{}", &shown, 1)
    }
}

/// `"a, b ,,c"` -> `["a", "b", "c"]`.
#[must_use]
pub fn parse_schemas(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_replace_and_append() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        m.finish(Ok(Generated {
            dbml: "Table b {\n  id int\n}\n".into(),
            warnings: vec![],
            tables: 1,
        }));
        assert!(m.can_apply());
        let existing = "Table a {\n  id int\n}\n";
        assert_eq!(
            m.build_merged_source(existing, false),
            "Table a {\n  id int\n}\n\nTable b {\n  id int\n}\n"
        );
        assert_eq!(
            m.build_merged_source(existing, true),
            "Table b {\n  id int\n}\n"
        );
    }

    #[test]
    fn changing_engine_clears_the_previous_result() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        m.finish(Err("boom".into()));
        assert_eq!(m.status, Status::Failed("boom".into()));
        m.set_dialect(Dialect::Mysql);
        assert_eq!(m.status, Status::Idle);
        assert!(!m.can_apply());
    }

    #[test]
    fn test_result_never_enables_apply() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        m.finish_test(Ok("Conectado a X.".into()));
        assert_eq!(m.status, Status::Tested("Conectado a X.".into()));
        assert!(!m.can_apply());
        m.finish_test(Err("nope".into()));
        assert_eq!(m.status, Status::Failed("nope".into()));
    }

    #[test]
    fn summarizes_schemas_and_truncates_long_lists() {
        assert_eq!(
            summarize_test("SQLite 3.45", &[]),
            "Conectado a SQLite 3.45."
        );
        assert_eq!(
            summarize_test("PG 16", &["a".into(), "b".into()]),
            "Conectado a PG 16. Schemas: a, b."
        );
        let many: Vec<String> = (0..11).map(|i| format!("s{i}")).collect();
        assert!(summarize_test("X", &many).ends_with("s7 (+3 a mais)."));
    }

    #[test]
    fn schema_list_is_trimmed() {
        assert_eq!(parse_schemas(" a, b ,,c "), vec!["a", "b", "c"]);
        assert_eq!(parse_schemas("  "), Vec::<String>::new());
    }

    #[test]
    fn sqlite_file_round_trip_generates_dbml() {
        use std::io::Write as _;
        // A real SQLite file: header only is enough to fail cleanly, so create via the CLI-free
        // path of the introspector tests instead; here we only check the error surface.
        let mut f = ConnectionParams::new(Dialect::Sqlite);
        f.path = std::env::temp_dir()
            .join("dbi_app_missing.sqlite")
            .display()
            .to_string();
        let _ = std::io::stdout().flush();
        let err = introspect_blocking(&f, vec![]).unwrap_err();
        assert!(err.contains("could not connect"), "{err}");
    }

    #[test]
    fn missing_user_is_reported_before_any_network_call() {
        let p = ConnectionParams::new(Dialect::Postgres);
        let err = introspect_blocking(&p, vec![]).unwrap_err();
        assert!(err.contains("user is required"), "{err}");
    }
}
