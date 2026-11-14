//! `POST /api/introspect`: lets the web UI read the catalog of a database through this local
//! server (browsers cannot open TCP sockets). Read-only; credentials only travel in the request
//! body and are never logged or echoed back.

use axum::{
    Json,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use db_introspect::{ConnectionParams, Dialect, Options};
use serde::{Deserialize, Serialize};

/// Form fields sent by the "Connect to Database" modal.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct IntrospectRequest {
    /// `postgres`, `mysql`, `sqlserver` or `sqlite`.
    pub dialect: String,
    pub host: String,
    pub port: Option<u16>,
    pub database: String,
    pub user: String,
    pub password: String,
    /// SQLite file path on the machine running `dbml run`.
    pub path: String,
    pub trust_server_certificate: bool,
    /// Only these schemas; empty means all user schemas.
    pub schemas: Vec<String>,
}

#[derive(Serialize)]
pub struct IntrospectResponse {
    pub dbml: String,
    pub warnings: Vec<String>,
    pub tables: usize,
}

fn parse_dialect(s: &str) -> Option<Dialect> {
    match s.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" => Some(Dialect::Postgres),
        "mysql" | "mariadb" => Some(Dialect::Mysql),
        "sqlserver" | "mssql" => Some(Dialect::SqlServer),
        "sqlite" => Some(Dialect::Sqlite),
        _ => None,
    }
}

/// A page the user is visiting must not be able to drive this endpoint: when the request carries
/// an `Origin`, it has to be this server's own origin. Requests without `Origin` (curl, native
/// clients) are allowed; the `application/json` requirement of [`axum::Json`] already forces a
/// CORS preflight for cross-site pages.
#[must_use]
pub fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let (Ok(origin), Some(Ok(host))) = (
        origin.to_str(),
        headers.get(header::HOST).map(|h| h.to_str()),
    ) else {
        return false;
    };
    origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .is_some_and(|authority| authority.eq_ignore_ascii_case(host))
}

#[derive(Serialize)]
pub struct TestResponse {
    pub server: String,
    pub schemas: Vec<String>,
}

fn target(req: IntrospectRequest) -> Result<(String, Options), (StatusCode, String)> {
    let dialect = parse_dialect(&req.dialect).ok_or((
        StatusCode::BAD_REQUEST,
        format!("unsupported database `{}`", req.dialect),
    ))?;
    let mut params = ConnectionParams::new(dialect);
    params.host = req.host;
    params.port = req.port;
    params.database = req.database;
    params.user = req.user;
    params.password = req.password;
    params.path = req.path;
    params.trust_server_certificate = req.trust_server_certificate;
    let url = params
        .to_url()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let options = Options {
        schemas: req.schemas,
        ..Options::default()
    };
    Ok((url, options))
}

/// Checks that the credentials work and lists the schemas, without reading any table.
///
/// # Errors
///
/// Returns an HTTP status and a message that never contains the password.
pub async fn run_test(req: IntrospectRequest) -> Result<TestResponse, (StatusCode, String)> {
    let (url, options) = target(req)?;
    let out = db_introspect::test_connection(&url, &options)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(TestResponse {
        server: out.server,
        schemas: out.schemas,
    })
}

/// Pure request handler (no HTTP types) so it can be tested without a server.
///
/// # Errors
///
/// Returns an HTTP status and a message that never contains the password.
pub async fn run(req: IntrospectRequest) -> Result<IntrospectResponse, (StatusCode, String)> {
    let (url, options) = target(req)?;
    let outcome = Box::pin(db_introspect::introspect_with(&url, &options))
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    let tables = dbml_analysis::analyze(&outcome.dbml).tables.len();
    Ok(IntrospectResponse {
        dbml: outcome.dbml,
        warnings: outcome.warnings,
        tables,
    })
}

pub async fn test_handler(headers: HeaderMap, Json(req): Json<IntrospectRequest>) -> Response {
    if !origin_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "cross-origin request refused" })),
        )
            .into_response();
    }
    match run_test(req).await {
        Ok(ok) => Json(ok).into_response(),
        Err((status, error)) => {
            (status, Json(serde_json::json!({ "error": error }))).into_response()
        }
    }
}

pub async fn handler(headers: HeaderMap, Json(req): Json<IntrospectRequest>) -> Response {
    if !origin_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "cross-origin request refused" })),
        )
            .into_response();
    }
    match run(req).await {
        Ok(ok) => Json(ok).into_response(),
        Err((status, error)) => {
            (status, Json(serde_json::json!({ "error": error }))).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(origin: Option<&str>, host: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        if let Some(o) = origin {
            h.insert(header::ORIGIN, HeaderValue::from_str(o).unwrap());
        }
        h
    }

    #[test]
    fn only_same_origin_or_no_origin_is_allowed() {
        assert!(origin_allowed(&headers(None, "127.0.0.1:3000")));
        assert!(origin_allowed(&headers(
            Some("http://127.0.0.1:3000"),
            "127.0.0.1:3000"
        )));
        assert!(!origin_allowed(&headers(
            Some("https://evil.example"),
            "127.0.0.1:3000"
        )));
        assert!(!origin_allowed(&headers(
            Some("http://127.0.0.1:9999"),
            "127.0.0.1:3000"
        )));
        assert!(!origin_allowed(&headers(Some("null"), "127.0.0.1:3000")));
    }

    #[tokio::test]
    async fn test_endpoint_validates_and_hides_the_password() {
        let (status, msg) = run_test(IntrospectRequest {
            dialect: "db2".into(),
            ..Default::default()
        })
        .await
        .err()
        .unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(msg.contains("unsupported"), "{msg}");
        let (status, msg) = run_test(IntrospectRequest {
            dialect: "mysql".into(),
            host: "127.0.0.1".into(),
            port: Some(1),
            user: "u".into(),
            password: "pw-secret-xyz".into(),
            ..Default::default()
        })
        .await
        .err()
        .unwrap();
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert!(!msg.contains("pw-secret-xyz"), "{msg}");
    }

    #[tokio::test]
    async fn rejects_unknown_engine_and_missing_fields() {
        let bad = IntrospectRequest {
            dialect: "db2".into(),
            ..Default::default()
        };
        assert_eq!(run(bad).await.err().unwrap().0, StatusCode::BAD_REQUEST);
        let no_user = IntrospectRequest {
            dialect: "postgres".into(),
            ..Default::default()
        };
        let (status, msg) = run(no_user).await.err().unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(msg.contains("user is required"), "{msg}");
    }

    #[tokio::test]
    async fn unreachable_server_is_a_bad_gateway_without_the_password() {
        let req = IntrospectRequest {
            dialect: "postgres".into(),
            host: "127.0.0.1".into(),
            port: Some(1),
            user: "u".into(),
            password: "pw-secret-xyz".into(),
            ..Default::default()
        };
        let (status, msg) = run(req).await.err().unwrap();
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert!(!msg.contains("pw-secret-xyz"), "{msg}");
    }
}
