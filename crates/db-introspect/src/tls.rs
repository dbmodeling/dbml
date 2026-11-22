//! TLS policy for network targets (PostgreSQL, MySQL, SQL Server).
//!
//! Without an explicit setting the drivers fall back to opportunistic TLS: the connection may be
//! plain text and the server certificate is not verified, so a password can travel in clear.
//! The default here is stricter: a non-loopback host gets verified TLS unless the target says
//! otherwise, and an explicit weaker choice is reported as a warning.

use crate::Dialect;
use crate::constants::{
    MYSQL_TLS_DISABLE, MYSQL_TLS_REQUIRE, MYSQL_TLS_VERIFY, PARAM_ENCRYPT, PARAM_SSL_MODE,
    PARAM_SSLMODE, PARAM_TRUST_SERVER_CERT, PARAM_TRUST_SERVER_CERT_LOWER, PG_TLS_DISABLE,
    PG_TLS_REQUIRE, PG_TLS_VERIFY, SQLSERVER_FALSE, SQLSERVER_TRUE,
};
use std::borrow::Cow;

/// How a network connection must protect its traffic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TlsMode {
    /// Encrypted TLS for non-loopback hosts (works with cloud private CAs); driver default for loopback.
    #[default]
    Auto,
    /// TLS with certificate and host name verification.
    Verify,
    /// TLS without verifying the server certificate (self-signed or cloud private CAs).
    Require,
    /// No TLS: the password travels in clear text. Explicit opt-out.
    Disable,
}

/// Whether `host` only reaches this machine (`localhost`, `127.0.0.0/8`, `::1`, or no host for a
/// Unix socket).
#[must_use]
pub fn is_loopback_host(host: &str) -> bool {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']');
    if h.is_empty() || h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    h.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// What the target URL already says about TLS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Declared {
    /// No TLS parameter in the URL.
    Nothing,
    /// TLS with a verified certificate.
    Verified,
    /// TLS but the certificate is not verified.
    Unverified,
    /// Plain text allowed or forced.
    Plain,
}

fn declared(dialect: Dialect, u: &url::Url) -> Declared {
    let mut result = Declared::Nothing;
    let mut encrypt: Option<bool> = None;
    let mut trust = false;
    for (k, v) in u.query_pairs() {
        let k = k.to_ascii_lowercase();
        let v = v.to_ascii_lowercase();
        match (dialect, k.as_str()) {
            (Dialect::Postgres, "sslmode" | "ssl-mode") => {
                result = match v.as_str() {
                    "verify-full" | "verify-ca" => Declared::Verified,
                    "require" => Declared::Unverified,
                    _ => Declared::Plain, // disable, allow, prefer
                };
            }
            (Dialect::Mysql, "ssl-mode" | "sslmode") => {
                result = match v.as_str() {
                    "verify_identity" | "verify_ca" | "verify-full" | "verify-ca" => {
                        Declared::Verified
                    }
                    "required" | "require" => Declared::Unverified,
                    _ => Declared::Plain, // disabled, preferred
                };
            }
            (Dialect::SqlServer, "encrypt") => {
                encrypt = Some(matches!(v.as_str(), "true" | "yes" | "1"));
            }
            (Dialect::SqlServer, "trustservercertificate") => {
                trust = matches!(v.as_str(), "true" | "yes" | "1");
            }
            _ => {}
        }
    }
    if dialect == Dialect::SqlServer {
        result = match (encrypt, trust) {
            (Some(false), _) => Declared::Plain,
            (Some(true), false) => Declared::Verified,
            (Some(true) | None, true) => Declared::Unverified,
            (None, false) => Declared::Nothing,
        };
    }
    result
}

fn parse(target: &str) -> Option<(Dialect, url::Url)> {
    let dialect = crate::detect_dialect(target).ok()?;
    if !matches!(
        dialect,
        Dialect::Postgres | Dialect::Mysql | Dialect::SqlServer
    ) {
        return None;
    }
    let u = url::Url::parse(target.trim()).ok()?;
    Some((dialect, u))
}

/// Applies an explicit [`TlsMode`] to `target`, overriding or appending the engine's TLS query
/// parameter. [`TlsMode::Auto`], SQLite and unparsable targets are returned unchanged.
#[must_use]
pub fn apply_mode(target: &str, mode: TlsMode) -> Cow<'_, str> {
    if mode == TlsMode::Auto {
        return Cow::Borrowed(target);
    }
    let Some((dialect, mut u)) = parse(target) else {
        return Cow::Borrowed(target);
    };
    let existing: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| {
            let k = k.to_ascii_lowercase();
            !matches!(
                (dialect, k.as_str()),
                (
                    Dialect::Postgres | Dialect::Mysql,
                    PARAM_SSLMODE | PARAM_SSL_MODE
                ) | (
                    Dialect::SqlServer,
                    PARAM_ENCRYPT | PARAM_TRUST_SERVER_CERT_LOWER
                )
            )
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    {
        let mut pairs = u.query_pairs_mut();
        pairs.clear();
        for (k, v) in existing {
            pairs.append_pair(&k, &v);
        }
        match (dialect, mode) {
            (Dialect::Postgres, TlsMode::Verify) => {
                pairs.append_pair(PARAM_SSLMODE, PG_TLS_VERIFY);
            }
            (Dialect::Postgres, TlsMode::Require) => {
                pairs.append_pair(PARAM_SSLMODE, PG_TLS_REQUIRE);
            }
            (Dialect::Postgres, TlsMode::Disable) => {
                pairs.append_pair(PARAM_SSLMODE, PG_TLS_DISABLE);
            }
            (Dialect::Mysql, TlsMode::Verify) => {
                pairs.append_pair(PARAM_SSL_MODE, MYSQL_TLS_VERIFY);
            }
            (Dialect::Mysql, TlsMode::Require) => {
                pairs.append_pair(PARAM_SSL_MODE, MYSQL_TLS_REQUIRE);
            }
            (Dialect::Mysql, TlsMode::Disable) => {
                pairs.append_pair(PARAM_SSL_MODE, MYSQL_TLS_DISABLE);
            }
            (Dialect::SqlServer, TlsMode::Verify) => {
                pairs.append_pair(PARAM_ENCRYPT, SQLSERVER_TRUE);
            }
            (Dialect::SqlServer, TlsMode::Require) => {
                pairs.append_pair(PARAM_ENCRYPT, SQLSERVER_TRUE);
                pairs.append_pair(PARAM_TRUST_SERVER_CERT, SQLSERVER_TRUE);
            }
            (Dialect::SqlServer, TlsMode::Disable) => {
                pairs.append_pair(PARAM_ENCRYPT, SQLSERVER_FALSE);
            }
            _ => {}
        }
    }
    Cow::Owned(u.into())
}

/// Applies the default TLS policy to `target`: a non-loopback host without any TLS parameter
/// gets encrypted TLS (`require` / `REQUIRED` / `encrypt=true&trustServerCertificate=true`) so
/// managed cloud databases with private CAs (AWS RDS, GCP Cloud SQL, Supabase, Azure) connect out
/// of the box without travelling in clear text. Explicit settings, loopback hosts, SQLite and
/// unparsable targets are left untouched.
#[must_use]
pub fn apply_default(target: &str) -> Cow<'_, str> {
    let Some((dialect, u)) = parse(target) else {
        return Cow::Borrowed(target);
    };
    if is_loopback_host(u.host_str().unwrap_or("")) || declared(dialect, &u) != Declared::Nothing {
        return Cow::Borrowed(target);
    }
    let param = match dialect {
        Dialect::Postgres => "sslmode=require",
        Dialect::Mysql => "ssl-mode=REQUIRED",
        _ => "encrypt=true&trustServerCertificate=true",
    };
    let t = target.trim();
    let sep = if t.contains('?') { '&' } else { '?' };
    Cow::Owned(format!("{t}{sep}{param}"))
}

/// Explains why `target` is not protected by verified TLS, or `None` when it is (or when the
/// host is loopback). Call it on the target after [`apply_default`].
#[must_use]
pub fn weak_tls_warning(target: &str) -> Option<String> {
    let (dialect, u) = parse(target)?;
    let host = u.host_str().unwrap_or("");
    if is_loopback_host(host) {
        return None;
    }
    match declared(dialect, &u) {
        Declared::Plain => Some(format!(
            "connection to {host} is not encrypted: the password and data travel in clear text"
        )),
        Declared::Unverified => Some(format!(
            "connection to {host} is encrypted but the server certificate is not verified"
        )),
        Declared::Verified | Declared::Nothing => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_detection() {
        for h in [
            "localhost",
            "LOCALHOST",
            "127.0.0.1",
            "127.9.9.9",
            "::1",
            "[::1]",
            "",
        ] {
            assert!(is_loopback_host(h), "{h}");
        }
        for h in ["db.internal", "10.0.0.5", "8.8.8.8", "::2", "example.com"] {
            assert!(!is_loopback_host(h), "{h}");
        }
    }

    #[test]
    fn remote_hosts_get_encrypted_tls_by_default() {
        assert_eq!(
            apply_default("postgres://u:p@db.example.com:5432/app"),
            "postgres://u:p@db.example.com:5432/app?sslmode=require"
        );
        assert_eq!(
            apply_default("mysql://u@10.0.0.5/app?charset=utf8"),
            "mysql://u@10.0.0.5/app?charset=utf8&ssl-mode=REQUIRED"
        );
        assert_eq!(
            apply_default("sqlserver://sa:x@db.example.com/app"),
            "sqlserver://sa:x@db.example.com/app?encrypt=true&trustServerCertificate=true"
        );
    }

    #[test]
    fn loopback_explicit_and_sqlite_targets_are_untouched() {
        for t in [
            "postgres://u:p@localhost/app",
            "postgres://u@127.0.0.1:5432/app",
            "mysql://u@[::1]/app",
            "postgres://u@db.example.com/app?sslmode=disable",
            "postgres://u@db.example.com/app?sslmode=require",
            "mysql://u@db.example.com/app?ssl-mode=DISABLED",
            "sqlserver://sa@db.example.com/app?encrypt=false",
            "sqlserver://sa@db.example.com/app?trustServerCertificate=true",
            "/data/app.db",
            "sqlite:///data/app.db",
        ] {
            assert_eq!(apply_default(t), t, "{t}");
        }
    }

    #[test]
    fn explicit_weak_settings_on_remote_hosts_are_reported() {
        let w = |t: &str| weak_tls_warning(&apply_default(t));
        assert!(
            w("postgres://u@db.example.com/app?sslmode=disable")
                .unwrap()
                .contains("not encrypted")
        );
        assert!(
            w("postgres://u@db.example.com/app?sslmode=prefer")
                .unwrap()
                .contains("not encrypted")
        );
        assert!(
            w("mysql://u@db.example.com/app?ssl-mode=DISABLED")
                .unwrap()
                .contains("not encrypted")
        );
        assert!(
            w("sqlserver://sa@db.example.com/app?encrypt=false")
                .unwrap()
                .contains("not encrypted")
        );
        assert!(
            w("postgres://u@db.example.com/app?sslmode=require")
                .unwrap()
                .contains("not verified")
        );
        assert!(
            w("sqlserver://sa@db.example.com/app?trustServerCertificate=true")
                .unwrap()
                .contains("not verified")
        );
    }

    #[test]
    fn secure_and_local_targets_have_no_warning() {
        let w = |t: &str| weak_tls_warning(&apply_default(t));
        assert_eq!(
            w("postgres://u@db.example.com/app?sslmode=verify-full"),
            None
        );
        assert_eq!(w("postgres://u@localhost/app?sslmode=disable"), None);
        assert_eq!(w("/data/app.db"), None);
    }
}
