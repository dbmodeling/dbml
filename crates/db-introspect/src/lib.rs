#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Read-only database introspection: connects to a database, reads its catalog and returns DBML.
//!
//! Only catalog queries are issued and connections are opened read-only. Network and driver
//! dependencies live in this crate so the pure core (`schema`, `dbml-*`) stays free of them.

mod constants;
mod dbml;
mod model;
mod mysql;
mod params;
mod postgres;
mod sqlite;
mod sqlserver;
mod tls;
mod url;

pub use dbml::to_dbml;
pub use model::{Column, EnumType, ForeignKey, Index, Introspected, Table};
pub use params::ConnectionParams;
pub use tls::{
    TlsMode, apply_default as with_default_tls, apply_mode as with_tls_mode, is_loopback_host,
    weak_tls_warning,
};
pub use url::{Dialect, detect_dialect, mask_url, with_password};

/// Error raised while connecting to or reading a database. Messages never contain the password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The connection target could not be understood.
    InvalidTarget(String),
    /// The engine is recognised but its introspector is not available yet.
    Unsupported(Dialect),
    /// Connecting failed (network, authentication, missing file, timeout).
    Connect(String),
    /// A catalog query failed.
    Query(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTarget(m) => write!(f, "invalid connection target: {m}"),
            Self::Unsupported(d) => write!(f, "introspection of {d:?} is not available yet"),
            Self::Connect(m) => write!(f, "could not connect: {m}"),
            Self::Query(m) => write!(f, "could not read the catalog: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// Limits applied while reading a catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Only read these schemas; empty means every user schema.
    pub schemas: Vec<String>,
    /// Seconds to wait for the connection and for each query.
    pub timeout_secs: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            schemas: Vec::new(),
            timeout_secs: 10,
        }
    }
}

/// Result of [`test_connection`]: proof that the credentials work, without reading the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestOutcome {
    /// Server version banner (first line), or the SQLite library version for file databases.
    pub server: String,
    /// User schemas available to pick from (empty for SQLite).
    pub schemas: Vec<String>,
}

/// Result of [`introspect`]: DBML text plus non-fatal warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Generated DBML.
    pub dbml: String,
    /// Non-fatal problems found while reading the catalog.
    pub warnings: Vec<String>,
}

/// Connects to `target` (URL or SQLite file path), reads the catalog and renders DBML.
///
/// Network targets follow the TLS policy of [`TlsMode`]: a non-loopback host without an explicit
/// TLS parameter is connected with verified TLS, and a weaker explicit choice is reported in
/// [`Outcome::warnings`].
///
/// # Errors
///
/// Returns [`Error`] when the target is invalid, the engine is not supported yet, the connection
/// fails or a catalog query fails.
pub async fn introspect(target: &str) -> Result<Outcome, Error> {
    introspect_with(target, &Options::default()).await
}

/// Like [`introspect`] but restricted by [`Options`].
///
/// # Errors
///
/// Same as [`introspect`].
pub async fn introspect_with(target: &str, options: &Options) -> Result<Outcome, Error> {
    let db = read_catalog_with(target, options).await?;
    Ok(Outcome {
        dbml: to_dbml(&db),
        warnings: db.warnings.clone(),
    })
}

/// Connects to `target` and returns the neutral catalog model.
///
/// # Errors
///
/// Same as [`introspect`].
pub async fn read_catalog(target: &str) -> Result<Introspected, Error> {
    read_catalog_with(target, &Options::default()).await
}

/// Like [`read_catalog`] but restricted by [`Options`].
///
/// # Errors
///
/// Same as [`introspect`].
pub async fn read_catalog_with(target: &str, options: &Options) -> Result<Introspected, Error> {
    let target = with_default_tls(target);
    let target = &*target;
    let mut db = match detect_dialect(target)? {
        Dialect::Sqlite => sqlite::read(target).await,
        Dialect::Postgres => postgres::read(target, options).await,
        Dialect::Mysql => mysql::read(target, options).await,
        Dialect::SqlServer => Box::pin(sqlserver::read(target, options)).await,
        Dialect::Oracle => Err(Error::Unsupported(Dialect::Oracle)),
    }?;
    if let Some(warning) = weak_tls_warning(target) {
        db.warnings.push(warning);
    }
    Ok(db)
}

/// Connects to `target`, runs a trivial query and lists the schemas, without reading any tables.
/// Uses the same read-only connection as [`introspect`].
///
/// # Errors
///
/// Returns [`Error::Connect`] / [`Error::Query`] on failure (never containing the password) and
/// [`Error::Unsupported`] for engines without an introspector.
pub async fn test_connection(target: &str, options: &Options) -> Result<TestOutcome, Error> {
    let target = with_default_tls(target);
    let target = &*target;
    match detect_dialect(target)? {
        Dialect::Sqlite => sqlite::ping(target).await,
        Dialect::Postgres => postgres::ping(target, options).await,
        Dialect::Mysql => mysql::ping(target, options).await,
        Dialect::SqlServer => Box::pin(sqlserver::ping(target, options)).await,
        Dialect::Oracle => Err(Error::Unsupported(Dialect::Oracle)),
    }
}

/// Removes anything that looks like the target (and so a password) from a driver message.
pub(crate) fn sanitize(msg: &str, target: &str) -> String {
    msg.replace(target, &mask_url(target))
}
