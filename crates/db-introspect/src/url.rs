//! Connection target parsing and secret masking.

use crate::Error;
use std::fmt::Write as _;

/// Database engine behind a connection target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// PostgreSQL.
    Postgres,
    /// MySQL / MariaDB.
    Mysql,
    /// SQLite (file).
    Sqlite,
    /// Microsoft SQL Server.
    SqlServer,
    /// Oracle Database.
    Oracle,
}

/// Detects the engine from a URL scheme (`postgres://`, `mysql://`, `sqlite:`, `sqlserver://`,
/// `oracle://`). A target without a known scheme is treated as an SQLite file path.
///
/// # Errors
///
/// Returns [`Error::InvalidTarget`] when the scheme is present but unknown.
pub fn detect_dialect(target: &str) -> Result<Dialect, Error> {
    let lower = target.trim().to_ascii_lowercase();
    let scheme = lower.split_once(':').map(|(s, _)| s);
    match scheme {
        Some("postgres" | "postgresql") => Ok(Dialect::Postgres),
        Some("mysql" | "mariadb") => Ok(Dialect::Mysql),
        Some("sqlite") => Ok(Dialect::Sqlite),
        Some("sqlserver" | "mssql") => Ok(Dialect::SqlServer),
        Some("oracle") => Ok(Dialect::Oracle),
        // `C:\data\app.db` has a one-letter "scheme": treat as a path.
        Some(s) if s.len() > 1 && lower.contains("://") => {
            Err(Error::InvalidTarget(format!("unknown scheme `{s}`")))
        }
        _ => Ok(Dialect::Sqlite),
    }
}

/// Replaces the `user:password@` part of a URL with `user:***@` so it is safe to show or log.
#[must_use]
pub fn mask_url(target: &str) -> String {
    let Some((scheme, rest)) = target.split_once("://") else {
        return target.to_owned();
    };
    let (authority, tail) = rest.split_once('/').map_or((rest, ""), |(a, t)| (a, t));
    let Some((userinfo, host)) = authority.rsplit_once('@') else {
        return target.to_owned();
    };
    let user = userinfo.split_once(':').map_or(userinfo, |(u, _)| u);
    let sep = if tail.is_empty() && !rest.contains('/') {
        ""
    } else {
        "/"
    };
    format!("{scheme}://{user}:***@{host}{sep}{tail}")
}

/// Adds `password` to a URL that has a user but no password (`scheme://user@host/db`), so the
/// secret can come from stdin or the environment instead of the shell history. URLs that already
/// carry a password, or have no user, are returned unchanged.
#[must_use]
pub fn with_password(target: &str, password: &str) -> String {
    let Some((scheme, rest)) = target.split_once("://") else {
        return target.to_owned();
    };
    let (authority, tail) = rest
        .split_once('/')
        .map_or((rest, None), |(a, t)| (a, Some(t)));
    let Some((userinfo, host)) = authority.rsplit_once('@') else {
        return target.to_owned();
    };
    if userinfo.contains(':') {
        return target.to_owned();
    }
    let mut enc = String::new();
    for b in password.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            enc.push(char::from(b));
        } else {
            let _ = write!(enc, "%{b:02X}");
        }
    }
    let tail = tail.map_or_else(String::new, |t| format!("/{t}"));
    format!("{scheme}://{userinfo}:{enc}@{host}{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_dialects() {
        assert_eq!(detect_dialect("postgres://u:p@h/db"), Ok(Dialect::Postgres));
        assert_eq!(detect_dialect("mysql://u@h/db"), Ok(Dialect::Mysql));
        assert_eq!(detect_dialect("sqlserver://u@h"), Ok(Dialect::SqlServer));
        assert_eq!(detect_dialect("oracle://u@h/x"), Ok(Dialect::Oracle));
        assert_eq!(detect_dialect("sqlite:app.db"), Ok(Dialect::Sqlite));
        assert_eq!(detect_dialect("./app.db"), Ok(Dialect::Sqlite));
        assert_eq!(detect_dialect("C:\\data\\app.db"), Ok(Dialect::Sqlite));
        assert!(detect_dialect("ftp://x").is_err());
    }

    #[test]
    fn injects_password_only_when_missing() {
        assert_eq!(
            with_password("postgres://admin@localhost:5432/app", "p@ss/w:rd"),
            "postgres://admin:p%40ss%2Fw%3Ard@localhost:5432/app"
        );
        assert_eq!(
            with_password("postgres://admin:old@h/app", "new"),
            "postgres://admin:old@h/app"
        );
        assert_eq!(with_password("postgres://h/app", "x"), "postgres://h/app");
        assert_eq!(with_password("./a.db", "x"), "./a.db");
        assert_eq!(with_password("postgres://u@h", "x"), "postgres://u:x@h");
    }

    #[test]
    fn masks_password() {
        assert_eq!(
            mask_url("postgres://admin:s3cret@localhost:5432/app"),
            "postgres://admin:***@localhost:5432/app"
        );
        assert_eq!(
            mask_url("postgres://admin@localhost/app"),
            "postgres://admin:***@localhost/app"
        );
        assert_eq!(mask_url("./app.db"), "./app.db");
        assert_eq!(
            mask_url("postgres://localhost/app"),
            "postgres://localhost/app"
        );
    }
}
