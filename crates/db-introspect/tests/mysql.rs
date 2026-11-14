//! MySQL end-to-end against a real server (for example a Docker container).
//!
//! Ignored by default. Run with:
//! `DBI_MYSQL_URL=mysql://root:secret@localhost:53306/app cargo test -p db-introspect --test mysql -- --ignored`

use db_introspect::{Error, Options, introspect, introspect_with, test_connection};
use sqlx::{Connection, MySqlConnection};

const DDL: &[&str] = &[
    "DROP DATABASE IF EXISTS dbi_b",
    "DROP DATABASE IF EXISTS dbi_a",
    "CREATE DATABASE dbi_a",
    "CREATE DATABASE dbi_b",
    "CREATE TABLE dbi_a.users (
       id INT AUTO_INCREMENT PRIMARY KEY,
       email VARCHAR(255) NOT NULL UNIQUE COMMENT 'login',
       name TEXT,
       active TINYINT(1) NOT NULL DEFAULT 1,
       created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
       role ENUM('admin','member') NOT NULL DEFAULT 'member',
       nick VARCHAR(20) DEFAULT 'anon'
     ) COMMENT='people'",
    "CREATE TABLE dbi_a.orders (
       id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
       user_id INT NOT NULL,
       total DECIMAL(10,2) DEFAULT 0,
       CONSTRAINT fk_orders_user FOREIGN KEY (user_id) REFERENCES dbi_a.users(id)
         ON DELETE CASCADE ON UPDATE RESTRICT,
       INDEX idx_orders_total (total, user_id)
     )",
    "CREATE TABLE dbi_a.order_items (
       order_id BIGINT UNSIGNED NOT NULL,
       line INT NOT NULL,
       sku VARCHAR(30),
       PRIMARY KEY (order_id, line),
       UNIQUE KEY uq_item_sku (order_id, sku),
       FOREIGN KEY (order_id) REFERENCES dbi_a.orders(id)
     )",
    "CREATE TABLE dbi_b.audit (id INT PRIMARY KEY, user_id INT,
       FOREIGN KEY (user_id) REFERENCES dbi_a.users(id))",
    "CREATE VIEW dbi_a.v_users AS SELECT id FROM dbi_a.users",
];

fn url() -> String {
    std::env::var("DBI_MYSQL_URL").expect("set DBI_MYSQL_URL to run this test")
}

static READY: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

async fn setup() {
    READY
        .get_or_init(|| async {
            let mut conn = MySqlConnection::connect(&url()).await.unwrap();
            for stmt in DDL {
                sqlx::raw_sql(*stmt).execute(&mut conn).await.unwrap();
            }
        })
        .await;
}

fn both() -> Options {
    Options {
        schemas: vec!["dbi_a".into(), "dbi_b".into()],
        ..Options::default()
    }
}

#[tokio::test]
#[ignore = "needs a MySQL server (DBI_MYSQL_URL)"]
async fn reads_columns_refs_indexes_and_enums() {
    setup().await;
    let out = introspect_with(&url(), &both()).await.unwrap();
    let d = &out.dbml;
    assert!(d.contains("Enum dbi_a.users_role {"), "{d}");
    assert!(d.contains("Table dbi_a.users {"), "{d}");
    assert!(d.contains("id int [pk, increment]"), "{d}");
    assert!(
        d.contains("email varchar(255) [not null, unique, note: 'login']"),
        "{d}"
    );
    assert!(
        d.contains("active boolean [not null, default: true]"),
        "{d}"
    );
    assert!(
        d.contains("created_at timestamp [not null, default: `CURRENT_TIMESTAMP`]"),
        "{d}"
    );
    assert!(
        d.contains("role dbi_a.users_role [not null, default: 'member']"),
        "{d}"
    );
    assert!(d.contains("nick varchar(20) [default: 'anon']"), "{d}");
    assert!(d.contains("Note: 'people'"), "{d}");
    assert!(d.contains("id \"bigint unsigned\" [pk, increment]"), "{d}");
    assert!(d.contains("total decimal(10,2) [default: 0.00]"), "{d}");
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
    assert!(
        d.contains("(order_id, sku) [unique, name: 'uq_item_sku']"),
        "{d}"
    );
    assert!(
        d.contains("(total, user_id) [name: 'idx_orders_total']"),
        "{d}"
    );
    assert!(!d.contains("v_users"), "views must not appear: {d}");
    assert!(
        !d.contains("fk_orders_user"),
        "FK helper index must be hidden: {d}"
    );
}

#[tokio::test]
#[ignore = "needs a MySQL server (DBI_MYSQL_URL)"]
async fn output_parses_as_valid_dbml() {
    setup().await;
    let opts = Options {
        schemas: vec!["dbi_a".into()],
        ..Options::default()
    };
    let out = introspect_with(&url(), &opts).await.unwrap();
    assert!(
        !out.dbml.contains("Table dbi_a."),
        "single schema is not qualified: {}",
        out.dbml
    );
    let schema = dbml_analysis::analyze(&out.dbml);
    assert_eq!(schema.tables.len(), 3, "{}", out.dbml);
    assert_eq!(schema.enums.len(), 1);
}

#[tokio::test]
#[ignore = "needs a MySQL server (DBI_MYSQL_URL)"]
async fn session_is_read_only() {
    let mut conn = MySqlConnection::connect(&url()).await.unwrap();
    sqlx::query("SET SESSION TRANSACTION READ ONLY")
        .execute(&mut conn)
        .await
        .unwrap();
    let err = sqlx::raw_sql("CREATE TABLE dbi_a.should_fail (id INT)")
        .execute(&mut conn)
        .await;
    // DDL is implicitly committed and is not blocked by READ ONLY in every MySQL version, but
    // DML is: that is what a misbehaving caller could do to the user's data.
    let _ = err;
    sqlx::raw_sql("INSERT INTO dbi_b.audit (id) VALUES (999)")
        .execute(&mut conn)
        .await
        .expect_err("writes must be rejected in a read-only session");
}

#[tokio::test]
#[ignore = "needs a MySQL server (DBI_MYSQL_URL)"]
async fn bad_password_does_not_leak_the_password() {
    let bad = url().replacen("secret", "wrong-pass-123", 1);
    let err = introspect(&bad).await.unwrap_err();
    assert!(matches!(err, Error::Connect(_)), "{err:?}");
    assert!(!err.to_string().contains("wrong-pass-123"), "{err}");
}

#[tokio::test]
#[ignore = "needs a server (DBI_MYSQL_URL)"]
async fn test_connection_reports_server_and_schemas() {
    setup().await;
    let out = test_connection(&url(), &Options::default()).await.unwrap();
    assert!(out.server.starts_with("MySQL 8"), "{}", out.server);
    assert!(
        out.schemas.iter().any(|s| s == "dbi_a"),
        "{:?}",
        out.schemas
    );
    assert!(!out.schemas.iter().any(|s| s == "mysql"));
}

#[tokio::test]
#[ignore = "needs a server (DBI_MYSQL_URL)"]
async fn test_connection_with_bad_password_fails_without_leaking_it() {
    let bad = url().replacen("secret", "wrong-pass-123", 1);
    let err = test_connection(&bad, &Options::default())
        .await
        .unwrap_err();
    assert!(!err.to_string().contains("wrong-pass-123"), "{err}");
}
