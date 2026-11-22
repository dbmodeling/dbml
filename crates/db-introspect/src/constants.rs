//! Shared TLS query parameter and mode constants for database introspection.

/// PostgreSQL SSL mode query parameter key.
pub const PARAM_SSLMODE: &str = "sslmode";
/// MySQL SSL mode query parameter key.
pub const PARAM_SSL_MODE: &str = "ssl-mode";
/// SQL Server encrypt query parameter key.
pub const PARAM_ENCRYPT: &str = "encrypt";
/// SQL Server trust-server-certificate query parameter key.
pub const PARAM_TRUST_SERVER_CERT: &str = "trustServerCertificate";
/// Lowercase SQL Server trust-server-certificate key for case-insensitive matching.
pub const PARAM_TRUST_SERVER_CERT_LOWER: &str = "trustservercertificate";

/// PostgreSQL verified TLS value.
pub const PG_TLS_VERIFY: &str = "verify-full";
/// PostgreSQL CA-only verified TLS value (read back as verified TLS).
pub const PG_TLS_VERIFY_CA: &str = "verify-ca";
/// PostgreSQL required (unverified) TLS value.
pub const PG_TLS_REQUIRE: &str = "require";
/// PostgreSQL disabled TLS value.
pub const PG_TLS_DISABLE: &str = "disable";

/// MySQL verified TLS value.
pub const MYSQL_TLS_VERIFY: &str = "VERIFY_IDENTITY";
/// MySQL CA-only verified TLS value (read back as verified TLS).
pub const MYSQL_TLS_VERIFY_CA: &str = "VERIFY_CA";
/// MySQL required (unverified) TLS value.
pub const MYSQL_TLS_REQUIRE: &str = "REQUIRED";
/// MySQL disabled TLS value.
pub const MYSQL_TLS_DISABLE: &str = "DISABLED";

/// SQL Server truthy flag value.
pub const SQLSERVER_TRUE: &str = "true";
/// Every spelling of a true SQL Server flag (`encrypt`, `trustServerCertificate`).
pub const SQLSERVER_TRUTHY: [&str; 3] = [SQLSERVER_TRUE, "yes", "1"];
/// SQL Server falsy flag value.
pub const SQLSERVER_FALSE: &str = "false";

/// Case-insensitive driver substrings indicating an authentication failure rather than a network error.
pub const AUTH_ERROR_MARKERS: [&str; 6] = [
    "28p01",
    "28000",
    "password authentication failed",
    "access denied for user",
    "login failed for user",
    "authentication failed",
];

/// Placeholder shown instead of a password (see `mask_url`); read back by `ConnectionParams::from_url`
/// as "keep the password already typed in the form".
pub const PASSWORD_MASK: &str = "***";

/// Stable error keys of `ParseUrlError`, shared with the front ends (translated by key).
pub const URL_ERR_EMPTY: &str = "empty";
/// The scheme is not one of the supported engines.
pub const URL_ERR_UNKNOWN_SCHEME: &str = "unknown_scheme";
/// The engine is recognised but cannot be edited as a form yet (Oracle).
pub const URL_ERR_UNSUPPORTED_ENGINE: &str = "unsupported_engine";
/// The URL is not well formed.
pub const URL_ERR_MALFORMED: &str = "malformed";
/// A network URL without a host.
pub const URL_ERR_MISSING_HOST: &str = "missing_host";
/// The port is not a number in `1..=65535`.
pub const URL_ERR_INVALID_PORT: &str = "invalid_port";
