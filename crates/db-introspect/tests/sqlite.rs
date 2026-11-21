#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! SQLite end-to-end: real file, read-only connection, DBML that parses cleanly.

use db_introspect::{Error, Options, introspect, test_connection};
use rusqlite::Connection;

const DDL: &str = "
CREATE TABLE users (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  email VARCHAR(255) NOT NULL UNIQUE,
  name TEXT,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  role TEXT NOT NULL DEFAULT 'member'
);
CREATE TABLE orders (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  status VARCHAR(20) DEFAULT 'new',
  total DECIMAL(10,2) DEFAULT 0
);
CREATE INDEX idx_orders_status ON orders(status);
CREATE TABLE order_items (
  order_id INTEGER NOT NULL,
  line INTEGER NOT NULL,
  sku TEXT,
  PRIMARY KEY (order_id, line),
  FOREIGN KEY (order_id) REFERENCES orders
);
CREATE TABLE \"weird name\" (\"a b\" INT);
";

fn make_db(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("dbi_{name}_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(DDL).unwrap();
    path
}

#[tokio::test]
async fn generates_dbml_with_columns_refs_and_indexes() {
    let path = make_db("full");
    let out = introspect(path.to_str().unwrap()).await.unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let dbml = &out.dbml;
    assert!(dbml.contains("Table users {"), "{dbml}");
    assert!(dbml.contains("id integer [pk, increment]"), "{dbml}");
    assert!(
        dbml.contains("email varchar(255) [not null, unique]"),
        "{dbml}"
    );
    assert!(
        dbml.contains("created_at timestamp [default: `CURRENT_TIMESTAMP`]"),
        "{dbml}"
    );
    assert!(
        dbml.contains("role text [not null, default: 'member']"),
        "{dbml}"
    );
    assert!(dbml.contains("total decimal(10,2) [default: 0]"), "{dbml}");
    assert!(
        dbml.contains("Ref: orders.user_id > users.id [delete: cascade]"),
        "{dbml}"
    );
    assert!(
        dbml.contains("Ref: order_items.order_id > orders.id"),
        "{dbml}"
    );
    assert!(dbml.contains("(order_id, line) [pk]"), "{dbml}");
    assert!(
        dbml.contains("status [name: 'idx_orders_status']"),
        "{dbml}"
    );
    assert!(dbml.contains("Table \"weird name\" {"), "{dbml}");
}

#[tokio::test]
async fn output_is_valid_dbml_with_no_diagnostics() {
    let path = make_db("valid");
    let out = introspect(path.to_str().unwrap()).await.unwrap();
    let _ = std::fs::remove_file(&path);
    let schema = dbml_analysis::analyze(&out.dbml);
    assert_eq!(schema.tables.len(), 4);
    assert_eq!(schema.refs.len(), 2);
}

#[tokio::test]
async fn connection_is_read_only() {
    let path = make_db("ro");
    let before = std::fs::read(&path).unwrap();
    let _ = introspect(path.to_str().unwrap()).await.unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "file must not change"
    );
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn missing_file_is_a_connect_error_not_a_created_file() {
    let path = std::env::temp_dir().join(format!("dbi_missing_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let err = introspect(path.to_str().unwrap()).await.unwrap_err();
    assert!(matches!(err, Error::Connect(_)), "{err:?}");
    assert!(!path.exists());
}

#[tokio::test]
async fn not_yet_supported_engines_report_unsupported() {
    let err = introspect("oracle://u:secret@localhost:1/db")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Unsupported(_)));
    assert!(!err.to_string().contains("secret"));
}

#[tokio::test]
async fn test_connection_reports_version_and_rejects_non_databases() {
    let path = make_db("ping");
    let ok = test_connection(path.to_str().unwrap(), &Options::default())
        .await
        .unwrap();
    assert!(ok.server.starts_with("SQLite 3."), "{}", ok.server);
    assert_eq!(ok.schemas, Vec::<String>::new());
    let _ = std::fs::remove_file(&path);

    let junk = std::env::temp_dir().join(format!("dbi_junk_{}.sqlite", std::process::id()));
    std::fs::write(
        &junk,
        b"this is definitely not a sqlite database file, just text",
    )
    .unwrap();
    let err = test_connection(junk.to_str().unwrap(), &Options::default())
        .await
        .unwrap_err();
    let _ = std::fs::remove_file(&junk);
    assert!(
        matches!(err, Error::Query(_) | Error::Connect(_)),
        "{err:?}"
    );

    let missing = std::env::temp_dir().join("dbi_no_such_file.sqlite");
    assert!(
        test_connection(missing.to_str().unwrap(), &Options::default())
            .await
            .is_err()
    );
    assert!(!missing.exists());
}
