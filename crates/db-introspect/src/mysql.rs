//! MySQL / MariaDB introspector: `information_schema` queries over a read-only session.

use crate::model::{Column, EnumType, ForeignKey, Index, Introspected, Table};
use crate::{Error, Options, sanitize};
use sqlx::mysql::{MySqlConnectOptions, MySqlRow};
use sqlx::{ConnectOptions, MySqlConnection, Row};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

const SYSTEM_SCHEMAS: [&str; 4] = ["mysql", "information_schema", "performance_schema", "sys"];

fn qerr(target: &str, e: &sqlx::Error) -> Error {
    Error::Query(sanitize(&e.to_string(), target))
}

async fn connect(target: &str, options: &Options) -> Result<MySqlConnection, Error> {
    let opts = MySqlConnectOptions::from_str(target.trim())
        .map_err(|e| Error::InvalidTarget(sanitize(&e.to_string(), target)))?;
    let secs = Duration::from_secs(options.timeout_secs.max(1));
    let mut conn = tokio::time::timeout(secs, opts.connect())
        .await
        .map_err(|_| Error::Connect(format!("timed out after {}s", secs.as_secs())))?
        .map_err(|e| Error::Connect(sanitize(&e.to_string(), target)))?;
    sqlx::query("SET SESSION TRANSACTION READ ONLY")
        .execute(&mut conn)
        .await
        .map_err(|e| qerr(target, &e))?;
    Ok(conn)
}

pub(crate) async fn read(target: &str, options: &Options) -> Result<Introspected, Error> {
    let mut conn = connect(target, options).await?;
    read_conn(&mut conn, target, options).await
}

pub(crate) async fn ping(target: &str, options: &Options) -> Result<crate::TestOutcome, Error> {
    let mut conn = connect(target, options).await?;
    let version: String = sqlx::query("SELECT CAST(VERSION() AS CHAR) AS v")
        .fetch_one(&mut conn)
        .await
        .map_err(|e| qerr(target, &e))?
        .try_get("v")
        .map_err(|e| qerr(target, &e))?;
    let rows = sqlx::query(
        "SELECT CAST(schema_name AS CHAR) AS s FROM information_schema.schemata ORDER BY 1",
    )
    .fetch_all(&mut conn)
    .await
    .map_err(|e| qerr(target, &e))?;
    let mut schemas = Vec::new();
    for r in &rows {
        let s = text(r, "s", target)?;
        if !SYSTEM_SCHEMAS.contains(&s.as_str()) {
            schemas.push(s);
        }
    }
    Ok(crate::TestOutcome {
        server: format!("MySQL {version}"),
        schemas,
    })
}

fn text(r: &MySqlRow, col: &str, target: &str) -> Result<String, Error> {
    r.try_get::<String, _>(col).map_err(|e| qerr(target, &e))
}

fn opt(r: &MySqlRow, col: &str, target: &str) -> Result<Option<String>, Error> {
    r.try_get::<Option<String>, _>(col)
        .map_err(|e| qerr(target, &e))
}

fn action(rule: &str) -> Option<String> {
    match rule.to_ascii_uppercase().as_str() {
        "CASCADE" => Some("cascade".to_owned()),
        "SET NULL" => Some("set null".to_owned()),
        "SET DEFAULT" => Some("set default".to_owned()),
        "RESTRICT" => Some("restrict".to_owned()),
        _ => None, // NO ACTION is the default
    }
}

/// `enum('a','b')` -> `["a", "b"]`; `None` when the type is not a well-formed enum.
fn enum_values(column_type: &str) -> Option<Vec<String>> {
    let inner = column_type
        .strip_prefix("enum(")
        .and_then(|r| r.strip_suffix(')'))?;
    let mut values = Vec::new();
    let mut cur = String::new();
    let mut chars = inner.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        match (c, in_str) {
            ('\'', false) => in_str = true,
            ('\'', true) if chars.peek() == Some(&'\'') => {
                cur.push('\'');
                chars.next();
            }
            ('\'', true) => {
                in_str = false;
                values.push(std::mem::take(&mut cur));
            }
            (c, true) => cur.push(c),
            (',', false) => {}
            _ => return None,
        }
    }
    (!in_str).then_some(values)
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn numeric(data_type: &str) -> bool {
    matches!(
        data_type,
        "int"
            | "tinyint"
            | "smallint"
            | "mediumint"
            | "bigint"
            | "decimal"
            | "numeric"
            | "float"
            | "double"
            | "bit"
            | "year"
    )
}

async fn schemas_to_read(
    conn: &mut MySqlConnection,
    target: &str,
    options: &Options,
) -> Result<Vec<String>, Error> {
    if !options.schemas.is_empty() {
        return Ok(options.schemas.clone());
    }
    let current: Option<String> = sqlx::query("SELECT CAST(DATABASE() AS CHAR) AS d")
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| qerr(target, &e))?
        .try_get("d")
        .map_err(|e| qerr(target, &e))?;
    if let Some(db) = current {
        return Ok(vec![db]);
    }
    let rows = sqlx::query(
        "SELECT CAST(schema_name AS CHAR) AS s FROM information_schema.schemata ORDER BY 1",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| qerr(target, &e))?;
    let mut out = Vec::new();
    for r in &rows {
        let s = text(r, "s", target)?;
        if !SYSTEM_SCHEMAS.contains(&s.as_str()) {
            out.push(s);
        }
    }
    Ok(out)
}

async fn read_conn(
    conn: &mut MySqlConnection,
    target: &str,
    options: &Options,
) -> Result<Introspected, Error> {
    let mut db = Introspected::default();
    let schemas = schemas_to_read(conn, target, options).await?;
    let qualify = schemas.len() > 1;
    for schema in &schemas {
        read_schema(conn, target, schema, qualify, &mut db).await?;
    }
    Ok(db)
}

async fn read_schema(
    conn: &mut MySqlConnection,
    target: &str,
    schema: &str,
    qualify: bool,
    db: &mut Introspected,
) -> Result<(), Error> {
    let sch = || qualify.then(|| schema.to_owned());
    let e = |x: sqlx::Error| qerr(target, &x);

    let tables = sqlx::query(
        "SELECT CAST(table_name AS CHAR) AS n, CAST(table_comment AS CHAR) AS c \
         FROM information_schema.tables WHERE table_schema = ? AND table_type = 'BASE TABLE' \
         ORDER BY table_name",
    )
    .bind(schema)
    .fetch_all(&mut *conn)
    .await
    .map_err(e)?;
    let mut pos: HashMap<String, usize> = HashMap::new();
    for t in &tables {
        let name = text(t, "n", target)?;
        let comment = opt(t, "c", target)?.filter(|c| !c.is_empty());
        pos.insert(name.clone(), db.tables.len());
        db.tables.push(Table {
            schema: sch(),
            name,
            columns: Vec::new(),
            indexes: Vec::new(),
            comment,
        });
    }

    let cols = sqlx::query(
        "SELECT CAST(table_name AS CHAR) AS t, CAST(column_name AS CHAR) AS c, \
         CAST(column_type AS CHAR) AS ty, CAST(data_type AS CHAR) AS dt, \
         CAST(is_nullable AS CHAR) AS nul, CAST(column_default AS CHAR) AS d, \
         CAST(extra AS CHAR) AS x, CAST(column_comment AS CHAR) AS cm \
         FROM information_schema.columns WHERE table_schema = ? \
         ORDER BY table_name, ordinal_position",
    )
    .bind(schema)
    .fetch_all(&mut *conn)
    .await
    .map_err(e)?;
    for c in &cols {
        let table = text(c, "t", target)?;
        let Some(&ti) = pos.get(&table) else {
            continue; // a view
        };
        let name = text(c, "c", target)?;
        let column_type = text(c, "ty", target)?;
        let data_type = text(c, "dt", target)?.to_ascii_lowercase();
        let extra = opt(c, "x", target)?
            .unwrap_or_default()
            .to_ascii_lowercase();
        let nullable = text(c, "nul", target)? == "YES";
        let raw_default = opt(c, "d", target)?;

        let ty = if let Some(values) = enum_values(&column_type) {
            let ename = format!("{table}_{name}");
            db.enums.push(EnumType {
                schema: sch(),
                name: ename.clone(),
                values,
            });
            if qualify {
                format!("{schema}.{ename}")
            } else {
                ename
            }
        } else if column_type.eq_ignore_ascii_case("tinyint(1)") {
            "boolean".to_owned()
        } else {
            if data_type == "set" {
                db.warnings.push(format!(
                    "{table}.{name}: SET type kept as `{column_type}` (no DBML equivalent)"
                ));
            }
            column_type.clone()
        };

        let generated_default = extra.contains("default_generated");
        let default = raw_default.map(|d| {
            if extra.contains("generated") && !generated_default {
                return String::new(); // virtual/stored generated column: no default
            }
            if d.eq_ignore_ascii_case("null") {
                String::new() // MariaDB reports `DEFAULT NULL` as the text NULL
            } else if generated_default || d.starts_with('(') || d.starts_with('\'') {
                d // an expression such as CURRENT_TIMESTAMP or (uuid()), or a MariaDB literal
            } else if ty == "boolean" && matches!(d.as_str(), "0" | "1") {
                (d == "1").to_string()
            } else if numeric(&data_type) {
                d
            } else {
                quote(&d)
            }
        });
        db.tables[ti].columns.push(Column {
            name,
            ty,
            not_null: !nullable,
            primary_key: false,
            unique: false,
            auto_increment: extra.contains("auto_increment"),
            default: default.filter(|d| !d.is_empty()),
            comment: opt(c, "cm", target)?.filter(|c| !c.is_empty()),
            check: None,
        });
    }

    let stats = sqlx::query(
        "SELECT CAST(table_name AS CHAR) AS t, CAST(index_name AS CHAR) AS i, non_unique AS nu, \
         CAST(column_name AS CHAR) AS c \
         FROM information_schema.statistics WHERE table_schema = ? \
         ORDER BY table_name, index_name, seq_in_index",
    )
    .bind(schema)
    .fetch_all(&mut *conn)
    .await
    .map_err(e)?;
    // (table, index) -> (unique, columns or None when a part is an expression)
    let mut groups: Vec<(String, String, bool, Vec<Option<String>>)> = Vec::new();
    for r in &stats {
        let table = text(r, "t", target)?;
        let iname = text(r, "i", target)?;
        let non_unique: i64 = r.try_get("nu").map_err(e)?;
        let col = opt(r, "c", target)?;
        match groups.last_mut() {
            Some((t, i, _, cols)) if *t == table && *i == iname => cols.push(col),
            _ => groups.push((table, iname, non_unique == 0, vec![col])),
        }
    }
    for (table, iname, unique, cols) in groups {
        let Some(&ti) = pos.get(&table) else { continue };
        let Some(cols) = cols.into_iter().collect::<Option<Vec<String>>>() else {
            db.warnings.push(format!(
                "{table}: index `{iname}` uses an expression and was skipped"
            ));
            continue;
        };
        let t = &mut db.tables[ti];
        let single = (cols.len() == 1)
            .then(|| t.columns.iter().position(|c| c.name == cols[0]))
            .flatten();
        match (iname.as_str(), single) {
            ("PRIMARY", Some(ci)) => t.columns[ci].primary_key = true,
            ("PRIMARY", None) => t.indexes.push(Index {
                name: None,
                columns: cols,
                unique: false,
                primary_key: true,
            }),
            (_, Some(ci)) if unique => t.columns[ci].unique = true,
            _ => t.indexes.push(Index {
                name: Some(iname),
                columns: cols,
                unique,
                primary_key: false,
            }),
        }
    }

    let fks = sqlx::query(
        "SELECT CAST(k.constraint_name AS CHAR) AS cn, CAST(k.table_name AS CHAR) AS t, \
         CAST(k.column_name AS CHAR) AS c, CAST(k.referenced_table_schema AS CHAR) AS rs, \
         CAST(k.referenced_table_name AS CHAR) AS rt, CAST(k.referenced_column_name AS CHAR) AS rc, \
         CAST(r.delete_rule AS CHAR) AS dr, CAST(r.update_rule AS CHAR) AS ur \
         FROM information_schema.key_column_usage k \
         JOIN information_schema.referential_constraints r \
           ON r.constraint_schema = k.constraint_schema \
          AND r.constraint_name = k.constraint_name AND r.table_name = k.table_name \
         WHERE k.table_schema = ? AND k.referenced_table_name IS NOT NULL \
         ORDER BY k.table_name, k.constraint_name, k.ordinal_position",
    )
    .bind(schema)
    .fetch_all(&mut *conn)
    .await
    .map_err(e)?;
    let mut last: Option<(String, String)> = None;
    let mut fk_names: Vec<(String, String)> = Vec::new();
    for r in &fks {
        let key = (text(r, "t", target)?, text(r, "cn", target)?);
        let col = text(r, "c", target)?;
        let rcol = text(r, "rc", target)?;
        if last.as_ref() == Some(&key) {
            if let Some(fk) = db.refs.last_mut() {
                fk.from_cols.push(col);
                fk.to_cols.push(rcol);
            }
            continue;
        }
        let rs = text(r, "rs", target)?;
        db.refs.push(ForeignKey {
            from_schema: sch(),
            from_table: key.0.clone(),
            from_cols: vec![col],
            to_schema: (qualify || rs != schema).then_some(rs),
            to_table: text(r, "rt", target)?,
            to_cols: vec![rcol],
            on_delete: action(&text(r, "dr", target)?),
            on_update: action(&text(r, "ur", target)?),
        });
        fk_names.push(key.clone());
        last = Some(key);
    }
    // MySQL creates a plain index named after each foreign key; it adds nothing to the diagram.
    for (table, constraint) in &fk_names {
        if let Some(&ti) = pos.get(table) {
            db.tables[ti]
                .indexes
                .retain(|i| i.unique || i.name.as_deref() != Some(constraint.as_str()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_enum_values() {
        assert_eq!(
            enum_values("enum('a','b c','it''s')"),
            Some(vec!["a".into(), "b c".into(), "it's".into()])
        );
        assert_eq!(enum_values("varchar(10)"), None);
        assert_eq!(enum_values("enum('a"), None);
    }

    #[test]
    fn quotes_defaults() {
        assert_eq!(quote("it's"), "'it''s'");
    }
}
