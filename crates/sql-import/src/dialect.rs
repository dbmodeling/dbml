//! SQL dialects understood by the importer.

use std::fmt;
use std::str::FromStr;

/// Source SQL dialect of the DDL being imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SqlDialect {
    /// PostgreSQL (`pg_dump --schema-only`).
    Postgres,
    /// MySQL / MariaDB (`mysqldump --no-data`).
    Mysql,
    /// SQLite (`.schema`).
    Sqlite,
    /// Microsoft SQL Server (T-SQL, `[ident]`, `IDENTITY`, `GO`).
    SqlServer,
    /// Oracle Database (`"IDENT"`, `VARCHAR2`, `NUMBER`, `/`).
    Oracle,
}

impl SqlDialect {
    /// Canonical lowercase name, also accepted by [`FromStr`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Mysql => "mysql",
            Self::Sqlite => "sqlite",
            Self::SqlServer => "sqlserver",
            Self::Oracle => "oracle",
        }
    }
}

impl fmt::Display for SqlDialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when a dialect name is not recognised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownDialect(pub String);

impl fmt::Display for UnknownDialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown SQL dialect `{}` (expected postgres, mysql, sqlite, sqlserver or oracle)",
            self.0
        )
    }
}

impl std::error::Error for UnknownDialect {}

impl FromStr for SqlDialect {
    type Err = UnknownDialect;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" | "pg" => Ok(Self::Postgres),
            "mysql" | "mariadb" => Ok(Self::Mysql),
            "sqlite" | "sqlite3" => Ok(Self::Sqlite),
            "sqlserver" | "mssql" | "tsql" => Ok(Self::SqlServer),
            "oracle" | "ora" => Ok(Self::Oracle),
            _ => Err(UnknownDialect(s.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_should_parse_aliases_case_insensitively() {
        let parsed: Vec<_> = ["PG", "PostgreSQL", "MariaDB", "MySQL", "SQLite3"]
            .iter()
            .filter_map(|s| s.parse::<SqlDialect>().ok())
            .collect();
        assert_eq!(
            parsed,
            [
                SqlDialect::Postgres,
                SqlDialect::Postgres,
                SqlDialect::Mysql,
                SqlDialect::Mysql,
                SqlDialect::Sqlite
            ]
        );
    }

    #[test]
    fn dialect_should_reject_unknown_name() {
        assert_eq!(
            "nosql".parse::<SqlDialect>(),
            Err(UnknownDialect("nosql".to_owned()))
        );
    }

    #[test]
    fn dialect_display_should_round_trip_through_from_str() {
        for d in [
            SqlDialect::Postgres,
            SqlDialect::Mysql,
            SqlDialect::Sqlite,
            SqlDialect::SqlServer,
            SqlDialect::Oracle,
        ] {
            assert_eq!(d.to_string().parse::<SqlDialect>(), Ok(d));
        }
    }
}
