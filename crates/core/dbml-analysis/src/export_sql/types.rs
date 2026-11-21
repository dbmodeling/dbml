//! SQL identifier quoting, string escaping, column type mapping, and default literals.

use super::dialect::SqlDialect;
use schema::EnumDef;

pub(super) fn write_qualified_ident(
    out: &mut String,
    schema_name: Option<&str>,
    ident: &str,
    dialect: SqlDialect,
) {
    if let Some(s) = schema_name.filter(|s| !s.is_empty()) {
        write_single_ident(out, s, dialect);
        out.push('.');
    }
    write_single_ident(out, ident, dialect);
}

pub(super) fn write_single_ident(out: &mut String, ident: &str, dialect: SqlDialect) {
    match dialect {
        SqlDialect::Postgres | SqlDialect::Sqlite | SqlDialect::Oracle => {
            out.push('"');
            for ch in ident.chars() {
                if ch == '"' {
                    out.push_str("\"\"");
                } else {
                    out.push(ch);
                }
            }
            out.push('"');
        }
        SqlDialect::Mysql => {
            out.push('`');
            for ch in ident.chars() {
                if ch == '`' {
                    out.push_str("``");
                } else {
                    out.push(ch);
                }
            }
            out.push('`');
        }
        SqlDialect::SqlServer => {
            out.push('[');
            for ch in ident.chars() {
                if ch == ']' {
                    out.push_str("]]");
                } else {
                    out.push(ch);
                }
            }
            out.push(']');
        }
    }
}

pub(super) fn write_sql_string(out: &mut String, val: &str) {
    out.push('\'');
    for ch in val.chars() {
        if ch == '\'' {
            out.push_str("''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
}

pub(super) fn write_column_type(
    out: &mut String,
    col_name: &str,
    ty: &str,
    enum_def: Option<&EnumDef>,
    dialect: SqlDialect,
) {
    if let Some(edef) = enum_def {
        match dialect {
            SqlDialect::Postgres => {
                write_qualified_ident(out, edef.schema.as_deref(), &edef.name, dialect);
                return;
            }
            SqlDialect::Mysql => {
                out.push_str("ENUM(");
                for (i, v) in edef.values.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    write_sql_string(out, &v.name);
                }
                out.push(')');
                return;
            }
            SqlDialect::Sqlite | SqlDialect::SqlServer | SqlDialect::Oracle => {
                let base = match dialect {
                    SqlDialect::SqlServer => "NVARCHAR(255)",
                    SqlDialect::Oracle => "VARCHAR2(255)",
                    _ => "TEXT",
                };
                out.push_str(base);
                if !edef.values.is_empty() {
                    out.push_str(" CHECK (");
                    write_single_ident(out, col_name, dialect);
                    out.push_str(" IN (");
                    for (i, v) in edef.values.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        write_sql_string(out, &v.name);
                    }
                    out.push_str("))");
                }
                return;
            }
        }
    }

    let (base, params) = match ty.find('(') {
        Some(idx) => (ty[..idx].trim(), Some(&ty[idx..])),
        None => (ty.trim(), None),
    };
    let low = base.to_ascii_lowercase();
    match (dialect, low.as_str()) {
        (SqlDialect::Postgres, "int" | "integer") => out.push_str("INTEGER"),
        (SqlDialect::Postgres | SqlDialect::SqlServer, "bigint") => out.push_str("BIGINT"),
        (SqlDialect::Postgres, "bool" | "boolean") => out.push_str("BOOLEAN"),
        (SqlDialect::Postgres, "json" | "jsonb") => out.push_str("JSONB"),
        (SqlDialect::Postgres, "uuid") => out.push_str("UUID"),
        (SqlDialect::Postgres, "text") => out.push_str("TEXT"),
        (SqlDialect::Postgres | SqlDialect::Oracle, "timestamp" | "datetime") => {
            out.push_str("TIMESTAMP");
        }

        (SqlDialect::Mysql, "bool" | "boolean") => out.push_str("TINYINT(1)"),
        (SqlDialect::Mysql, "uuid") => out.push_str("CHAR(36)"),
        (SqlDialect::Mysql, "json" | "jsonb") => out.push_str("JSON"),

        (
            SqlDialect::Sqlite,
            "int" | "integer" | "bigint" | "smallint" | "tinyint" | "bool" | "boolean",
        ) => {
            out.push_str("INTEGER");
        }
        (
            SqlDialect::Sqlite,
            "varchar" | "char" | "text" | "uuid" | "timestamp" | "datetime" | "json" | "jsonb",
        ) => {
            out.push_str("TEXT");
        }

        (SqlDialect::SqlServer, "int" | "integer") => out.push_str("INT"),
        (SqlDialect::SqlServer, "bool" | "boolean") => out.push_str("BIT"),
        (SqlDialect::SqlServer, "varchar") => {
            out.push_str("NVARCHAR");
            out.push_str(params.unwrap_or("(255)"));
        }
        (SqlDialect::SqlServer, "text" | "json" | "jsonb") => out.push_str("NVARCHAR(MAX)"),
        (SqlDialect::SqlServer, "timestamp" | "datetime") => out.push_str("DATETIME2"),
        (SqlDialect::SqlServer, "uuid") => out.push_str("UNIQUEIDENTIFIER"),

        (SqlDialect::Oracle, "int" | "integer") => out.push_str("NUMBER(10)"),
        (SqlDialect::Oracle, "bigint") => out.push_str("NUMBER(19)"),
        (SqlDialect::Oracle, "bool" | "boolean") => out.push_str("NUMBER(1)"),
        (SqlDialect::Oracle, "varchar") => {
            out.push_str("VARCHAR2");
            out.push_str(params.unwrap_or("(255)"));
        }
        (SqlDialect::Oracle, "text") => out.push_str("CLOB"),
        (SqlDialect::Oracle, "json" | "jsonb") => {
            out.push_str("CLOB CHECK (");
            write_single_ident(out, col_name, dialect);
            out.push_str(" IS JSON)");
        }
        (SqlDialect::Oracle, "uuid") => out.push_str("RAW(16)"),

        _ => {
            out.push_str(&base.to_ascii_uppercase());
            if let Some(p) = params {
                out.push_str(p);
            }
        }
    }
}

pub(super) fn write_default_literal(out: &mut String, raw: &str, dialect: SqlDialect) {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("false") {
        let is_true = trimmed.eq_ignore_ascii_case("true");
        match dialect {
            SqlDialect::Postgres | SqlDialect::Mysql => {
                out.push_str(if is_true { "TRUE" } else { "FALSE" });
            }
            SqlDialect::Sqlite | SqlDialect::SqlServer | SqlDialect::Oracle => {
                out.push(if is_true { '1' } else { '0' });
            }
        }
        return;
    }
    if let Some(expr) = trimmed.strip_prefix('`').and_then(|s| s.strip_suffix('`')) {
        if expr.trim().eq_ignore_ascii_case("now()") {
            match dialect {
                SqlDialect::SqlServer => out.push_str("GETDATE()"),
                SqlDialect::Oracle => out.push_str("SYSTIMESTAMP"),
                _ => out.push_str("CURRENT_TIMESTAMP"),
            }
        } else {
            out.push_str(expr);
        }
        return;
    }
    out.push_str(trimmed);
}
