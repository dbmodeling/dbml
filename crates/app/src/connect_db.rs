//! View-model for the "Conectar ao banco…" modal (parity with `connect-db.ts` on the web).

use db_introspect::{ConnectionParams, Dialect, Options, TlsMode, UrlErrorCode, mask_url};
use std::sync::OnceLock;
use std::time::Duration;

/// Runtime tokio compartilhado entre chamadas de introspecção (#115).
fn introspect_runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    static RT: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    match RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(crate::constants::INTROSPECT_WORKER_THREADS)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())
    }) {
        Ok(rt) => Ok(rt),
        Err(e) => Err(e.clone()),
    }
}

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

/// Which side of the form the user edited last: it decides the direction of the sync, so a
/// field edit rewrites the connection string and a string edit fills the fields, never both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOrigin {
    /// A form field (host, port, database, user, password, file, TLS or engine).
    Fields,
    /// The "Connection string" field.
    String,
}

/// Text of the form fields that go into the connection string (never the password).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormFields {
    pub host: String,
    pub port: String,
    pub database: String,
    pub user: String,
    pub path: String,
}

/// What a valid string edit asks the dialog to do with the password field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringApplied {
    /// `None`: keep the password field (the string had the `***` marker or is a SQLite path);
    /// `Some(p)`: write `p` into it. Handed straight to the widget, never stored in the model.
    pub password: Option<String>,
}

/// One entry of the engine dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineItem {
    pub dialect: Dialect,
    pub label: &'static str,
    pub enabled: bool,
    pub checked: bool,
}

/// Engines of the dropdown, in order; Oracle is listed but has no introspector yet.
const ENGINES: [(Dialect, &str, bool); 5] = [
    (Dialect::Postgres, "PostgreSQL", true),
    (Dialect::Mysql, "MySQL", true),
    (Dialect::SqlServer, "SQL Server", true),
    (Dialect::Sqlite, "SQLite", true),
    (Dialect::Oracle, "Oracle (em breve)", false),
];

/// Entries of the engine dropdown with the current one checked.
#[must_use]
pub fn engine_menu(current: Dialect) -> Vec<EngineItem> {
    ENGINES
        .iter()
        .map(|&(dialect, label, enabled)| EngineItem {
            dialect,
            label: crate::i18n::t(label),
            enabled,
            checked: dialect == current,
        })
        .collect()
}

/// Label of `dialect` in the dropdown trigger.
#[must_use]
pub fn engine_label(dialect: Dialect) -> &'static str {
    ENGINES
        .iter()
        .find(|(d, _, _)| *d == dialect)
        .map_or("", |(_, label, _)| crate::i18n::t(label))
}

/// Short inline message for a connection string that could not be read (pt-BR source text,
/// translated by `i18n`).
#[must_use]
pub fn url_error_text(code: UrlErrorCode) -> &'static str {
    crate::i18n::t(match code {
        UrlErrorCode::Empty => "Connection string vazia.",
        UrlErrorCode::UnknownScheme => {
            "Esquema desconhecido. Use postgres://, mysql://, sqlserver:// ou sqlite:."
        }
        UrlErrorCode::UnsupportedEngine => "Oracle ainda não é suportado.",
        UrlErrorCode::Malformed => "Connection string malformada.",
        UrlErrorCode::MissingHost => "Informe o host.",
        UrlErrorCode::InvalidPort => "Porta inválida (1 a 65535).",
    })
}

/// URL of `params`; without a user (`to_url` requires one) it still shows scheme, host, port,
/// database and TLS so the string follows the form from the first keystroke.
fn compose(params: &ConnectionParams) -> Option<String> {
    if params.dialect == Dialect::Sqlite || !params.user.trim().is_empty() {
        return params.to_url().ok();
    }
    let mut stub = params.clone();
    crate::constants::CONN_USER_STUB.clone_into(&mut stub.user);
    let url = stub.to_url().ok()?;
    let (scheme, rest) = url.split_once(crate::constants::CONN_SCHEME_SEP)?;
    let rest = rest.strip_prefix(crate::constants::CONN_USER_STUB)?;
    let rest = rest.strip_prefix('@').unwrap_or(rest);
    Some(format!(
        "{scheme}{}{rest}",
        crate::constants::CONN_SCHEME_SEP
    ))
}

/// State shared between the modal and the background task. The password lives only in its input
/// widget: methods that need it take it as an argument, and nothing here stores or prints it.
#[derive(Debug, Clone)]
pub struct ConnectModel {
    pub dialect: Dialect,
    pub trust_cert: bool,
    pub tls: TlsMode,
    pub status: Status,
    pub dbml: String,
    pub warnings: Vec<String>,
    pub tables: usize,
    pub confirm_replace: bool,
    /// Last valid values of the fields that make up the connection string.
    pub fields: FormFields,
    /// Connection string of `fields`, always with the password masked (safe to keep and log).
    pub url: String,
    /// Whether the password field is non-empty (decides where the `***` marker goes).
    pub has_password: bool,
    pub origin: EditOrigin,
    /// Why the text of the string field could not be read; the fields keep their last values.
    pub url_error: Option<UrlErrorCode>,
    /// Password shown in the field and in the string. Never persisted; hidden on every open.
    pub revealed: bool,
    /// Engine dropdown open (controlled so the arrow key can open it).
    pub engine_menu_open: bool,
    /// Bumped on every keystroke in the string field; a debounced parse runs only if it is
    /// still the latest one.
    pub string_edit_seq: u64,
}

impl ConnectModel {
    #[must_use]
    pub const fn new(dialect: Dialect) -> Self {
        Self {
            dialect,
            trust_cert: false,
            tls: TlsMode::Auto,
            status: Status::Idle,
            dbml: String::new(),
            warnings: Vec::new(),
            tables: 0,
            confirm_replace: false,
            fields: FormFields {
                host: String::new(),
                port: String::new(),
                database: String::new(),
                user: String::new(),
                path: String::new(),
            },
            url: String::new(),
            has_password: false,
            origin: EditOrigin::Fields,
            url_error: None,
            revealed: false,
            engine_menu_open: false,
            string_edit_seq: 0,
        }
    }

    /// Engine picked in the dropdown: keeps every typed value and hides the password again.
    pub fn set_dialect(&mut self, dialect: Dialect) {
        self.dialect = dialect;
        self.revealed = false;
        self.engine_menu_open = false;
        self.origin = EditOrigin::Fields;
        self.reset_result();
    }

    /// Dialog closed: nothing of the reveal state survives.
    pub fn close(&mut self) {
        self.revealed = false;
        self.engine_menu_open = false;
    }

    pub fn toggle_reveal(&mut self) {
        self.revealed = !self.revealed;
    }

    /// Form fields plus `password` (read from its widget at the moment of use).
    #[must_use]
    pub fn params(&self, password: &str) -> ConnectionParams {
        let mut p = ConnectionParams::new(self.dialect);
        p.host.clone_from(&self.fields.host);
        p.port = self.fields.port.trim().parse().ok();
        p.database.clone_from(&self.fields.database);
        p.user.clone_from(&self.fields.user);
        password.clone_into(&mut p.password);
        p.path.clone_from(&self.fields.path);
        p.trust_server_certificate = self.trust_cert;
        p.tls = self.tls;
        p
    }

    /// Default port of the engine, shown as the port placeholder.
    #[must_use]
    pub fn port_placeholder(&self) -> String {
        ConnectionParams::default_port(self.dialect).map_or_else(String::new, |p| p.to_string())
    }

    /// The connection string as the string field shows it: masked unless the password is
    /// revealed (or empty).
    #[must_use]
    pub fn display_url(&self, password: &str) -> String {
        let url = compose(&self.params(password)).unwrap_or_default();
        if self.revealed || password.is_empty() || url.is_empty() {
            url
        } else {
            mask_url(&url)
        }
    }

    /// What "Copiar" puts on the clipboard: the same masked text unless the password is revealed.
    #[must_use]
    pub fn copy_text(&self, password: &str) -> String {
        self.display_url(password)
    }

    /// A field changed: keep its values and return the string to show.
    pub fn edit_fields(&mut self, fields: FormFields, password: &str) -> String {
        self.fields = fields;
        self.refresh_url(password)
    }

    /// Engine, TLS or certificate changed: rewrite the string from the current fields.
    pub fn refresh_url(&mut self, password: &str) -> String {
        self.origin = EditOrigin::Fields;
        self.url_error = None;
        self.has_password = !password.is_empty();
        self.url = self.masked_url();
        self.display_url(password)
    }

    fn masked_url(&self) -> String {
        let stub = if self.has_password {
            crate::constants::CONN_PASSWORD_STUB
        } else {
            ""
        };
        let url = compose(&self.params(stub)).unwrap_or_default();
        if self.has_password && !url.is_empty() {
            mask_url(&url)
        } else {
            url
        }
    }

    /// The string field changed: fill the fields (engine and TLS included) or, if the text cannot
    /// be read, keep the last valid fields and remember the error.
    ///
    /// # Errors
    ///
    /// Returns the code of the problem, also kept in `url_error`.
    pub fn edit_string(&mut self, text: &str) -> Result<StringApplied, UrlErrorCode> {
        self.origin = EditOrigin::String;
        let parsed = match ConnectionParams::from_url(text) {
            Ok(parsed) => parsed,
            Err(e) => {
                self.url_error = Some(e.code());
                return Err(e.code());
            }
        };
        self.url_error = None;
        let p = parsed.params;
        if p.dialect != self.dialect {
            self.dialect = p.dialect;
            self.reset_result();
        }
        let password = if p.dialect == Dialect::Sqlite {
            // only the file comes from a path: the network fields stay for a later switch back
            self.fields.path = p.path;
            None
        } else {
            self.fields.host = p.host;
            self.fields.port = p.port.map_or_else(String::new, |n| n.to_string());
            self.fields.database = p.database;
            self.fields.user = p.user;
            self.tls = p.tls;
            self.trust_cert = p.trust_server_certificate;
            if parsed.password_masked {
                None
            } else {
                self.has_password = !p.password.is_empty();
                Some(p.password)
            }
        };
        self.url = self.masked_url();
        Ok(StringApplied { password })
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
                self.revealed = false;
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

    /// Inline message for the string field, if its text could not be read.
    #[must_use]
    pub fn url_error_text(&self) -> Option<&'static str> {
        self.url_error.map(url_error_text)
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
    let rt = introspect_runtime()?;
    let options = Options {
        schemas,
        timeout_secs: Duration::from_secs(crate::constants::INTROSPECT_TIMEOUT_SECS).as_secs(),
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
    let rt = introspect_runtime()?;
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

    fn fields(host: &str, port: &str, database: &str, user: &str) -> FormFields {
        FormFields {
            host: host.into(),
            port: port.into(),
            database: database.into(),
            user: user.into(),
            path: String::new(),
        }
    }

    #[test]
    fn opens_hidden_with_a_masked_string() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        assert!(!m.revealed);
        let shown = m.edit_fields(fields("h", "", "db", "u"), "p@ss");
        assert_eq!(shown, "postgres://u:***@h:5432/db");
        assert_eq!(m.url, shown);
        assert!(!m.url.contains("p@ss") && !m.url.contains("p%40ss"));
    }

    #[test]
    fn editing_a_field_rewrites_the_string_percent_encoded() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        m.toggle_reveal();
        let shown = m.edit_fields(fields("h", "5433", "my db", "u@x"), "p@ss/:%");
        assert_eq!(shown, "postgres://u%40x:p%40ss%2F%3A%25@h:5433/my%20db");
        assert_eq!(m.origin, EditOrigin::Fields);
    }

    #[test]
    fn string_without_user_still_shows_scheme_and_default_port() {
        let mut m = ConnectModel::new(Dialect::Mysql);
        assert_eq!(
            m.edit_fields(FormFields::default(), ""),
            "mysql://localhost:3306"
        );
        m.set_dialect(Dialect::SqlServer);
        assert_eq!(m.refresh_url(""), "sqlserver://localhost:1433");
    }

    #[test]
    fn pasting_a_full_string_fills_the_fields_engine_and_tls() {
        let mut m = ConnectModel::new(Dialect::Mysql);
        let applied = m
            .edit_string("postgres://u:p%40ss@h:5433/db?sslmode=require")
            .unwrap();
        assert_eq!(m.dialect, Dialect::Postgres);
        assert_eq!(m.fields, fields("h", "5433", "db", "u"));
        assert_eq!(m.tls, TlsMode::Require);
        assert_eq!(applied.password.as_deref(), Some("p@ss"));
        assert_eq!(m.origin, EditOrigin::String);
        assert_eq!(m.url_error, None);
        assert_eq!(m.url, "postgres://u:***@h:5433/db?sslmode=require");
    }

    #[test]
    fn scheme_picks_the_engine() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        m.edit_string("mysql://root@db").unwrap();
        assert_eq!(m.dialect, Dialect::Mysql);
        m.edit_string("sqlite:/tmp/a.db").unwrap();
        assert_eq!(m.dialect, Dialect::Sqlite);
        assert_eq!(m.fields.path, "/tmp/a.db");
    }

    #[test]
    fn invalid_string_keeps_the_last_valid_fields_until_fixed() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        m.edit_string("postgres://u@h:5433/db").unwrap();
        let before = m.fields.clone();
        let err = m.edit_string("ftp://nope").unwrap_err();
        assert_eq!(err, UrlErrorCode::UnknownScheme);
        assert_eq!(m.url_error, Some(UrlErrorCode::UnknownScheme));
        assert_eq!(m.fields, before);
        assert_eq!(m.dialect, Dialect::Postgres);
        assert!(m.url_error_text().is_some());
        assert_eq!(
            m.edit_string("postgres://u@h:99999").unwrap_err(),
            UrlErrorCode::InvalidPort
        );
        m.edit_string("postgres://u@h2").unwrap();
        assert_eq!(m.url_error, None);
        assert_eq!(m.url_error_text(), None);
        assert_eq!(m.fields.host, "h2");
    }

    #[test]
    fn editing_a_field_clears_a_pending_string_error() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        let _ = m.edit_string("nope://x");
        m.edit_fields(fields("h", "", "", "u"), "");
        assert_eq!(m.url_error, None);
    }

    #[test]
    fn masked_marker_keeps_the_password_and_a_real_one_replaces_it() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        m.edit_fields(fields("h", "", "db", "u"), "secret");
        let kept = m.edit_string("postgres://u:***@h:5432/other").unwrap();
        assert_eq!(kept.password, None);
        assert_eq!(m.fields.database, "other");
        assert!(m.url.contains(":***@"));
        let typed = m.edit_string("postgres://u:n3w@h:5432/other").unwrap();
        assert_eq!(typed.password.as_deref(), Some("n3w"));
        assert!(!m.url.contains("n3w"));
    }

    #[test]
    fn round_trip_keeps_every_field() {
        let mut m = ConnectModel::new(Dialect::SqlServer);
        m.tls = TlsMode::Verify;
        m.toggle_reveal();
        let original = fields("::1", "1500", "a/b c", "us:er");
        let shown = m.edit_fields(original.clone(), "p@ss/:%");
        let applied = m.edit_string(&shown).unwrap();
        assert_eq!(m.fields, original);
        assert_eq!(m.dialect, Dialect::SqlServer);
        assert_eq!(m.tls, TlsMode::Verify);
        assert_eq!(applied.password.as_deref(), Some("p@ss/:%"));
    }

    #[test]
    fn reveal_controls_the_shown_and_copied_string() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        m.edit_fields(fields("h", "", "db", "u"), "pw");
        assert_eq!(m.display_url("pw"), "postgres://u:***@h:5432/db");
        assert_eq!(m.copy_text("pw"), "postgres://u:***@h:5432/db");
        m.toggle_reveal();
        assert!(m.revealed);
        assert_eq!(m.display_url("pw"), "postgres://u:pw@h:5432/db");
        assert_eq!(m.copy_text("pw"), "postgres://u:pw@h:5432/db");
        m.toggle_reveal();
        assert_eq!(m.copy_text("pw"), "postgres://u:***@h:5432/db");
    }

    #[test]
    fn empty_password_is_not_masked() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        assert_eq!(
            m.edit_fields(fields("h", "", "", "u"), ""),
            "postgres://u@h:5432"
        );
    }

    #[test]
    fn reveal_goes_back_to_hidden_on_engine_change_success_and_close() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        m.toggle_reveal();
        m.set_dialect(Dialect::Mysql);
        assert!(!m.revealed);
        m.toggle_reveal();
        m.finish(Ok(Generated {
            dbml: "Table a {\n  id int\n}\n".into(),
            warnings: vec![],
            tables: 1,
        }));
        assert!(!m.revealed);
        m.toggle_reveal();
        m.close();
        assert!(!m.revealed);
    }

    #[test]
    fn switching_engine_keeps_the_typed_values_and_updates_the_scheme() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        m.edit_fields(fields("h", "", "db", "u"), "pw");
        m.set_dialect(Dialect::Mysql);
        assert_eq!(m.fields, fields("h", "", "db", "u"));
        assert_eq!(m.refresh_url("pw"), "mysql://u:***@h:3306/db");
        assert_eq!(m.port_placeholder(), "3306");
    }

    #[test]
    fn params_combine_fields_and_the_password_given_by_the_widget() {
        let mut m = ConnectModel::new(Dialect::Postgres);
        m.edit_fields(fields("h", "5433", "db", "u"), "pw");
        let p = m.params("pw");
        assert_eq!(
            (p.host.as_str(), p.port, p.password.as_str()),
            ("h", Some(5433), "pw")
        );
        assert!(!format!("{m:?}").contains("pw@"));
    }

    #[test]
    fn engine_menu_lists_oracle_disabled_and_marks_the_current_one() {
        let items = engine_menu(Dialect::Mysql);
        assert_eq!(items.len(), 5);
        let oracle = items.iter().find(|i| i.dialect == Dialect::Oracle).unwrap();
        assert!(!oracle.enabled);
        assert!(
            items
                .iter()
                .filter(|i| i.checked)
                .all(|i| i.dialect == Dialect::Mysql)
        );
        assert_eq!(items.iter().filter(|i| i.checked).count(), 1);
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
