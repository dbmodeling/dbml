//! Connection form fields to a connection URL (shared by Desktop, Web and CLI front ends).

use crate::constants::{
    MYSQL_TLS_DISABLE, MYSQL_TLS_REQUIRE, MYSQL_TLS_VERIFY, MYSQL_TLS_VERIFY_CA, PARAM_ENCRYPT,
    PARAM_SSL_MODE, PARAM_SSLMODE, PARAM_TRUST_SERVER_CERT_LOWER, PASSWORD_MASK, PG_TLS_DISABLE,
    PG_TLS_REQUIRE, PG_TLS_VERIFY, PG_TLS_VERIFY_CA, SQLSERVER_TRUTHY, URL_ERR_EMPTY,
    URL_ERR_INVALID_PORT, URL_ERR_MALFORMED, URL_ERR_MISSING_HOST, URL_ERR_UNKNOWN_SCHEME,
    URL_ERR_UNSUPPORTED_ENGINE,
};
use crate::{Dialect, Error, TlsMode, detect_dialect, is_loopback_host};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use std::borrow::Cow;
use std::num::NonZeroU16;

/// Everything except RFC 3986 unreserved characters is percent-encoded.
const ENCODE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Stable classification of a [`ParseUrlError`]; front ends translate it by [`UrlErrorCode::key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlErrorCode {
    /// Nothing to parse (blank text, or `sqlite:` without a path).
    Empty,
    /// The scheme is not postgres, postgresql, mysql, mariadb, sqlserver, mssql or sqlite.
    UnknownScheme,
    /// The engine is recognised but has no connection form yet (Oracle).
    UnsupportedEngine,
    /// The text is not a well formed connection URL.
    Malformed,
    /// A network URL without a host.
    MissingHost,
    /// The port is not a number in `1..=65535`.
    InvalidPort,
}

impl UrlErrorCode {
    /// Stable key (`snake_case`, never changes) for translation tables and JSON.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Empty => URL_ERR_EMPTY,
            Self::UnknownScheme => URL_ERR_UNKNOWN_SCHEME,
            Self::UnsupportedEngine => URL_ERR_UNSUPPORTED_ENGINE,
            Self::Malformed => URL_ERR_MALFORMED,
            Self::MissingHost => URL_ERR_MISSING_HOST,
            Self::InvalidPort => URL_ERR_INVALID_PORT,
        }
    }
}

/// Why [`ConnectionParams::from_url`] failed. The message is stable English and never contains
/// the password; convert it with `Error::from` to get [`Error::InvalidTarget`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ParseUrlError {
    code: UrlErrorCode,
    message: String,
}

impl ParseUrlError {
    fn new(code: UrlErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Stable code to translate by key.
    #[must_use]
    pub const fn code(&self) -> UrlErrorCode {
        self.code
    }

    /// Stable English message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<ParseUrlError> for Error {
    fn from(e: ParseUrlError) -> Self {
        Self::InvalidTarget(e.message)
    }
}

/// Result of [`ConnectionParams::from_url`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUrl {
    /// Form fields read from the URL.
    pub params: ConnectionParams,
    /// The URL carried the [`PASSWORD_MASK`] placeholder: `params.password` is empty and the form
    /// must keep the password it already has.
    pub password_masked: bool,
}

/// Fields of a "connect to database" form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionParams {
    /// Engine.
    pub dialect: Dialect,
    /// Host name or address (ignored for SQLite). Empty means `localhost`.
    pub host: String,
    /// TCP port; `None` uses the engine default.
    pub port: Option<u16>,
    /// Database name (ignored for SQLite).
    pub database: String,
    /// User name (ignored for SQLite).
    pub user: String,
    /// Password (ignored for SQLite). Never stored by this crate.
    pub password: String,
    /// SQLite database file.
    pub path: String,
    /// SQL Server only: accept a self-signed server certificate (same as [`TlsMode::Require`]).
    pub trust_server_certificate: bool,
    /// Transport protection. [`TlsMode::Auto`] writes nothing: remote hosts then get verified
    /// TLS from the introspection entry points; set [`TlsMode::Disable`] to opt out explicitly.
    pub tls: TlsMode,
}

impl ConnectionParams {
    /// Empty form for `dialect`.
    #[must_use]
    pub const fn new(dialect: Dialect) -> Self {
        Self {
            dialect,
            host: String::new(),
            port: None,
            database: String::new(),
            user: String::new(),
            password: String::new(),
            path: String::new(),
            trust_server_certificate: false,
            tls: TlsMode::Auto,
        }
    }

    /// Default TCP port of `dialect` (`None` for SQLite).
    #[must_use]
    pub const fn default_port(dialect: Dialect) -> Option<u16> {
        match dialect {
            Dialect::Postgres => Some(5432),
            Dialect::Mysql => Some(3306),
            Dialect::SqlServer => Some(1433),
            Dialect::Oracle => Some(1521),
            Dialect::Sqlite => None,
        }
    }

    /// Query string that spells out [`ConnectionParams::tls`] for the engine, if any.
    fn tls_query(&self) -> Option<&'static str> {
        match (self.dialect, self.tls) {
            (Dialect::Postgres, TlsMode::Verify) => Some("sslmode=verify-full"),
            (Dialect::Postgres, TlsMode::Require) => Some("sslmode=require"),
            (Dialect::Postgres, TlsMode::Disable) => Some("sslmode=disable"),
            (Dialect::Mysql, TlsMode::Verify) => Some("ssl-mode=VERIFY_IDENTITY"),
            (Dialect::Mysql, TlsMode::Require) => Some("ssl-mode=REQUIRED"),
            (Dialect::Mysql, TlsMode::Disable) => Some("ssl-mode=DISABLED"),
            (Dialect::SqlServer, TlsMode::Verify) => Some("encrypt=true"),
            (Dialect::SqlServer, TlsMode::Require) => {
                Some("encrypt=true&trustServerCertificate=true")
            }
            (Dialect::SqlServer, TlsMode::Disable) => Some("encrypt=false"),
            (Dialect::SqlServer, TlsMode::Auto) if self.trust_server_certificate => {
                Some("trustServerCertificate=true")
            }
            _ => None,
        }
    }

    /// Whether the form targets a host that is not this machine (so TLS matters).
    #[must_use]
    pub fn is_remote(&self) -> bool {
        self.dialect != Dialect::Sqlite && !is_loopback_host(&self.host)
    }

    /// Builds the connection URL, percent-encoding user and password.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidTarget`] when a required field is empty or the engine has no
    /// introspector yet.
    pub fn to_url(&self) -> Result<String, Error> {
        let scheme = match self.dialect {
            Dialect::Sqlite => {
                let p = self.path.trim();
                if p.is_empty() {
                    return Err(Error::InvalidTarget("choose a SQLite database file".into()));
                }
                return Ok(p.to_owned());
            }
            Dialect::Postgres => "postgres",
            Dialect::Mysql => "mysql",
            Dialect::SqlServer => "sqlserver",
            Dialect::Oracle => return Err(Error::Unsupported(Dialect::Oracle)),
        };
        let host = match self.host.trim() {
            "" => "localhost",
            h => h,
        };
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
        let port = self.port.or_else(|| Self::default_port(self.dialect));
        let user = self.user.trim();
        if user.is_empty() {
            return Err(Error::InvalidTarget("user is required".into()));
        }
        let mut url = format!("{scheme}://{}", utf8_percent_encode(user, ENCODE));
        if !self.password.is_empty() {
            url.push(':');
            url.push_str(&utf8_percent_encode(&self.password, ENCODE).to_string());
        }
        url.push('@');
        url.push_str(&host);
        if let Some(p) = port {
            url.push(':');
            url.push_str(&p.to_string());
        }
        let db = self.database.trim();
        if !db.is_empty() {
            url.push('/');
            url.push_str(&utf8_percent_encode(db, ENCODE).to_string());
        }
        if let Some(query) = self.tls_query() {
            url.push('?');
            url.push_str(query);
        }
        Ok(url)
    }

    /// Reads a connection string back into form fields (inverse of [`Self::to_url`]).
    ///
    /// Accepts `postgres`/`postgresql`, `mysql`/`mariadb` and `sqlserver`/`mssql` URLs
    /// (`scheme://user:password@host:port/database?tls-query`, user and password percent-encoded,
    /// IPv6 hosts in brackets, port optional) and SQLite as `sqlite:path` or a bare path. TLS is
    /// read from the same query keys `to_url` writes; a value it never writes (`prefer`,
    /// `PREFERRED`, ...) reads as [`TlsMode::Auto`]. Unknown query keys are ignored.
    ///
    /// A password written exactly as [`PASSWORD_MASK`] (the placeholder of `mask_url`) is not a
    /// password: it is returned empty with [`ParsedUrl::password_masked`] set. A real `***`
    /// password is percent-encoded by `to_url` and so is not mistaken for the placeholder.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseUrlError`] whose [`UrlErrorCode`] says what is wrong.
    pub fn from_url(url: &str) -> Result<ParsedUrl, ParseUrlError> {
        let target = url.trim();
        if target.is_empty() {
            return Err(ParseUrlError::new(
                UrlErrorCode::Empty,
                "connection string is empty",
            ));
        }
        let dialect = detect_dialect(target).map_err(|e| match e {
            Error::InvalidTarget(message) => {
                ParseUrlError::new(UrlErrorCode::UnknownScheme, message)
            }
            other => ParseUrlError::new(UrlErrorCode::UnknownScheme, other.to_string()),
        })?;
        match dialect {
            Dialect::Sqlite => sqlite_from_target(target),
            Dialect::Oracle => Err(ParseUrlError::new(
                UrlErrorCode::UnsupportedEngine,
                "oracle connections are not supported yet",
            )),
            Dialect::Postgres | Dialect::Mysql | Dialect::SqlServer => {
                network_from_target(dialect, target)
            }
        }
    }
}

/// `target` without a leading `prefix`, compared ignoring ASCII case.
fn strip_prefix_ignore_case<'a>(target: &'a str, prefix: &str) -> Option<&'a str> {
    let head = target.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| target.get(prefix.len()..))
        .flatten()
}

fn malformed(message: &str) -> ParseUrlError {
    ParseUrlError::new(UrlErrorCode::Malformed, message)
}

fn decode(part: &str) -> Result<String, ParseUrlError> {
    percent_decode_str(part)
        .decode_utf8()
        .map(Cow::into_owned)
        .map_err(|_| malformed("percent-encoded text is not valid UTF-8"))
}

fn sqlite_from_target(target: &str) -> Result<ParsedUrl, ParseUrlError> {
    let path = strip_prefix_ignore_case(target, "sqlite://")
        .or_else(|| strip_prefix_ignore_case(target, "sqlite:"))
        .unwrap_or(target)
        .trim();
    if path.is_empty() {
        return Err(ParseUrlError::new(
            UrlErrorCode::Empty,
            "SQLite database file is empty",
        ));
    }
    let mut params = ConnectionParams::new(Dialect::Sqlite);
    path.clone_into(&mut params.path);
    Ok(ParsedUrl {
        params,
        password_masked: false,
    })
}

fn network_from_target(dialect: Dialect, target: &str) -> Result<ParsedUrl, ParseUrlError> {
    let (_, rest) = target
        .split_once("://")
        .ok_or_else(|| malformed("expected scheme://user:password@host:port/database"))?;
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (userinfo, hostport) = authority.rsplit_once('@').unwrap_or(("", authority));
    let (raw_user, raw_password) = match userinfo.split_once(':') {
        Some((user, password)) => (user, Some(password)),
        None => (userinfo, None),
    };
    let password_masked = raw_password == Some(PASSWORD_MASK);
    let (host, raw_port) = split_host_port(hostport)?;
    let port = match raw_port {
        None | Some("") => None,
        Some(p) => Some(p.parse::<NonZeroU16>().map(NonZeroU16::get).map_err(|_| {
            ParseUrlError::new(
                UrlErrorCode::InvalidPort,
                "port must be a number between 1 and 65535",
            )
        })?),
    };
    let mut params = ConnectionParams::new(dialect);
    params.user = decode(raw_user)?;
    if let (Some(password), false) = (raw_password, password_masked) {
        params.password = decode(password)?;
    }
    host.clone_into(&mut params.host);
    params.port = port;
    params.database = decode(path)?;
    apply_tls_query(&mut params, query);
    Ok(ParsedUrl {
        params,
        password_masked,
    })
}

/// Splits `host[:port]` / `[ipv6][:port]`; the host is returned without brackets.
fn split_host_port(hostport: &str) -> Result<(&str, Option<&str>), ParseUrlError> {
    let (host, port) = if let Some(after) = hostport.strip_prefix('[') {
        let (host, tail) = after
            .split_once(']')
            .ok_or_else(|| malformed("unclosed `[` in the host"))?;
        let port = match tail {
            "" => None,
            t => Some(
                t.strip_prefix(':')
                    .ok_or_else(|| malformed("unexpected text after the host"))?,
            ),
        };
        (host, port)
    } else {
        match hostport.rsplit_once(':') {
            Some((host, _)) if host.contains(':') => {
                return Err(malformed("IPv6 hosts must be written in brackets"));
            }
            Some((host, port)) => (host, Some(port)),
            None => (hostport, None),
        }
    };
    if host.is_empty() {
        return Err(ParseUrlError::new(
            UrlErrorCode::MissingHost,
            "host is required",
        ));
    }
    Ok((host, port))
}

fn is_one_of(value: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|c| value.eq_ignore_ascii_case(c))
}

/// Engine value of `sslmode` / `ssl-mode` as a [`TlsMode`]; anything weaker than `require` that
/// `to_url` never writes (`prefer`, `allow`, `PREFERRED`) stays [`TlsMode::Auto`].
fn tls_from_mode(value: &str, verify: &[&str], require: &str, disable: &str) -> TlsMode {
    if is_one_of(value, verify) {
        TlsMode::Verify
    } else if value.eq_ignore_ascii_case(require) {
        TlsMode::Require
    } else if value.eq_ignore_ascii_case(disable) {
        TlsMode::Disable
    } else {
        TlsMode::Auto
    }
}

/// Mirror of [`ConnectionParams::tls_query`]: fills `tls` and `trust_server_certificate`.
fn apply_tls_query(params: &mut ConnectionParams, query: &str) {
    let mut encrypt = None;
    let mut trust = false;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = key.to_ascii_lowercase();
        let is_mode_key = key == PARAM_SSLMODE || key == PARAM_SSL_MODE;
        match params.dialect {
            Dialect::Postgres if is_mode_key => {
                params.tls = tls_from_mode(
                    value,
                    &[PG_TLS_VERIFY, PG_TLS_VERIFY_CA],
                    PG_TLS_REQUIRE,
                    PG_TLS_DISABLE,
                );
            }
            Dialect::Mysql if is_mode_key => {
                params.tls = tls_from_mode(
                    value,
                    &[MYSQL_TLS_VERIFY, MYSQL_TLS_VERIFY_CA],
                    MYSQL_TLS_REQUIRE,
                    MYSQL_TLS_DISABLE,
                );
            }
            Dialect::SqlServer if key == PARAM_ENCRYPT => {
                encrypt = Some(is_one_of(value, &SQLSERVER_TRUTHY));
            }
            Dialect::SqlServer if key == PARAM_TRUST_SERVER_CERT_LOWER => {
                trust = is_one_of(value, &SQLSERVER_TRUTHY);
            }
            _ => {}
        }
    }
    if params.dialect == Dialect::SqlServer {
        params.trust_server_certificate = trust;
        params.tls = match (encrypt, trust) {
            (Some(true), true) => TlsMode::Require,
            (Some(true), false) => TlsMode::Verify,
            (Some(false), _) => TlsMode::Disable,
            (None, _) => TlsMode::Auto,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(d: Dialect) -> ConnectionParams {
        let mut c = ConnectionParams::new(d);
        c.user = "app".into();
        c.password = "p@ss:w/rd&1".into();
        c.database = "shop".into();
        c
    }

    #[test]
    fn builds_urls_with_encoded_secrets_and_default_ports() {
        assert_eq!(
            p(Dialect::Postgres).to_url().unwrap(),
            "postgres://app:p%40ss%3Aw%2Frd%261@localhost:5432/shop"
        );
        let mut my = p(Dialect::Mysql);
        my.host = "db.internal".into();
        my.port = Some(3307);
        assert_eq!(
            my.to_url().unwrap(),
            "mysql://app:p%40ss%3Aw%2Frd%261@db.internal:3307/shop"
        );
        let mut ms = p(Dialect::SqlServer);
        ms.trust_server_certificate = true;
        assert_eq!(
            ms.to_url().unwrap(),
            "sqlserver://app:p%40ss%3Aw%2Frd%261@localhost:1433/shop?trustServerCertificate=true"
        );
    }

    #[test]
    fn tls_mode_is_spelled_out_per_engine() {
        let cases = [
            (Dialect::Postgres, TlsMode::Verify, "?sslmode=verify-full"),
            (Dialect::Postgres, TlsMode::Require, "?sslmode=require"),
            (Dialect::Postgres, TlsMode::Disable, "?sslmode=disable"),
            (Dialect::Mysql, TlsMode::Verify, "?ssl-mode=VERIFY_IDENTITY"),
            (Dialect::Mysql, TlsMode::Require, "?ssl-mode=REQUIRED"),
            (Dialect::Mysql, TlsMode::Disable, "?ssl-mode=DISABLED"),
            (Dialect::SqlServer, TlsMode::Verify, "?encrypt=true"),
            (
                Dialect::SqlServer,
                TlsMode::Require,
                "?encrypt=true&trustServerCertificate=true",
            ),
            (Dialect::SqlServer, TlsMode::Disable, "?encrypt=false"),
        ];
        for (dialect, tls, suffix) in cases {
            let mut c = p(dialect);
            c.tls = tls;
            let url = c.to_url().unwrap();
            assert!(url.ends_with(suffix), "{dialect:?} {tls:?}: {url}");
        }
        // Auto não escreve nada: o padrão seguro é aplicado na conexão.
        assert!(!p(Dialect::Postgres).to_url().unwrap().contains('?'));
    }

    #[test]
    fn remote_detection_follows_the_host() {
        let mut c = p(Dialect::Postgres);
        assert!(!c.is_remote(), "host vazio é localhost");
        c.host = "db.example.com".into();
        assert!(c.is_remote());
        assert!(!ConnectionParams::new(Dialect::Sqlite).is_remote());
    }

    #[test]
    fn url_round_trips_through_detection_and_masking() {
        let url = p(Dialect::Postgres).to_url().unwrap();
        assert_eq!(crate::detect_dialect(&url), Ok(Dialect::Postgres));
        assert!(!crate::mask_url(&url).contains("p%40ss"));
    }

    #[test]
    fn handles_missing_password_ipv6_and_sqlite() {
        let mut c = ConnectionParams::new(Dialect::Postgres);
        c.user = "u".into();
        c.host = "::1".into();
        assert_eq!(c.to_url().unwrap(), "postgres://u@[::1]:5432");
        let mut s = ConnectionParams::new(Dialect::Sqlite);
        assert!(s.to_url().is_err());
        s.path = " /data/app.db ".into();
        assert_eq!(s.to_url().unwrap(), "/data/app.db");
    }

    #[test]
    fn requires_user_and_rejects_oracle_for_now() {
        let c = ConnectionParams::new(Dialect::Mysql);
        assert!(matches!(c.to_url(), Err(Error::InvalidTarget(_))));
        let mut o = ConnectionParams::new(Dialect::Oracle);
        o.user = "u".into();
        assert_eq!(o.to_url(), Err(Error::Unsupported(Dialect::Oracle)));
    }

    fn parse(url: &str) -> ParsedUrl {
        ConnectionParams::from_url(url).unwrap()
    }

    fn code(url: &str) -> UrlErrorCode {
        ConnectionParams::from_url(url).unwrap_err().code()
    }

    /// `from_url(to_url(p))` devolve `p`, normalizando o que `to_url` completa (porta padrão).
    fn assert_round_trip(original: &ConnectionParams) {
        let url = original.to_url().unwrap();
        let parsed = parse(&url);
        assert!(!parsed.password_masked, "{url}");
        let mut expected = original.clone();
        expected.port = expected
            .port
            .or(ConnectionParams::default_port(expected.dialect));
        if expected.dialect == Dialect::SqlServer && expected.tls == TlsMode::Require {
            expected.trust_server_certificate = true; // `Require` escreve trustServerCertificate
        }
        if expected.dialect != Dialect::Sqlite && expected.host.trim().is_empty() {
            expected.host = "localhost".into(); // `to_url` completa o host vazio
        }
        assert_eq!(parsed.params, expected, "{url}");
    }

    #[test]
    fn from_url_round_trips_every_dialect_and_tls_mode() {
        for dialect in [Dialect::Postgres, Dialect::Mysql, Dialect::SqlServer] {
            for tls in [
                TlsMode::Auto,
                TlsMode::Verify,
                TlsMode::Require,
                TlsMode::Disable,
            ] {
                let mut c = p(dialect);
                c.host = "db.internal".into();
                c.port = Some(2345);
                c.tls = tls;
                assert_round_trip(&c);
            }
        }
        let mut ms = p(Dialect::SqlServer);
        ms.trust_server_certificate = true;
        assert_round_trip(&ms);
        let mut s = ConnectionParams::new(Dialect::Sqlite);
        s.path = "/data/app.db".into();
        assert_round_trip(&s);
    }

    #[test]
    fn from_url_round_trips_awkward_secrets_ipv6_and_missing_parts() {
        for password in ["p@ss:w/rd%41 x", "100%", "a b", "ç~é?#&=+", "@", ":", "/"] {
            let mut c = p(Dialect::Postgres);
            c.password = password.into();
            c.user = "us@er:name".into();
            c.database = "my db/x".into();
            assert_round_trip(&c);
        }
        let mut v6 = ConnectionParams::new(Dialect::Mysql);
        v6.user = "u".into();
        v6.host = "::1".into();
        assert_round_trip(&v6);
        v6.host = "2001:db8::7".into();
        v6.port = Some(3307);
        assert_round_trip(&v6);
        let mut bare = ConnectionParams::new(Dialect::Postgres);
        bare.user = "u".into();
        assert_round_trip(&bare); // sem senha, sem banco, host vazio
    }

    #[test]
    fn from_url_reads_each_part() {
        let r = parse("postgres://app:p%40ss@db.example.com:6543/shop?sslmode=require");
        assert_eq!(r.params.dialect, Dialect::Postgres);
        assert_eq!(r.params.host, "db.example.com");
        assert_eq!(r.params.port, Some(6543));
        assert_eq!(r.params.user, "app");
        assert_eq!(r.params.password, "p@ss");
        assert_eq!(r.params.database, "shop");
        assert_eq!(r.params.tls, TlsMode::Require);
        let r = parse("postgresql://app@[::1]/shop");
        assert_eq!(r.params.host, "::1");
        assert_eq!(r.params.port, None, "sem porta = padrão do motor");
        assert_eq!(r.params.password, "");
    }

    #[test]
    fn from_url_accepts_every_scheme_alias() {
        for (scheme, dialect) in [
            ("postgres", Dialect::Postgres),
            ("postgresql", Dialect::Postgres),
            ("mysql", Dialect::Mysql),
            ("mariadb", Dialect::Mysql),
            ("sqlserver", Dialect::SqlServer),
            ("mssql", Dialect::SqlServer),
            ("MySQL", Dialect::Mysql),
        ] {
            assert_eq!(parse(&format!("{scheme}://u@h/d")).params.dialect, dialect);
        }
    }

    #[test]
    fn from_url_reads_tls_from_the_query() {
        let cases = [
            ("postgres://u@h/d?sslmode=disable", TlsMode::Disable, false),
            (
                "postgres://u@h/d?sslmode=verify-full",
                TlsMode::Verify,
                false,
            ),
            ("postgres://u@h/d?sslmode=verify-ca", TlsMode::Verify, false),
            ("postgres://u@h/d?sslmode=prefer", TlsMode::Auto, false),
            ("mysql://u@h/d?ssl-mode=DISABLED", TlsMode::Disable, false),
            ("mysql://u@h/d?ssl-mode=REQUIRED", TlsMode::Require, false),
            (
                "mysql://u@h/d?ssl-mode=VERIFY_IDENTITY",
                TlsMode::Verify,
                false,
            ),
            ("mysql://u@h/d?ssl-mode=PREFERRED", TlsMode::Auto, false),
            ("sqlserver://u@h/d?encrypt=false", TlsMode::Disable, false),
            ("sqlserver://u@h/d?encrypt=true", TlsMode::Verify, false),
            (
                "sqlserver://u@h/d?encrypt=true&trustServerCertificate=true",
                TlsMode::Require,
                true,
            ),
            (
                "sqlserver://u@h/d?trustServerCertificate=true",
                TlsMode::Auto,
                true,
            ),
            (
                "sqlserver://u@h/d?foo=bar&encrypt=true",
                TlsMode::Verify,
                false,
            ),
            ("postgres://u@h/d", TlsMode::Auto, false),
        ];
        for (url, tls, trust) in cases {
            let r = parse(url).params;
            assert_eq!((r.tls, r.trust_server_certificate), (tls, trust), "{url}");
        }
    }

    #[test]
    fn from_url_reads_sqlite_paths() {
        for (target, path) in [
            ("sqlite:app.db", "app.db"),
            ("sqlite:/data/app.db", "/data/app.db"),
            ("sqlite:///data/app.db", "/data/app.db"),
            ("/data/app.db", "/data/app.db"),
            ("./a b.db", "./a b.db"),
            ("C:\\data\\app.db", "C:\\data\\app.db"),
            ("  /data/app.db  ", "/data/app.db"),
        ] {
            let r = parse(target).params;
            assert_eq!(
                (r.dialect, r.path.as_str()),
                (Dialect::Sqlite, path),
                "{target}"
            );
        }
    }

    #[test]
    fn from_url_reports_stable_error_codes() {
        assert_eq!(code("ftp://u@h/d"), UrlErrorCode::UnknownScheme);
        assert_eq!(code("oracle://u@h/d"), UrlErrorCode::UnsupportedEngine);
        assert_eq!(code("postgres://u@/d"), UrlErrorCode::MissingHost);
        assert_eq!(code("postgres://u@:5432/d"), UrlErrorCode::MissingHost);
        assert_eq!(code("postgres://u@h:abc/d"), UrlErrorCode::InvalidPort);
        assert_eq!(code("postgres://u@h:70000/d"), UrlErrorCode::InvalidPort);
        assert_eq!(code("postgres://u@h:0/d"), UrlErrorCode::InvalidPort);
        assert_eq!(code("postgres:u@h/d"), UrlErrorCode::Malformed);
        assert_eq!(code("postgres://u@[::1/d"), UrlErrorCode::Malformed);
        assert_eq!(code("postgres://u@a:b:c/d"), UrlErrorCode::Malformed);
        assert_eq!(code("postgres://u:%FF@h/d"), UrlErrorCode::Malformed);
        assert_eq!(code("   "), UrlErrorCode::Empty);
        assert_eq!(code("sqlite:"), UrlErrorCode::Empty);
    }

    #[test]
    fn url_error_has_stable_keys_and_message_and_converts_to_invalid_target() {
        let err = ConnectionParams::from_url("ftp://x").unwrap_err();
        assert_eq!(err.code().key(), "unknown_scheme");
        assert_eq!(err.message(), "unknown scheme `ftp`");
        assert_eq!(err.to_string(), "unknown scheme `ftp`");
        assert_eq!(
            Error::from(err),
            Error::InvalidTarget("unknown scheme `ftp`".into())
        );
        let keys: Vec<_> = [
            UrlErrorCode::Empty,
            UrlErrorCode::UnknownScheme,
            UrlErrorCode::UnsupportedEngine,
            UrlErrorCode::Malformed,
            UrlErrorCode::MissingHost,
            UrlErrorCode::InvalidPort,
        ]
        .map(UrlErrorCode::key)
        .into();
        assert_eq!(
            keys,
            [
                "empty",
                "unknown_scheme",
                "unsupported_engine",
                "malformed",
                "missing_host",
                "invalid_port"
            ]
        );
    }

    #[test]
    fn from_url_flags_a_masked_password_and_keeps_it_empty() {
        let r = parse("postgres://app:***@h:5432/shop");
        assert!(r.password_masked);
        assert_eq!(r.params.password, "");
        assert_eq!(r.params.user, "app");
        // Saída de `mask_url` é entendida de volta.
        let masked = crate::mask_url(&p(Dialect::Mysql).to_url().unwrap());
        assert!(parse(&masked).password_masked);
        // Senha real "***" é escrita como %2A%2A%2A: não é o marcador.
        let mut c = p(Dialect::Postgres);
        c.password = PASSWORD_MASK.into();
        let r = parse(&c.to_url().unwrap());
        assert!(!r.password_masked);
        assert_eq!(r.params.password, "***");
        // Sem senha não há o que manter.
        assert!(!parse("postgres://app@h/shop").password_masked);
    }
}
