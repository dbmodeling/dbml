//! PostgreSQL introspector: `pg_catalog` queries over a read-only connection.

use crate::model::{Column, EnumType, ForeignKey, Index, Introspected, Table};
use crate::{Error, Options, sanitize};
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, PgConnection, Row};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

// Constant fragment, never user input: safe to splice into the catalog queries.
const SYSTEM_SCHEMAS: &str = "n.nspname NOT IN ('pg_catalog', 'information_schema') \
     AND n.nspname NOT LIKE 'pg\\_toast%' AND n.nspname NOT LIKE 'pg\\_temp%'";

fn qerr(target: &str, e: &sqlx::Error) -> Error {
    Error::Query(sanitize(&e.to_string(), target))
}

async fn connect(target: &str, options: &Options) -> Result<PgConnection, Error> {
    let opts = PgConnectOptions::from_str(target.trim())
        .map_err(|e| Error::InvalidTarget(sanitize(&e.to_string(), target)))?
        .options([("default_transaction_read_only", "on")]);
    let secs = Duration::from_secs(options.timeout_secs.max(1));
    let conn = tokio::time::timeout(secs, opts.connect())
        .await
        .map_err(|_| Error::Connect(format!("timed out after {}s", secs.as_secs())))?
        .map_err(|e| Error::Connect(sanitize(&e.to_string(), target)))?;
    Ok(conn)
}

pub(crate) async fn read(target: &str, options: &Options) -> Result<Introspected, Error> {
    let mut conn = connect(target, options).await?;
    read_conn(&mut conn, target, options).await
}

pub(crate) async fn ping(target: &str, options: &Options) -> Result<crate::TestOutcome, Error> {
    let mut conn = connect(target, options).await?;
    let e = |x: sqlx::Error| qerr(target, &x);
    let version: String = sqlx::query("SELECT version() AS v")
        .fetch_one(&mut conn)
        .await
        .map_err(e)?
        .try_get("v")
        .map_err(e)?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT n.nspname FROM pg_namespace n WHERE {SYSTEM_SCHEMAS} ORDER BY 1"
    )))
    .fetch_all(&mut conn)
    .await
    .map_err(e)?;
    let mut schemas = Vec::new();
    for r in &rows {
        schemas.push(r.try_get::<String, _>("nspname").map_err(e)?);
    }
    Ok(crate::TestOutcome {
        server: version.split(" on ").next().unwrap_or(&version).to_owned(),
        schemas,
    })
}

/// `format_type` output to the DBML spelling.
fn dbml_type(ty: &str) -> String {
    let (base, suffix) = ty.find(['(', '[']).map_or((ty, ""), |i| ty.split_at(i));
    let mapped = match base.trim() {
        "character varying" => "varchar",
        "character" => "char",
        "timestamp without time zone" => "timestamp",
        "timestamp with time zone" => "timestamptz",
        "time without time zone" => "time",
        "time with time zone" => "timetz",
        "double precision" => "double",
        other => other,
    };
    format!("{mapped}{suffix}")
}

/// `'member'::text` -> `'member'`: drops the type cast PostgreSQL appends to literal defaults.
/// Anything that is not a plain quoted literal followed by casts is returned unchanged.
fn clean_default(raw: &str) -> String {
    let t = raw.trim();
    if !t.starts_with('\'') {
        return t.to_owned();
    }
    let bytes = t.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if bytes.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            let (lit, rest) = t.split_at(i + 1);
            let casts_only = rest.is_empty()
                || (rest.starts_with("::")
                    && rest.chars().all(|c| {
                        c.is_ascii_alphanumeric()
                            || matches!(c, ':' | '_' | ' ' | '[' | ']' | '.' | '"')
                    }));
            return if casts_only {
                lit.to_owned()
            } else {
                t.to_owned()
            };
        }
        i += 1;
    }
    t.to_owned()
}

fn action(c: &str) -> Option<String> {
    match c {
        "c" => Some("cascade".to_owned()),
        "n" => Some("set null".to_owned()),
        "d" => Some("set default".to_owned()),
        "r" => Some("restrict".to_owned()),
        _ => None, // `a` = no action, the default
    }
}

fn schema_of(s: String) -> Option<String> {
    (s != "public").then_some(s)
}

async fn read_conn(
    conn: &mut PgConnection,
    target: &str,
    options: &Options,
) -> Result<Introspected, Error> {
    let mut db = Introspected::default();
    let schemas: Option<&[String]> = (!options.schemas.is_empty()).then_some(&options.schemas[..]);
    let e = |x: sqlx::Error| qerr(target, &x);

    let enums = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT n.nspname, t.typname, array_agg(e.enumlabel ORDER BY e.enumsortorder)::text[] AS vals \
         FROM pg_type t JOIN pg_enum e ON e.enumtypid = t.oid \
         JOIN pg_namespace n ON n.oid = t.typnamespace \
         WHERE {SYSTEM_SCHEMAS} AND ($1::text[] IS NULL OR n.nspname = ANY($1)) \
         GROUP BY 1, 2 ORDER BY 1, 2"
    )))
    .bind(schemas)
    .fetch_all(&mut *conn)
    .await
    .map_err(e)?;
    for r in &enums {
        db.enums.push(EnumType {
            schema: schema_of(r.try_get("nspname").map_err(e)?),
            name: r.try_get("typname").map_err(e)?,
            values: r.try_get("vals").map_err(e)?,
        });
    }

    let tables = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT n.nspname, c.relname, c.oid::int8 AS oid, obj_description(c.oid, 'pg_class') AS cmt \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE c.relkind IN ('r', 'p') AND NOT c.relispartition AND {SYSTEM_SCHEMAS} \
         AND ($1::text[] IS NULL OR n.nspname = ANY($1)) ORDER BY 1, 2"
    )))
    .bind(schemas)
    .fetch_all(&mut *conn)
    .await
    .map_err(e)?;

    // (schema, table) -> index into db.tables
    let mut pos: HashMap<(String, String), usize> = HashMap::new();
    for t in &tables {
        let schema: String = t.try_get("nspname").map_err(e)?;
        let name: String = t.try_get("relname").map_err(e)?;
        let oid: i64 = t.try_get("oid").map_err(e)?;
        let comment: Option<String> = t.try_get("cmt").map_err(e)?;
        let cols = sqlx::query(
            "SELECT a.attname, format_type(a.atttypid, a.atttypmod) AS ty, a.attnotnull, \
             pg_get_expr(d.adbin, d.adrelid) AS dflt, a.attidentity::text AS ident, \
             a.attgenerated::text AS generated, col_description(a.attrelid, a.attnum) AS cmt \
             FROM pg_attribute a \
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             WHERE a.attrelid = $1::int8::oid AND a.attnum > 0 AND NOT a.attisdropped \
             ORDER BY a.attnum",
        )
        .bind(oid)
        .fetch_all(&mut *conn)
        .await
        .map_err(e)?;
        let mut columns = Vec::new();
        for c in &cols {
            let default: Option<String> = c.try_get("dflt").map_err(e)?;
            let ident: String = c.try_get("ident").map_err(e)?;
            let generated: String = c.try_get("generated").map_err(e)?;
            let serial = default
                .as_deref()
                .is_some_and(|d| d.starts_with("nextval('"));
            let identity = matches!(ident.as_str(), "a" | "d");
            let default = if serial || identity || !generated.is_empty() {
                None
            } else {
                default.as_deref().map(clean_default)
            };
            columns.push(Column {
                name: c.try_get("attname").map_err(e)?,
                ty: dbml_type(&c.try_get::<String, _>("ty").map_err(e)?),
                not_null: c.try_get("attnotnull").map_err(e)?,
                primary_key: false,
                unique: false,
                auto_increment: serial || identity,
                default,
                comment: c.try_get("cmt").map_err(e)?,
                check: None,
            });
        }
        pos.insert((schema.clone(), name.clone()), db.tables.len());
        db.tables.push(Table {
            schema: schema_of(schema),
            name,
            columns,
            indexes: Vec::new(),
            comment,
        });
    }

    let constraints = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT con.contype::text AS kind, con.conname, n.nspname, rel.relname, \
           (SELECT array_agg(a.attname::text ORDER BY k.ord) \
              FROM unnest(con.conkey) WITH ORDINALITY k(attnum, ord) \
              JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.attnum) AS cols, \
           fns.nspname AS fnsp, frel.relname AS frel, \
           (SELECT array_agg(a.attname::text ORDER BY k.ord) \
              FROM unnest(con.confkey) WITH ORDINALITY k(attnum, ord) \
              JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.attnum) AS fcols, \
           con.confdeltype::text AS del, con.confupdtype::text AS upd \
         FROM pg_constraint con \
         JOIN pg_class rel ON rel.oid = con.conrelid \
         JOIN pg_namespace n ON n.oid = rel.relnamespace \
         LEFT JOIN pg_class frel ON frel.oid = con.confrelid \
         LEFT JOIN pg_namespace fns ON fns.oid = frel.relnamespace \
         WHERE con.contype IN ('p', 'u', 'f') AND {SYSTEM_SCHEMAS} \
         AND ($1::text[] IS NULL OR n.nspname = ANY($1)) ORDER BY 3, 4, con.conname"
    )))
    .bind(schemas)
    .fetch_all(&mut *conn)
    .await
    .map_err(e)?;
    for r in &constraints {
        let kind: String = r.try_get("kind").map_err(e)?;
        let schema: String = r.try_get("nspname").map_err(e)?;
        let table: String = r.try_get("relname").map_err(e)?;
        let cname: String = r.try_get("conname").map_err(e)?;
        let cols: Vec<String> = r.try_get("cols").map_err(e)?;
        let Some(&ti) = pos.get(&(schema.clone(), table.clone())) else {
            continue; // partition child or table outside the selected schemas
        };
        match kind.as_str() {
            "p" | "u" if cols.len() == 1 => {
                if let Some(c) = db.tables[ti].columns.iter_mut().find(|c| c.name == cols[0]) {
                    if kind == "p" {
                        c.primary_key = true;
                    } else {
                        c.unique = true;
                    }
                }
            }
            "p" | "u" => db.tables[ti].indexes.push(Index {
                name: (kind == "u").then_some(cname),
                columns: cols,
                unique: kind == "u",
                primary_key: kind == "p",
            }),
            _ => {
                let fcols: Vec<String> = r.try_get("fcols").map_err(e)?;
                let fschema: String = r.try_get("fnsp").map_err(e)?;
                let del: String = r.try_get("del").map_err(e)?;
                let upd: String = r.try_get("upd").map_err(e)?;
                db.refs.push(ForeignKey {
                    from_schema: schema_of(schema),
                    from_table: table,
                    from_cols: cols,
                    to_schema: schema_of(fschema),
                    to_table: r.try_get("frel").map_err(e)?,
                    to_cols: fcols,
                    on_delete: action(&del),
                    on_update: action(&upd),
                });
            }
        }
    }

    let idx = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT n.nspname, t.relname AS tbl, i.relname AS iname, ix.indisunique, \
           (SELECT array_agg(pg_get_indexdef(ix.indexrelid, k.n, true) ORDER BY k.n) \
              FROM generate_series(1, ix.indnkeyatts::int) k(n)) AS cols \
         FROM pg_index ix \
         JOIN pg_class i ON i.oid = ix.indexrelid \
         JOIN pg_class t ON t.oid = ix.indrelid \
         JOIN pg_namespace n ON n.oid = t.relnamespace \
         WHERE ix.indisvalid AND {SYSTEM_SCHEMAS} \
         AND ($1::text[] IS NULL OR n.nspname = ANY($1)) \
         AND NOT EXISTS (SELECT 1 FROM pg_constraint c \
                         WHERE c.conindid = ix.indexrelid AND c.contype IN ('p', 'u')) \
         ORDER BY 1, 2, 3"
    )))
    .bind(schemas)
    .fetch_all(&mut *conn)
    .await
    .map_err(e)?;
    for r in &idx {
        let schema: String = r.try_get("nspname").map_err(e)?;
        let table: String = r.try_get("tbl").map_err(e)?;
        let iname: String = r.try_get("iname").map_err(e)?;
        let Some(&ti) = pos.get(&(schema, table.clone())) else {
            continue;
        };
        let cols: Vec<String> = r.try_get("cols").map_err(e)?;
        let known = |c: &str| db.tables[ti].columns.iter().any(|x| x.name == c);
        // `pg_get_indexdef` quotes odd identifiers; strip the quotes before matching.
        let cols: Vec<String> = cols
            .into_iter()
            .map(|c| c.trim_matches('"').to_owned())
            .collect();
        if cols.iter().any(|c| !known(c)) {
            db.warnings.push(format!(
                "{table}: index `{iname}` uses an expression and was skipped"
            ));
            continue;
        }
        db.tables[ti].indexes.push(Index {
            name: Some(iname),
            columns: cols,
            unique: r.try_get("indisunique").map_err(e)?,
            primary_key: false,
        });
    }
    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_casts_from_literal_defaults_only() {
        assert_eq!(clean_default("'member'::text"), "'member'");
        assert_eq!(clean_default("'a''b'::character varying"), "'a''b'");
        assert_eq!(clean_default("'{}'::text[]"), "'{}'");
        assert_eq!(clean_default("'new'::dbi_a.status"), "'new'");
        assert_eq!(clean_default("'new'::\"My Schema\".\"st\""), "'new'");
        assert_eq!(clean_default("'x'"), "'x'");
        assert_eq!(clean_default("now()"), "now()");
        assert_eq!(clean_default("0"), "0");
        assert_eq!(
            clean_default("('a'::text || 'b'::text)"),
            "('a'::text || 'b'::text)"
        );
        assert_eq!(
            clean_default("'a'::text || 'b'::text"),
            "'a'::text || 'b'::text"
        );
    }

    #[test]
    fn maps_type_spellings() {
        assert_eq!(dbml_type("character varying(255)"), "varchar(255)");
        assert_eq!(dbml_type("timestamp without time zone"), "timestamp");
        assert_eq!(dbml_type("double precision"), "double");
        assert_eq!(dbml_type("text[]"), "text[]");
    }
}
