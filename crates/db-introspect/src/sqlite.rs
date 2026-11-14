//! SQLite introspector: `sqlite_master` plus table-valued `pragma_*` functions, read-only.
//!
//! Uses `rusqlite` with the bundled SQLite (not `sqlx-sqlite`, whose `libsqlite3-sys` range
//! excludes the patched releases). The blocking driver runs on tokio's blocking pool.

use crate::model::{Column, ForeignKey, Index, Introspected, Table};
use crate::{Error, sanitize};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;

/// `sqlite:relative.db`, `sqlite:///abs/app.db?mode=ro` or a plain path -> the file path.
fn file_path(target: &str) -> String {
    let t = target.trim();
    if t.to_ascii_lowercase().starts_with("sqlite:") {
        let rest = &t["sqlite:".len()..];
        let rest = rest.strip_prefix("//").unwrap_or(rest);
        return rest.split('?').next().unwrap_or(rest).to_owned();
    }
    t.to_owned()
}

fn open(target: &str) -> Result<Connection, Error> {
    let path = file_path(target);
    if path.is_empty() {
        return Err(Error::InvalidTarget("choose a SQLite database file".into()));
    }
    Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| Error::Connect(sanitize(&e.to_string(), target)))
}

fn qerr(target: &str, e: &rusqlite::Error) -> Error {
    Error::Query(sanitize(&e.to_string(), target))
}

/// Runs a blocking closure on tokio's blocking pool.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| Error::Query(format!("worker failed: {e}")))?
}

pub(crate) async fn read(target: &str) -> Result<Introspected, Error> {
    let target = target.to_owned();
    blocking(move || {
        let conn = open(&target)?;
        read_conn(&conn, &target)
    })
    .await
}

pub(crate) async fn ping(target: &str) -> Result<crate::TestOutcome, Error> {
    let target = target.to_owned();
    blocking(move || {
        let conn = open(&target)?;
        let version: String = conn
            .query_row("SELECT sqlite_version()", [], |r| r.get(0))
            .map_err(|e| qerr(&target, &e))?;
        // Opening an arbitrary non-database file succeeds lazily; this proves it is a database.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(|e| qerr(&target, &e))?;
        Ok(crate::TestOutcome {
            server: format!("SQLite {version}"),
            schemas: Vec::new(),
        })
    })
    .await
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

struct FkRow {
    id: i64,
    table: String,
    from: String,
    to: Option<String>,
    on_update: String,
    on_delete: String,
}

fn read_conn(conn: &Connection, target: &str) -> Result<Introspected, Error> {
    let e = |x: rusqlite::Error| qerr(target, &x);
    let mut db = Introspected::default();

    let masters: Vec<(String, Option<String>)> = conn
        .prepare(
            "SELECT name, sql FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name",
        )
        .map_err(e)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(e)?
        .collect::<Result<_, _>>()
        .map_err(e)?;

    let mut pk_cols: HashMap<String, Vec<String>> = HashMap::new();
    let mut raw_fks: Vec<(String, Vec<FkRow>)> = Vec::new();

    for (name, ddl) in &masters {
        let autoinc = ddl
            .as_deref()
            .is_some_and(|s| s.to_ascii_uppercase().contains("AUTOINCREMENT"));

        // (name, type, notnull, default, pk position)
        let col_rows: Vec<(String, String, i64, Option<String>, i64)> = conn
            .prepare(
                "SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_xinfo(?1) \
                 WHERE hidden != 1 ORDER BY cid",
            )
            .map_err(e)?
            .query_map([name], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .map_err(e)?
            .collect::<Result<_, _>>()
            .map_err(e)?;

        let mut columns = Vec::new();
        let mut pks: Vec<(i64, String)> = Vec::new();
        for (cname, ty, notnull, default, pk) in col_rows {
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

        let idx_rows: Vec<(String, i64, String)> = conn
            .prepare("SELECT name, \"unique\", origin FROM pragma_index_list(?1) ORDER BY seq")
            .map_err(e)?
            .query_map([name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(e)?
            .collect::<Result<_, _>>()
            .map_err(e)?;
        for (iname, unique, origin) in idx_rows {
            if origin == "pk" {
                continue;
            }
            let cols: Vec<Option<String>> = conn
                .prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")
                .map_err(e)?
                .query_map([&iname], |r| r.get(0))
                .map_err(e)?
                .collect::<Result<_, _>>()
                .map_err(e)?;
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

        let fk_rows: Vec<FkRow> = conn
            .prepare(
                "SELECT id, \"table\", \"from\", \"to\", on_update, on_delete \
                 FROM pragma_foreign_key_list(?1) ORDER BY id, seq",
            )
            .map_err(e)?
            .query_map([name], |r| {
                Ok(FkRow {
                    id: r.get(0)?,
                    table: r.get(1)?,
                    from: r.get(2)?,
                    to: r.get(3)?,
                    on_update: r.get(4)?,
                    on_delete: r.get(5)?,
                })
            })
            .map_err(e)?
            .collect::<Result<_, _>>()
            .map_err(e)?;
        raw_fks.push((name.clone(), fk_rows));

        db.tables.push(Table {
            schema: None,
            name: name.clone(),
            columns,
            indexes,
            comment: None,
        });
    }

    for (from_table, rows) in raw_fks {
        let mut groups: Vec<(i64, ForeignKey, bool)> = Vec::new();
        for r in rows {
            if let Some(g) = groups.iter_mut().find(|(gid, _, _)| *gid == r.id) {
                g.1.from_cols.push(r.from);
                if let Some(t) = r.to {
                    g.1.to_cols.push(t);
                }
            } else {
                let wildcard = r.to.is_none();
                groups.push((
                    r.id,
                    ForeignKey {
                        from_schema: None,
                        from_table: from_table.clone(),
                        from_cols: vec![r.from],
                        to_schema: None,
                        to_table: r.table,
                        to_cols: r.to.into_iter().collect(),
                        on_delete: action(&r.on_delete),
                        on_update: action(&r.on_update),
                    },
                    wildcard,
                ));
            }
        }
        for (_, mut fk, wildcard) in groups {
            if wildcard || fk.to_cols.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::file_path;

    #[test]
    fn extracts_the_file_path() {
        assert_eq!(file_path("/data/app.db"), "/data/app.db");
        assert_eq!(file_path("sqlite:app.db"), "app.db");
        assert_eq!(file_path("sqlite:///abs/app.db?mode=ro"), "/abs/app.db");
        assert_eq!(file_path("SQLITE:rel/app.db"), "rel/app.db");
        assert_eq!(file_path("  ./x.db  "), "./x.db");
    }
}
