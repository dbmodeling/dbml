use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
