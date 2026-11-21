#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! PostgreSQL end-to-end against a real server (for example a Docker container).
//!
//! Ignored by default. Run with:
//! `DBI_POSTGRES_URL=postgres://postgres:secret@localhost:55432/app cargo test -p db-introspect --test postgres -- --ignored`

use db_introspect::{Error, Options, introspect, introspect_with, test_connection};
use sqlx::{Connection, PgConnection};

const DDL: &str = "
DROP SCHEMA IF EXISTS dbi_a CASCADE;
DROP SCHEMA IF EXISTS dbi_b CASCADE;
CREATE SCHEMA dbi_a;
CREATE SCHEMA dbi_b;
CREATE TYPE dbi_a.status AS ENUM ('new', 'paid', 'shipped');
CREATE TABLE dbi_a.users (
  id serial PRIMARY KEY,
  email varchar(255) NOT NULL UNIQUE,
  name text,
  created_at timestamptz NOT NULL DEFAULT now(),
  role text NOT NULL DEFAULT 'member',
  tags text[]
);
COMMENT ON TABLE dbi_a.users IS 'people';
COMMENT ON COLUMN dbi_a.users.email IS 'login';
CREATE TABLE dbi_a.orders (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id int NOT NULL REFERENCES dbi_a.users(id) ON DELETE CASCADE ON UPDATE RESTRICT,
  status dbi_a.status NOT NULL DEFAULT 'new',
  total numeric(10,2) DEFAULT 0
);
CREATE INDEX idx_orders_status ON dbi_a.orders (status, user_id);
CREATE INDEX idx_users_expr ON dbi_a.users (lower(name));
CREATE TABLE dbi_a.order_items (
  order_id bigint NOT NULL REFERENCES dbi_a.orders(id),
  line int NOT NULL,
  sku text,
  UNIQUE (order_id, sku),
  PRIMARY KEY (order_id, line)
);
CREATE TABLE dbi_b.audit (id int PRIMARY KEY, user_id int REFERENCES dbi_a.users(id));
";

fn url() -> String {
    std::env::var("DBI_POSTGRES_URL").expect("set DBI_POSTGRES_URL to run this test")
}

static READY: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

/// Creates the fixture once per test binary; tests run in parallel and only read it.
async fn setup() {
    READY
        .get_or_init(|| async {
            let mut conn = PgConnection::connect(&url()).await.unwrap();
            sqlx::raw_sql(DDL).execute(&mut conn).await.unwrap();
        })
        .await;
}

#[tokio::test]
#[ignore = "needs a PostgreSQL server (DBI_POSTGRES_URL)"]
async fn reads_schemas_enums_columns_refs_and_indexes() {
    setup().await;
    let opts = Options {
        schemas: vec!["dbi_a".into(), "dbi_b".into()],
        ..Options::default()
    };
    let out = introspect_with(&url(), &opts).await.unwrap();
    let d = &out.dbml;
    assert!(d.contains("Enum dbi_a.status {"), "{d}");
    assert!(d.contains("Table dbi_a.users {"), "{d}");
    assert!(d.contains("id integer [pk, increment]"), "{d}");
    assert!(
        d.contains("email varchar(255) [not null, unique, note: 'login']"),
        "{d}"
    );
    assert!(
        d.contains("created_at timestamptz [not null, default: `now()`]"),
        "{d}"
    );
    assert!(d.contains("role text [not null, default: 'member']"), "{d}");
    assert!(d.contains("tags text[]"), "{d}");
    assert!(d.contains("Note: 'people'"), "{d}");
    assert!(d.contains("id bigint [pk, increment]"), "{d}");
    assert!(
        d.contains("status dbi_a.status [not null, default: 'new']"),
        "{d}"
    );
    assert!(d.contains("total numeric(10,2) [default: 0]"), "{d}");
    assert!(
        d.contains(
            "Ref: dbi_a.orders.user_id > dbi_a.users.id [delete: cascade, update: restrict]"
        ),
        "{d}"
    );
    assert!(
        d.contains("Ref: dbi_a.order_items.order_id > dbi_a.orders.id"),
        "{d}"
    );
    assert!(
        d.contains("Ref: dbi_b.audit.user_id > dbi_a.users.id"),
        "{d}"
    );
    assert!(d.contains("(order_id, line) [pk]"), "{d}");
    assert!(d.contains("(order_id, sku) [unique"), "{d}");
    assert!(
        d.contains("(status, user_id) [name: 'idx_orders_status']"),
        "{d}"
    );
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert!(out.warnings[0].contains("idx_users_expr"));
}

#[tokio::test]
#[ignore = "needs a PostgreSQL server (DBI_POSTGRES_URL)"]
async fn output_parses_as_valid_dbml() {
    setup().await;
    let opts = Options {
        schemas: vec!["dbi_a".into()],
        ..Options::default()
    };
    let out = introspect_with(&url(), &opts).await.unwrap();
    let schema = dbml_analysis::analyze(&out.dbml);
    assert_eq!(schema.tables.len(), 3, "{}", out.dbml);
    assert_eq!(schema.enums.len(), 1);
}

#[tokio::test]
#[ignore = "needs a PostgreSQL server (DBI_POSTGRES_URL)"]
async fn schema_filter_limits_tables() {
    setup().await;
    let opts = Options {
        schemas: vec!["dbi_b".into()],
        ..Options::default()
    };
    let out = introspect_with(&url(), &opts).await.unwrap();
    assert!(out.dbml.contains("Table dbi_b.audit {"));
    assert!(!out.dbml.contains("dbi_a.users {"));
}

#[tokio::test]
#[ignore = "needs a PostgreSQL server (DBI_POSTGRES_URL)"]
async fn connection_is_read_only() {
    // The introspector's own connection options force read-only transactions; prove the server
    // honours that option by repeating it on a raw connection.
    let opts: sqlx::postgres::PgConnectOptions = url().parse().unwrap();
    let opts = opts.options([("default_transaction_read_only", "on")]);
    let mut conn = <PgConnection as Connection>::connect_with(&opts)
        .await
        .unwrap();
    let err = sqlx::raw_sql("CREATE TABLE dbi_should_fail (id int)")
        .execute(&mut conn)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("read-only"), "{err}");
}

#[tokio::test]
#[ignore = "needs a PostgreSQL server (DBI_POSTGRES_URL)"]
async fn bad_password_does_not_leak_the_password() {
    let bad = url().replacen("secret", "wrong-pass-123", 1);
    let err = introspect(&bad).await.unwrap_err();
    assert!(matches!(err, Error::Connect(_)), "{err:?}");
    assert!(!err.to_string().contains("wrong-pass-123"), "{err}");
}

#[tokio::test]
#[ignore = "needs a PostgreSQL server (DBI_POSTGRES_URL)"]
async fn unreachable_host_times_out_with_clear_error() {
    let opts = Options {
        timeout_secs: 1,
        ..Options::default()
    };
    let err = introspect_with("postgres://u:pw-xyz@10.255.255.1:5432/db", &opts)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Connect(_)), "{err:?}");
    assert!(!err.to_string().contains("pw-xyz"));
}

#[tokio::test]
#[ignore = "needs a server (DBI_POSTGRES_URL)"]
async fn test_connection_reports_server_and_schemas() {
    setup().await;
    let out = test_connection(&url(), &Options::default()).await.unwrap();
    assert!(out.server.starts_with("PostgreSQL 16"), "{}", out.server);
    assert!(
        out.schemas.iter().any(|s| s == "dbi_a"),
        "{:?}",
        out.schemas
    );
    assert!(!out.schemas.iter().any(|s| s == "pg_catalog"));
}

#[tokio::test]
#[ignore = "needs a server (DBI_POSTGRES_URL)"]
async fn test_connection_with_bad_password_fails_without_leaking_it() {
    let bad = url().replacen("secret", "wrong-pass-123", 1);
    let err = test_connection(&bad, &Options::default())
        .await
        .unwrap_err();
    assert!(!err.to_string().contains("wrong-pass-123"), "{err}");
}
