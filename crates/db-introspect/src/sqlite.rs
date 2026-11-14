//! SQLite introspector: `sqlite_master` plus table-valued `pragma_*` functions, read-only.

use crate::model::{Column, ForeignKey, Index, Introspected, Table};
use crate::{Error, sanitize};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, Row, SqliteConnection};
use std::collections::HashMap;
use std::str::FromStr;

fn options(target: &str) -> Result<SqliteConnectOptions, Error> {
    let t = target.trim();
    let opts = if t.to_ascii_lowercase().starts_with("sqlite:") {
        SqliteConnectOptions::from_str(t).map_err(|e| Error::InvalidTarget(e.to_string()))?
    } else {
        SqliteConnectOptions::new().filename(t)
    };
    Ok(opts.read_only(true).create_if_missing(false))
}

fn query_err(target: &str, e: &sqlx::Error) -> Error {
    Error::Query(sanitize(&e.to_string(), target))
}

pub(crate) async fn read(target: &str) -> Result<Introspected, Error> {
    let mut conn = options(target)?
        .connect()
        .await
        .map_err(|e| Error::Connect(sanitize(&e.to_string(), target)))?;
    read_conn(&mut conn, target).await
}

pub(crate) async fn ping(target: &str) -> Result<crate::TestOutcome, Error> {
    let mut conn = options(target)?
        .connect()
        .await
        .map_err(|e| Error::Connect(sanitize(&e.to_string(), target)))?;
    let row = sqlx::query("SELECT sqlite_version() AS v")
        .fetch_one(&mut conn)
        .await
        .map_err(|e| query_err(target, &e))?;
    let v: String = row.try_get("v").map_err(|e| query_err(target, &e))?;
    // Opening an arbitrary non-database file succeeds lazily; this query proves it is a database.
    sqlx::query("SELECT count(*) FROM sqlite_master")
        .fetch_one(&mut conn)
        .await
        .map_err(|e| query_err(target, &e))?;
    Ok(crate::TestOutcome {
        server: format!("SQLite {v}"),
        schemas: Vec::new(),
    })
}

fn action(raw: &str) -> Option<String> {
    match raw.to_ascii_uppercase().as_str() {
        "CASCADE" => Some("cascade".to_owned()),
        "SET NULL" => Some("set null".to_owned()),
        "SET DEFAULT" => Some("set default".to_owned()),
        "RESTRICT" => Some("restrict".to_owned()),
        _ => None, // NO ACTION is the default
    }
}

async fn read_conn(conn: &mut SqliteConnection, target: &str) -> Result<Introspected, Error> {
    let mut db = Introspected::default();
    let masters = sqlx::query(
        "SELECT name, sql FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| query_err(target, &e))?;

    let mut pk_cols: HashMap<String, Vec<String>> = HashMap::new();
    let mut raw_fks: Vec<(String, Vec<sqlx::sqlite::SqliteRow>)> = Vec::new();

    for m in &masters {
        let name: String = m.try_get("name").map_err(|e| query_err(target, &e))?;
        let ddl: Option<String> = m.try_get("sql").map_err(|e| query_err(target, &e))?;
        let autoinc = ddl
            .as_deref()
            .is_some_and(|s| s.to_ascii_uppercase().contains("AUTOINCREMENT"));

        let col_rows = sqlx::query(
            "SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_xinfo(?) \
             WHERE hidden != 1 ORDER BY cid",
        )
        .bind(&name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| query_err(target, &e))?;

        let mut columns = Vec::new();
        let mut pks: Vec<(i64, String)> = Vec::new();
        for r in &col_rows {
            let cname: String = r.try_get("name").map_err(|e| query_err(target, &e))?;
            let ty: String = r.try_get("type").map_err(|e| query_err(target, &e))?;
            let notnull: i64 = r.try_get("notnull").map_err(|e| query_err(target, &e))?;
            let default: Option<String> =
                r.try_get("dflt_value").map_err(|e| query_err(target, &e))?;
            let pk: i64 = r.try_get("pk").map_err(|e| query_err(target, &e))?;
            if pk > 0 {
                pks.push((pk, cname.clone()));
            }
            columns.push(Column {
                name: cname,
                ty,
                not_null: notnull != 0,
                primary_key: false,
                unique: false,
                auto_increment: false,
                default,
                comment: None,
            });
        }
        pks.sort_by_key(|(order, _)| *order);
        let mut indexes = Vec::new();
        match pks.as_slice() {
            [] => {}
            [(_, only)] => {
                if let Some(c) = columns.iter_mut().find(|c| &c.name == only) {
                    c.primary_key = true;
                    c.auto_increment = autoinc;
                }
            }
            many => indexes.push(Index {
                name: None,
                columns: many.iter().map(|(_, n)| n.clone()).collect(),
                unique: false,
                primary_key: true,
            }),
        }
        pk_cols.insert(name.clone(), pks.iter().map(|(_, n)| n.clone()).collect());

        let idx_rows =
            sqlx::query("SELECT name, \"unique\", origin FROM pragma_index_list(?) ORDER BY seq")
                .bind(&name)
                .fetch_all(&mut *conn)
                .await
                .map_err(|e| query_err(target, &e))?;
        for ir in &idx_rows {
            let iname: String = ir.try_get("name").map_err(|e| query_err(target, &e))?;
            let unique: i64 = ir.try_get("unique").map_err(|e| query_err(target, &e))?;
            let origin: String = ir.try_get("origin").map_err(|e| query_err(target, &e))?;
            if origin == "pk" {
                continue;
            }
            let cols: Vec<Option<String>> =
                sqlx::query("SELECT name FROM pragma_index_info(?) ORDER BY seqno")
                    .bind(&iname)
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(|e| query_err(target, &e))?
                    .iter()
                    .map(|r| r.try_get("name"))
                    .collect::<Result<_, _>>()
                    .map_err(|e| query_err(target, &e))?;
            let Some(cols) = cols.into_iter().collect::<Option<Vec<String>>>() else {
                db.warnings.push(format!(
                    "{name}: index `{iname}` uses an expression and was skipped"
                ));
                continue;
            };
            // Single-column UNIQUE constraint declared in the table: fold into the column.
            if origin == "u"
                && unique != 0
                && cols.len() == 1
                && let Some(c) = columns.iter_mut().find(|c| c.name == cols[0])
            {
                c.unique = true;
                continue;
            }
            indexes.push(Index {
                // Auto-named constraint indexes (`sqlite_autoindex_*`) carry no information.
                name: (!iname.starts_with("sqlite_autoindex_")).then_some(iname),
                columns: cols,
                unique: unique != 0,
                primary_key: false,
            });
        }

        let fk_rows = sqlx::query(
            "SELECT id, seq, \"table\", \"from\", \"to\", on_update, on_delete \
             FROM pragma_foreign_key_list(?) ORDER BY id, seq",
        )
        .bind(&name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| query_err(target, &e))?;
        raw_fks.push((name.clone(), fk_rows));

        db.tables.push(Table {
            schema: None,
            name,
            columns,
            indexes,
            comment: None,
        });
    }

    for (from_table, rows) in &raw_fks {
        let mut groups: Vec<(i64, ForeignKey)> = Vec::new();
        let mut wildcard: Vec<bool> = Vec::new();
        for r in rows {
            let id: i64 = r.try_get("id").map_err(|e| query_err(target, &e))?;
            let to_table: String = r.try_get("table").map_err(|e| query_err(target, &e))?;
            let from: String = r.try_get("from").map_err(|e| query_err(target, &e))?;
            let to: Option<String> = r.try_get("to").map_err(|e| query_err(target, &e))?;
            let on_update: String = r.try_get("on_update").map_err(|e| query_err(target, &e))?;
            let on_delete: String = r.try_get("on_delete").map_err(|e| query_err(target, &e))?;
            if let Some(pos) = groups.iter().position(|(gid, _)| *gid == id) {
                let fk = &mut groups[pos].1;
                fk.from_cols.push(from);
                if let Some(t) = to {
                    fk.to_cols.push(t);
                }
            } else {
                wildcard.push(to.is_none());
                groups.push((
                    id,
                    ForeignKey {
                        from_schema: None,
                        from_table: from_table.clone(),
                        from_cols: vec![from],
                        to_schema: None,
                        to_table,
                        to_cols: to.into_iter().collect(),
                        on_delete: action(&on_delete),
                        on_update: action(&on_update),
                    },
                ));
            }
        }
        for ((_, mut fk), is_wildcard) in groups.into_iter().zip(wildcard) {
            if is_wildcard || fk.to_cols.is_empty() {
                // `REFERENCES parent` without columns targets the parent's primary key.
                fk.to_cols = pk_cols.get(&fk.to_table).cloned().unwrap_or_default();
            }
            if fk.to_cols.len() == fk.from_cols.len() {
                db.refs.push(fk);
            } else {
                db.warnings.push(format!(
                    "{}: foreign key to `{}` skipped (cannot resolve referenced columns)",
                    fk.from_table, fk.to_table
                ));
            }
        }
    }
    Ok(db)
}
