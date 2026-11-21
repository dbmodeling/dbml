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
/// PostgreSQL required (unverified) TLS value.
pub const PG_TLS_REQUIRE: &str = "require";
/// PostgreSQL disabled TLS value.
pub const PG_TLS_DISABLE: &str = "disable";

/// MySQL verified TLS value.
pub const MYSQL_TLS_VERIFY: &str = "VERIFY_IDENTITY";
/// MySQL required (unverified) TLS value.
pub const MYSQL_TLS_REQUIRE: &str = "REQUIRED";
/// MySQL disabled TLS value.
pub const MYSQL_TLS_DISABLE: &str = "DISABLED";

/// SQL Server truthy flag value.
pub const SQLSERVER_TRUE: &str = "true";
/// SQL Server falsy flag value.
pub const SQLSERVER_FALSE: &str = "false";
