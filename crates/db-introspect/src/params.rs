//! Connection form fields to a connection URL (shared by Desktop, Web and CLI front ends).

use crate::{Dialect, Error};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// Everything except RFC 3986 unreserved characters is percent-encoded.
const ENCODE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

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
    /// SQL Server only: accept a self-signed server certificate.
    pub trust_server_certificate: bool,
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
        if self.dialect == Dialect::SqlServer && self.trust_server_certificate {
            url.push_str("?trustServerCertificate=true");
        }
        Ok(url)
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
}
