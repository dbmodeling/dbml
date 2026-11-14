//! SQL Server end-to-end against a real server (for example a Docker container).
//!
//! Ignored by default. Run with:
//! `DBI_MSSQL_URL='sqlserver://sa:Secret_123%21@localhost:51433/dbi?trustServerCertificate=true' \
//!  cargo test -p db-introspect --test sqlserver -- --ignored`
//! The `dbi` database is created by the test from the `master` database of the same server.

use db_introspect::{Error, Options, introspect, introspect_with, test_connection};
use tiberius::{AuthMethod, Client, Config};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;

const DDL: &[&str] = &[
    "IF DB_ID('dbi') IS NOT NULL BEGIN ALTER DATABASE dbi SET SINGLE_USER WITH ROLLBACK IMMEDIATE; DROP DATABASE dbi; END",
    "CREATE DATABASE dbi",
    "USE dbi",
    "CREATE SCHEMA sales",
    "CREATE TABLE dbo.users (
       id INT IDENTITY(1,1) PRIMARY KEY,
       email NVARCHAR(255) NOT NULL UNIQUE,
       name NVARCHAR(MAX),
       active BIT NOT NULL DEFAULT 1,
       created_at DATETIME2 NOT NULL DEFAULT GETDATE(),
       role NVARCHAR(20) NOT NULL DEFAULT 'member',
       balance DECIMAL(10,2) DEFAULT 0
     )",
    "EXEC sp_addextendedproperty 'MS_Description', 'login', 'SCHEMA', 'dbo', 'TABLE', 'users', 'COLUMN', 'email'",
    "EXEC sp_addextendedproperty 'MS_Description', 'people', 'SCHEMA', 'dbo', 'TABLE', 'users'",
    "CREATE TABLE sales.orders (
       id BIGINT IDENTITY(1,1) PRIMARY KEY,
       user_id INT NOT NULL CONSTRAINT fk_orders_user REFERENCES dbo.users(id)
         ON DELETE CASCADE ON UPDATE NO ACTION,
       total DECIMAL(10,2)
     )",
    "CREATE INDEX idx_orders_total ON sales.orders (total, user_id)",
    "CREATE TABLE sales.order_items (
       order_id BIGINT NOT NULL REFERENCES sales.orders(id),
       line INT NOT NULL,
       sku VARCHAR(30),
       CONSTRAINT pk_items PRIMARY KEY (order_id, line),
       CONSTRAINT uq_item_sku UNIQUE (order_id, sku)
     )",
    "CREATE VIEW dbo.v_users AS SELECT id FROM dbo.users",
];

fn url() -> String {
    std::env::var("DBI_MSSQL_URL").expect("set DBI_MSSQL_URL to run this test")
}

static READY: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

async fn setup() {
    READY
        .get_or_init(|| async {
            let u = url::Url::parse(&url()).unwrap();
            let mut config = Config::new();
            config.host(u.host_str().unwrap());
            config.port(u.port().unwrap_or(1433));
            config.database("master");
            let pass = percent_encoding::percent_decode_str(u.password().unwrap())
                .decode_utf8_lossy()
                .into_owned();
            config.authentication(AuthMethod::sql_server(u.username(), pass));
            config.trust_cert();
            let tcp = TcpStream::connect(config.get_addr()).await.unwrap();
            let mut client = Box::pin(Client::connect(config, tcp.compat_write()))
                .await
                .unwrap();
            for stmt in DDL {
                client
                    .simple_query(*stmt)
                    .await
                    .unwrap()
                    .into_results()
                    .await
                    .unwrap();
            }
        })
        .await;
}

#[tokio::test]
#[ignore = "needs a SQL Server (DBI_MSSQL_URL)"]
async fn reads_columns_refs_and_indexes() {
    setup().await;
    let out = introspect(&url()).await.unwrap();
    let d = &out.dbml;
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(
        d.contains("Table users {"),
        "dbo must not be qualified: {d}"
    );
    assert!(d.contains("Table sales.orders {"), "{d}");
    assert!(d.contains("id int [pk, increment]"), "{d}");
    assert!(
        d.contains("email varchar(255) [not null, unique, note: 'login']"),
        "{d}"
    );
    assert!(d.contains("name text"), "{d}");
    assert!(
        d.contains("active boolean [not null, default: true]"),
        "{d}"
    );
    assert!(
        d.contains("created_at timestamp [not null, default: `getdate()`]"),
        "{d}"
    );
    assert!(
        d.contains("role varchar(20) [not null, default: 'member']"),
        "{d}"
    );
    assert!(d.contains("balance decimal(10,2) [default: 0]"), "{d}");
    assert!(d.contains("Note: 'people'"), "{d}");
    assert!(d.contains("id bigint [pk, increment]"), "{d}");
    assert!(
        d.contains("Ref: sales.orders.user_id > users.id [delete: cascade]"),
        "{d}"
    );
    assert!(
        d.contains("Ref: sales.order_items.order_id > sales.orders.id"),
        "{d}"
    );
    assert!(d.contains("(order_id, line) [pk]"), "{d}");
    assert!(
        d.contains("(order_id, sku) [unique, name: 'uq_item_sku']"),
        "{d}"
    );
    assert!(
        d.contains("(total, user_id) [name: 'idx_orders_total']"),
        "{d}"
    );
    assert!(!d.contains("v_users"), "views must not appear: {d}");
}

#[tokio::test]
#[ignore = "needs a SQL Server (DBI_MSSQL_URL)"]
async fn output_parses_as_valid_dbml_and_schema_filter_works() {
    setup().await;
    let opts = Options {
        schemas: vec!["sales".into()],
        ..Options::default()
    };
    let out = introspect_with(&url(), &opts).await.unwrap();
    assert!(!out.dbml.contains("Table users"), "{}", out.dbml);
    let all = introspect(&url()).await.unwrap();
    let schema = dbml_analysis::analyze(&all.dbml);
    assert_eq!(schema.tables.len(), 3, "{}", all.dbml);
    assert_eq!(schema.refs.len(), 2);
}

#[tokio::test]
#[ignore = "needs a SQL Server (DBI_MSSQL_URL)"]
async fn bad_password_does_not_leak_the_password() {
    let bad = url().replacen("Secret_123%21", "Wrong-pass-123", 1);
    let err = introspect(&bad).await.unwrap_err();
    assert!(matches!(err, Error::Connect(_)), "{err:?}");
    assert!(!err.to_string().contains("Wrong-pass-123"), "{err}");
}

#[tokio::test]
#[ignore = "needs a SQL Server (DBI_MSSQL_URL)"]
async fn untrusted_certificate_gives_an_actionable_hint() {
    setup().await;
    let strict = url().replace("?trustServerCertificate=true", "");
    let err = introspect(&strict).await.unwrap_err();
    assert!(
        err.to_string().contains("trustServerCertificate"),
        "expected a hint about trustServerCertificate, got: {err}"
    );
}

#[tokio::test]
#[ignore = "needs a SQL Server (DBI_MSSQL_URL)"]
async fn unreachable_host_times_out_with_clear_error() {
    let opts = Options {
        timeout_secs: 1,
        ..Options::default()
    };
    let err = introspect_with("sqlserver://sa:pw-xyz@10.255.255.1:1433/db", &opts)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Connect(_)), "{err:?}");
    assert!(!err.to_string().contains("pw-xyz"));
}

#[tokio::test]
#[ignore = "needs a server (DBI_MSSQL_URL)"]
async fn test_connection_reports_server_and_schemas() {
    setup().await;
    let out = test_connection(&url(), &Options::default()).await.unwrap();
    assert!(
        out.server.contains("Microsoft SQL Server"),
        "{}",
        out.server
    );
    assert!(
        out.schemas.iter().any(|s| s == "sales"),
        "{:?}",
        out.schemas
    );
    assert!(!out.schemas.iter().any(|s| s == "sys"));
}

#[tokio::test]
#[ignore = "needs a server (DBI_MSSQL_URL)"]
async fn test_connection_with_bad_password_fails_without_leaking_it() {
    let bad = url().replacen("Secret_123%21", "wrong-pass-123", 1);
    let err = test_connection(&bad, &Options::default())
        .await
        .unwrap_err();
    assert!(!err.to_string().contains("wrong-pass-123"), "{err}");
}
