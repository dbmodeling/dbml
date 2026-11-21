//! SQL Server introspector: `sys.*` catalog views over a TDS connection (`tiberius`).
//!
//! SQL Server has no read-only session mode, so this module only ever issues `SELECT` on catalog
//! views and sets `ApplicationIntent=ReadOnly`. Use a login with `db_datareader` for defence in
//! depth.

use crate::dbml::strip_outer_parens;
use crate::model::{Column, ForeignKey, Index, Introspected, Table};
use crate::{Error, Options, sanitize};
use std::collections::HashMap;
use std::time::Duration;
use tiberius::{AuthMethod, Client, Config, EncryptionLevel, Row};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

type Conn = Client<Compat<TcpStream>>;

/// `(schema, table, index, unique, primary key, filtered, columns)` while grouping index rows.
type IndexGroup = (String, String, String, bool, bool, bool, Vec<String>);

struct Target {
    config: Config,
}

/// Parses `sqlserver://user:pass@host[:port]/database?trustServerCertificate=true&encrypt=false`.
fn parse(target: &str) -> Result<Target, Error> {
    let u = url::Url::parse(target.trim())
        .map_err(|e| Error::InvalidTarget(sanitize(&e.to_string(), target)))?;
    let mut config = Config::new();
    config.host(u.host_str().unwrap_or("localhost"));
    config.port(u.port().unwrap_or(1433));
    let db = u.path().trim_start_matches('/');
    if !db.is_empty() {
        config.database(db);
    }
    let decode = |s: &str| percent_decode(s);
    config.authentication(AuthMethod::sql_server(
        decode(u.username()),
        decode(u.password().unwrap_or("")),
    ));
    config.readonly(true);
    for (k, v) in u.query_pairs() {
        match (
            k.to_ascii_lowercase().as_str(),
            v.to_ascii_lowercase().as_str(),
        ) {
            ("trustservercertificate", "true" | "yes" | "1") => config.trust_cert(),
            ("encrypt", "false" | "no" | "0") => config.encryption(EncryptionLevel::Off),
            ("encrypt", "true" | "yes" | "1") => config.encryption(EncryptionLevel::Required),
            _ => {}
        }
    }
    Ok(Target { config })
}

fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

async fn connect(target: &str, options: &Options) -> Result<Conn, Error> {
    let t = parse(target)?;
    let secs = Duration::from_secs(options.timeout_secs.max(1));
    let timed_out = || Error::Connect(format!("timed out after {}s", secs.as_secs()));
    let tcp = tokio::time::timeout(secs, TcpStream::connect(t.config.get_addr()))
        .await
        .map_err(|_| timed_out())?
        .map_err(|e| Error::Connect(sanitize(&e.to_string(), target)))?;
    tcp.set_nodelay(true)
        .map_err(|e| Error::Connect(e.to_string()))?;
    let conn = tokio::time::timeout(
        secs,
        Box::pin(Client::connect(t.config, tcp.compat_write())),
    )
    .await
    .map_err(|_| timed_out())?
    .map_err(|e| {
        let msg = sanitize(&e.to_string(), target);
        if msg.to_ascii_lowercase().contains("certificate") {
            Error::Connect(format!(
                "{msg} (for a self-signed certificate add ?trustServerCertificate=true)"
            ))
        } else {
            Error::from_connect(msg)
        }
    })?;
    Ok(conn)
}

pub(crate) async fn read(target: &str, options: &Options) -> Result<Introspected, Error> {
    let mut conn = connect(target, options).await?;
    read_conn(&mut conn, target, options).await
}

pub(crate) async fn ping(target: &str, options: &Options) -> Result<crate::TestOutcome, Error> {
    let mut conn = connect(target, options).await?;
    let version = rows(
        &mut conn,
        "SELECT CAST(@@VERSION AS NVARCHAR(512)) AS v",
        target,
    )
    .await?
    .first()
    .map(|r| s(r, "v", target))
    .transpose()?
    .unwrap_or_default();
    let mut schemas = Vec::new();
    for r in &rows(
        &mut conn,
        "SELECT name FROM sys.schemas \
         WHERE name NOT IN ('sys', 'INFORMATION_SCHEMA', 'guest') AND name NOT LIKE 'db[_]%' \
         ORDER BY name",
        target,
    )
    .await?
    {
        schemas.push(s(r, "name", target)?);
    }
    Ok(crate::TestOutcome {
        server: version.lines().next().unwrap_or_default().trim().to_owned(),
        schemas,
    })
}

fn qerr(target: &str, e: &tiberius::error::Error) -> Error {
    Error::Query(sanitize(&e.to_string(), target))
}

async fn rows(conn: &mut Conn, sql: &str, target: &str) -> Result<Vec<Row>, Error> {
    conn.simple_query(sql)
        .await
        .map_err(|e| qerr(target, &e))?
        .into_first_result()
        .await
        .map_err(|e| qerr(target, &e))
}

fn s(r: &Row, col: &str, target: &str) -> Result<String, Error> {
    r.try_get::<&str, _>(col)
        .map_err(|e| qerr(target, &e))
        .map(|v| v.unwrap_or_default().to_owned())
}

fn so(r: &Row, col: &str, target: &str) -> Result<Option<String>, Error> {
    r.try_get::<&str, _>(col)
        .map_err(|e| qerr(target, &e))
        .map(|v| v.map(str::to_owned))
}

fn b(r: &Row, col: &str, target: &str) -> Result<bool, Error> {
    r.try_get::<bool, _>(col)
        .map_err(|e| qerr(target, &e))
        .map(|v| v.unwrap_or(false))
}

fn schema_of(s: String) -> Option<String> {
    (s != "dbo").then_some(s)
}

/// SQL Server type to the DBML spelling used by the DDL importer (`sql-import`).
fn dbml_type(name: &str, max_length: i16, precision: u8, scale: u8) -> String {
    let n = name.to_ascii_lowercase();
    let unicode = n.starts_with('n');
    let len = if unicode && max_length > 0 {
        max_length / 2
    } else {
        max_length
    };
    match n.as_str() {
        "nvarchar" | "varchar" if max_length == -1 => "text".to_owned(),
        "ntext" | "text" => "text".to_owned(),
        "nvarchar" | "varchar" => format!("varchar({len})"),
        "nchar" | "char" => format!("char({len})"),
        "varbinary" | "binary" if max_length == -1 => format!("{n}(max)"),
        "varbinary" | "binary" => format!("{n}({max_length})"),
        "decimal" | "numeric" => format!("{n}({precision},{scale})"),
        "datetime2" | "datetime" | "smalldatetime" => "timestamp".to_owned(),
        "datetimeoffset" => "timestamptz".to_owned(),
        "bit" => "boolean".to_owned(),
        "uniqueidentifier" => "uuid".to_owned(),
        _ => n,
    }
}

/// `((0))` -> `0`, `(N'x')` -> `'x'`, `(getdate())` -> `getdate()`.
fn clean_default(raw: &str) -> String {
    let t = strip_outer_parens(raw);
    t.strip_prefix('N')
        .filter(|r| r.starts_with('\''))
        .unwrap_or(t)
        .to_owned()
}

fn action(a: u8) -> Option<String> {
    match a {
        1 => Some("cascade".to_owned()),
        2 => Some("set null".to_owned()),
        3 => Some("set default".to_owned()),
        _ => None,
    }
}

async fn read_conn(
    conn: &mut Conn,
    target: &str,
    options: &Options,
) -> Result<Introspected, Error> {
    let mut db = Introspected::default();
    let wanted = |schema: &str| {
        options.schemas.is_empty()
            || options
                .schemas
                .iter()
                .any(|x| x.eq_ignore_ascii_case(schema))
    };
    // (schema, table) -> index into db.tables
    let mut pos: HashMap<(String, String), usize> = HashMap::new();

    for r in &rows(
        conn,
        "SELECT s.name AS sch, t.name AS tbl, CAST(ep.value AS NVARCHAR(4000)) AS cmt \
         FROM sys.tables t JOIN sys.schemas s ON s.schema_id = t.schema_id \
         LEFT JOIN sys.extended_properties ep ON ep.class = 1 AND ep.major_id = t.object_id \
           AND ep.minor_id = 0 AND ep.name = 'MS_Description' \
         WHERE t.is_ms_shipped = 0 ORDER BY s.name, t.name",
        target,
    )
    .await?
    {
        let sch = s(r, "sch", target)?;
        if !wanted(&sch) {
            continue;
        }
        let name = s(r, "tbl", target)?;
        pos.insert((sch.clone(), name.clone()), db.tables.len());
        db.tables.push(Table {
            schema: schema_of(sch),
            name,
            columns: Vec::new(),
            indexes: Vec::new(),
            comment: so(r, "cmt", target)?.filter(|c| !c.is_empty()),
        });
    }

    for r in &rows(
        conn,
        "SELECT s.name AS sch, t.name AS tbl, c.name AS col, ty.name AS tyname, \
           c.max_length AS maxlen, c.precision AS prec, c.scale AS scl, \
           c.is_nullable AS nul, c.is_identity AS idn, c.is_computed AS comp, \
           dc.definition AS dflt, CAST(ep.value AS NVARCHAR(4000)) AS cmt \
         FROM sys.columns c \
         JOIN sys.tables t ON t.object_id = c.object_id \
         JOIN sys.schemas s ON s.schema_id = t.schema_id \
         JOIN sys.types ty ON ty.user_type_id = c.user_type_id \
         LEFT JOIN sys.default_constraints dc ON dc.object_id = c.default_object_id \
         LEFT JOIN sys.extended_properties ep ON ep.class = 1 AND ep.major_id = c.object_id \
           AND ep.minor_id = c.column_id AND ep.name = 'MS_Description' \
         WHERE t.is_ms_shipped = 0 ORDER BY s.name, t.name, c.column_id",
        target,
    )
    .await?
    {
        let Some(&ti) = pos.get(&(s(r, "sch", target)?, s(r, "tbl", target)?)) else {
            continue;
        };
        let maxlen = r
            .try_get::<i16, _>("maxlen")
            .map_err(|e| qerr(target, &e))?
            .unwrap_or(0);
        let prec = r
            .try_get::<u8, _>("prec")
            .map_err(|e| qerr(target, &e))?
            .unwrap_or(0);
        let scl = r
            .try_get::<u8, _>("scl")
            .map_err(|e| qerr(target, &e))?
            .unwrap_or(0);
        let computed = b(r, "comp", target)?;
        let ty = dbml_type(&s(r, "tyname", target)?, maxlen, prec, scl);
        db.tables[ti].columns.push(Column {
            name: s(r, "col", target)?,
            ty: ty.clone(),
            not_null: !b(r, "nul", target)?,
            primary_key: false,
            unique: false,
            auto_increment: b(r, "idn", target)?,
            default: so(r, "dflt", target)?
                .filter(|_| !computed)
                .map(|d| clean_default(&d))
                .map(|d| match (ty.as_str(), d.as_str()) {
                    ("boolean", "0") => "false".to_owned(),
                    ("boolean", "1") => "true".to_owned(),
                    _ => d,
                }),
            comment: so(r, "cmt", target)?.filter(|c| !c.is_empty()),
            check: None,
        });
    }

    let idx_rows = rows(
        conn,
        "SELECT s.name AS sch, t.name AS tbl, i.name AS idx, i.is_unique AS uq, \
           i.is_primary_key AS pk, i.has_filter AS filt, c.name AS col \
         FROM sys.indexes i \
         JOIN sys.tables t ON t.object_id = i.object_id \
         JOIN sys.schemas s ON s.schema_id = t.schema_id \
         JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
         JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
         WHERE t.is_ms_shipped = 0 AND i.type > 0 AND ic.is_included_column = 0 \
         ORDER BY s.name, t.name, i.index_id, ic.key_ordinal",
        target,
    )
    .await?;
    // (schema, table, index, unique, pk, filtered, columns)
    let mut groups: Vec<IndexGroup> = Vec::new();
    for r in &idx_rows {
        let key = (
            s(r, "sch", target)?,
            s(r, "tbl", target)?,
            s(r, "idx", target)?,
        );
        let col = s(r, "col", target)?;
        match groups.last_mut() {
            Some(g) if (g.0.as_str(), g.1.as_str(), g.2.as_str()) == (&key.0, &key.1, &key.2) => {
                g.6.push(col);
            }
            _ => groups.push((
                key.0,
                key.1,
                key.2,
                b(r, "uq", target)?,
                b(r, "pk", target)?,
                b(r, "filt", target)?,
                vec![col],
            )),
        }
    }
    for (sch, table, iname, unique, pk, filtered, cols) in groups {
        let Some(&ti) = pos.get(&(sch, table)) else {
            continue;
        };
        let t = &mut db.tables[ti];
        let single = (cols.len() == 1)
            .then(|| t.columns.iter().position(|c| c.name == cols[0]))
            .flatten();
        match single {
            Some(ci) if pk => t.columns[ci].primary_key = true,
            Some(ci) if unique && !filtered => t.columns[ci].unique = true,
            _ => t.indexes.push(Index {
                name: (!pk).then_some(iname),
                columns: cols,
                unique: unique && !pk,
                primary_key: pk,
            }),
        }
    }

    let mut last: Option<(String, String, String)> = None;
    for r in &rows(
        conn,
        "SELECT fk.name AS fkname, ps.name AS psch, pt.name AS ptbl, pc.name AS pcol, \
           rs.name AS rsch, rt.name AS rtbl, rc.name AS rcol, \
           fk.delete_referential_action AS del, fk.update_referential_action AS upd \
         FROM sys.foreign_keys fk \
         JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id \
         JOIN sys.tables pt ON pt.object_id = fk.parent_object_id \
         JOIN sys.schemas ps ON ps.schema_id = pt.schema_id \
         JOIN sys.columns pc ON pc.object_id = fkc.parent_object_id AND pc.column_id = fkc.parent_column_id \
         JOIN sys.tables rt ON rt.object_id = fk.referenced_object_id \
         JOIN sys.schemas rs ON rs.schema_id = rt.schema_id \
         JOIN sys.columns rc ON rc.object_id = fkc.referenced_object_id AND rc.column_id = fkc.referenced_column_id \
         WHERE pt.is_ms_shipped = 0 \
         ORDER BY ps.name, pt.name, fk.name, fkc.constraint_column_id",
        target,
    )
    .await?
    {
        let psch = s(r, "psch", target)?;
        if !wanted(&psch) {
            continue;
        }
        let key = (psch.clone(), s(r, "ptbl", target)?, s(r, "fkname", target)?);
        let col = s(r, "pcol", target)?;
        let rcol = s(r, "rcol", target)?;
        if last.as_ref() == Some(&key) {
            if let Some(fk) = db.refs.last_mut() {
                fk.from_cols.push(col);
                fk.to_cols.push(rcol);
            }
            continue;
        }
        db.refs.push(ForeignKey {
            from_schema: schema_of(psch),
            from_table: key.1.clone(),
            from_cols: vec![col],
            to_schema: schema_of(s(r, "rsch", target)?),
            to_table: s(r, "rtbl", target)?,
            to_cols: vec![rcol],
            on_delete: action(r.try_get::<u8, _>("del").map_err(|e| qerr(target, &e))?.unwrap_or(0)),
            on_update: action(r.try_get::<u8, _>("upd").map_err(|e| qerr(target, &e))?.unwrap_or(0)),
        });
        last = Some(key);
    }
    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_types() {
        assert_eq!(dbml_type("nvarchar", 100, 0, 0), "varchar(50)");
        assert_eq!(dbml_type("varchar", 20, 0, 0), "varchar(20)");
        assert_eq!(dbml_type("nvarchar", -1, 0, 0), "text");
        assert_eq!(dbml_type("decimal", 9, 10, 2), "decimal(10,2)");
        assert_eq!(dbml_type("bit", 1, 0, 0), "boolean");
        assert_eq!(dbml_type("datetime2", 8, 27, 7), "timestamp");
        assert_eq!(dbml_type("uniqueidentifier", 16, 0, 0), "uuid");
        assert_eq!(dbml_type("int", 4, 10, 0), "int");
    }

    #[test]
    fn cleans_defaults() {
        assert_eq!(clean_default("((0))"), "0");
        assert_eq!(clean_default("(N'member')"), "'member'");
        assert_eq!(clean_default("('x')"), "'x'");
        assert_eq!(clean_default("(getdate())"), "getdate()");
    }

    #[test]
    fn parses_urls_and_options() {
        assert!(
            parse("sqlserver://sa:Secret%5F123%21@localhost:51433/app?trustServerCertificate=true")
                .is_ok()
        );
        assert!(parse("not a url").is_err());
        assert_eq!(percent_decode("Secret%5F123%21"), "Secret_123!");
        assert_eq!(percent_decode("a%26b%3Dc+d"), "a&b=c+d");
    }
}
