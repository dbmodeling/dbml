use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Output format for `dbml export`.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// SQL DDL (`CREATE TABLE`, `FOREIGN KEY`, `CREATE INDEX`).
    Sql,
}

/// Target SQL dialect for `dbml export --format sql`.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliDialect {
    /// PostgreSQL
    #[value(alias = "postgresql", alias = "pg")]
    Postgres,
    /// MySQL
    Mysql,
    /// SQLite
    #[value(alias = "sqlite3")]
    Sqlite,
    /// Microsoft SQL Server / T-SQL
    #[value(alias = "mssql", alias = "tsql")]
    Sqlserver,
    /// Oracle Database
    #[value(alias = "ora")]
    Oracle,
}

impl CliDialect {
    /// Converts this CLI dialect flag into [`dbml_analysis::SqlDialect`].
    #[must_use]
    pub const fn to_sql_dialect(self) -> dbml_analysis::SqlDialect {
        match self {
            Self::Postgres => dbml_analysis::SqlDialect::Postgres,
            Self::Mysql => dbml_analysis::SqlDialect::Mysql,
            Self::Sqlite => dbml_analysis::SqlDialect::Sqlite,
            Self::Sqlserver => dbml_analysis::SqlDialect::SqlServer,
            Self::Oracle => dbml_analysis::SqlDialect::Oracle,
        }
    }
}

/// Source SQL dialect for `dbml import`.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportDialect {
    /// PostgreSQL
    #[value(alias = "postgresql", alias = "pg")]
    Postgres,
    /// MySQL / MariaDB
    #[value(alias = "mariadb")]
    Mysql,
    /// SQLite
    #[value(alias = "sqlite3")]
    Sqlite,
}

impl ImportDialect {
    /// Converts this CLI dialect flag into [`sql_import::SqlDialect`].
    #[must_use]
    pub const fn to_sql_dialect(self) -> sql_import::SqlDialect {
        match self {
            Self::Postgres => sql_import::SqlDialect::Postgres,
            Self::Mysql => sql_import::SqlDialect::Mysql,
            Self::Sqlite => sql_import::SqlDialect::Sqlite,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "dbml")]
#[command(about = "DB Modeling CLI - Interactive database schema viewer and editor")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to the .dbml file (when executed directly as `dbml schema.dbml`)
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Local web server port
    #[arg(short, long, default_value_t = 3000)]
    pub port: u16,

    /// Bind address
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Do not open browser automatically on startup
    #[arg(long)]
    pub no_open: bool,

    /// Disable watching for external file changes
    #[arg(long)]
    pub no_watch: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the local server and open the visual canvas in the browser
    Run {
        /// Path to the .dbml file or project folder
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,

        /// Local web server port
        #[arg(short, long, default_value_t = 3000)]
        port: u16,

        /// Bind address
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Do not open browser automatically
        #[arg(long)]
        no_open: bool,

        /// Disable watching for external file changes
        #[arg(long)]
        no_watch: bool,
    },
    /// Alias to start the web server
    Serve {
        /// Path to the .dbml file or project folder
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,

        /// Local web server port
        #[arg(short, long, default_value_t = 3000)]
        port: u16,

        /// Bind address
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Do not open browser automatically
        #[arg(long)]
        no_open: bool,

        /// Disable watching for external file changes
        #[arg(long)]
        no_watch: bool,
    },
    /// Export a .dbml file to SQL DDL
    Export {
        /// Path to the input .dbml file
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Export format
        #[arg(long, value_enum, default_value_t = ExportFormat::Sql)]
        format: ExportFormat,

        /// Target SQL dialect
        #[arg(long, value_enum, default_value_t = CliDialect::Postgres)]
        dialect: CliDialect,

        /// Optional output file path (writes to stdout if omitted)
        #[arg(short, long, value_name = "OUTPUT")]
        output: Option<PathBuf>,
    },
    /// Import a SQL DDL file into DBML
    Import {
        /// Path to the input .sql file
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Source SQL dialect
        #[arg(long, value_enum)]
        dialect: ImportDialect,

        /// Optional output .dbml file path (writes to stdout if omitted)
        #[arg(short, long, value_name = "OUTPUT")]
        output: Option<PathBuf>,

        /// Overwrite the output file if it already exists
        #[arg(long)]
        force: bool,
    },
}

pub struct RunConfig {
    pub file: PathBuf,
    pub port: u16,
    pub host: String,
    pub no_open: bool,
    pub no_watch: bool,
}

impl Cli {
    /// Resolves CLI flags and fallback session target into a [`RunConfig`].
    ///
    /// # Errors
    ///
    /// Returns an error if no target file or folder was provided and no previous session exists.
    pub fn resolve_config(self) -> anyhow::Result<RunConfig> {
        let (file, port, host, no_open, no_watch) = match self.command {
            Some(
                Commands::Run {
                    file,
                    port,
                    host,
                    no_open,
                    no_watch,
                }
                | Commands::Serve {
                    file,
                    port,
                    host,
                    no_open,
                    no_watch,
                },
            ) => (file, port, host, no_open, no_watch),
            Some(Commands::Export { .. }) => {
                anyhow::bail!("export command should be handled before starting the web server")
            }
            Some(Commands::Import { .. }) => {
                anyhow::bail!("import command should be handled before starting the web server")
            }
            None => (self.file, self.port, self.host, self.no_open, self.no_watch),
        };

        let resolved_file = if let Some(f) = file {
            f
        } else {
            let session = crate::session::load();
            match session.last_target {
                Some(prev) if prev.exists() => prev,
                _ => anyhow::bail!(
                    "No .dbml file or folder specified and no previous session found. Usage: dbml <file.dbml | folder>"
                ),
            }
        };

        Ok(RunConfig {
            file: resolved_file,
            port,
            host,
            no_open,
            no_watch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_export_subcommand_with_all_dialects() {
        for (flag, expected) in [
            ("postgres", dbml_analysis::SqlDialect::Postgres),
            ("mysql", dbml_analysis::SqlDialect::Mysql),
            ("sqlite", dbml_analysis::SqlDialect::Sqlite),
            ("sqlserver", dbml_analysis::SqlDialect::SqlServer),
            ("mssql", dbml_analysis::SqlDialect::SqlServer),
            ("oracle", dbml_analysis::SqlDialect::Oracle),
        ] {
            let cli = Cli::try_parse_from([
                "dbml",
                "export",
                "schema.dbml",
                "--format",
                "sql",
                "--dialect",
                flag,
                "-o",
                "out.sql",
            ])
            .unwrap();
            match cli.command {
                Some(Commands::Export {
                    file,
                    format,
                    dialect,
                    output,
                }) => {
                    assert_eq!(file, PathBuf::from("schema.dbml"));
                    assert_eq!(format, ExportFormat::Sql);
                    assert_eq!(dialect.to_sql_dialect(), expected);
                    assert_eq!(output, Some(PathBuf::from("out.sql")));
                }
                other => panic!("expected Export command, got {other:?}"),
            }
        }
    }

    #[test]
    fn cli_should_parse_import_with_dialect_and_output() {
        for (flag, expected) in [
            ("postgres", sql_import::SqlDialect::Postgres),
            ("pg", sql_import::SqlDialect::Postgres),
            ("postgresql", sql_import::SqlDialect::Postgres),
            ("mysql", sql_import::SqlDialect::Mysql),
            ("mariadb", sql_import::SqlDialect::Mysql),
            ("sqlite", sql_import::SqlDialect::Sqlite),
            ("sqlite3", sql_import::SqlDialect::Sqlite),
        ] {
            let cli = Cli::try_parse_from([
                "dbml",
                "import",
                "schema.sql",
                "--dialect",
                flag,
                "-o",
                "schema.dbml",
                "--force",
            ])
            .unwrap();
            match cli.command {
                Some(Commands::Import {
                    file,
                    dialect,
                    output,
                    force,
                }) => {
                    assert_eq!(file, PathBuf::from("schema.sql"));
                    assert_eq!(dialect.to_sql_dialect(), expected);
                    assert_eq!(output, Some(PathBuf::from("schema.dbml")));
                    assert!(force);
                }
                other => panic!("expected Import command, got {other:?}"),
            }
        }
    }

    #[test]
    fn cli_should_require_dialect_for_import() {
        let err = Cli::try_parse_from(["dbml", "import", "schema.sql"]);
        assert!(err.is_err());
    }
}
