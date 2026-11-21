use anyhow::Context as _;
use clap::Parser;
use colored::Colorize;
use dbml_cli::args::{Cli, Commands, ExportFormat, ImportDialect};
use dbml_cli::server;
use dbml_cli::state::AppState;
use dbml_cli::watcher;
use std::net::SocketAddr;
use std::path::Path;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Some(Commands::Export {
        file,
        format,
        dialect,
        output,
        force,
    }) = cli.command
    {
        let src = std::fs::read_to_string(&file)?;
        let out = match format {
            ExportFormat::Sql => dbml_analysis::try_export_sql_ddl(&src, dialect.to_sql_dialect())?,
        };
        return write_output(&out, output.as_deref(), force);
    }
    if let Some(Commands::Import {
        file,
        dialect,
        url,
        schemas,
        password_stdin,
        test,
        output,
        force,
    }) = cli.command
    {
        if let Some(url) = url {
            let password = if password_stdin {
                Some(read_password_stdin()?)
            } else {
                std::env::var("DBML_PASSWORD")
                    .ok()
                    .filter(|p| !p.is_empty())
            };
            let url = password.map_or(url.clone(), |p| db_introspect::with_password(&url, &p));
            if test {
                return run_test_url(&url).await;
            }
            return run_import_url(&url, &schemas, output.as_deref(), force).await;
        }
        let (Some(file), Some(dialect)) = (file, dialect) else {
            anyhow::bail!("`dbml import` needs FILE and --dialect, or --url");
        };
        return run_import(&file, dialect, output.as_deref(), force);
    }
    let config = cli.resolve_config()?;

    let state = AppState::new(config.file.clone()).await?;

    // Inicia o observador de arquivos no disco se não desativado
    if !config.no_watch {
        watcher::start_file_watcher(state.clone())?;
    }

    // Tenta alocar a porta desejada ou incrementa se ocupada
    let mut port = config.port;
    let (listener, bound_ip) = loop {
        let addr: SocketAddr = format!("{}:{}", config.host, port).parse()?;
        match TcpListener::bind(addr).await {
            Ok(l) => break (l, addr.ip()),
            Err(_) if port < config.port + 50 => {
                port += 1;
            }
            Err(e) => return Err(e.into()),
        }
    };

    let url = format!("http://{}:{}/?token={}", config.host, port, state.token());

    println!(
        "\n{}",
        "=========================================================="
            .blue()
            .bold()
    );
    println!(
        "{}",
        "         DB Modeling - Local Web Server & Canvas          "
            .blue()
            .bold()
    );
    println!(
        "{}\n",
        "=========================================================="
            .blue()
            .bold()
    );
    let (fp_display, lp_display) = {
        let doc = state.doc.read().await;
        (
            doc.file_path.display().to_string(),
            doc.layout_path.display().to_string(),
        )
    };
    println!("  {} {}", "File:    ".bold(), fp_display.cyan());
    println!("  {} {}", "Sidecar: ".bold(), lp_display.cyan());
    println!(
        "  {} {}",
        "Server:  ".bold(),
        url.green().bold().underline()
    );
    if let Some(warning) = server::non_loopback_warning(bound_ip) {
        println!("\n  {} {}", "AVISO:".red().bold(), warning.red());
    }
    println!(
        "\n  {} Press {} to stop the server.\n",
        "Tip:".yellow(),
        "Ctrl+C".bold()
    );

    // Abre o navegador padrão automaticamente
    if !config.no_open {
        let open_url = url.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            let _ = opener::open(&open_url);
        });
    }

    let app = server::create_router(state);

    // Encerramento gracioso com Ctrl+C
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            println!(
                "\n{}",
                "Shutting down DB Modeling server gracefully...".yellow()
            );
        })
        .await?;

    Ok(())
}

/// Connects, runs a trivial query and prints the server version and schemas as JSON.
async fn run_test_url(url: &str) -> anyhow::Result<()> {
    let out = db_introspect::test_connection(url, &db_introspect::Options::default())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!(
        "{}",
        serde_json::json!({ "server": out.server, "schemas": out.schemas })
    );
    Ok(())
}

/// Reads the catalog of a live database (read-only) and writes it as DBML.
async fn run_import_url(
    url: &str,
    schemas: &[String],
    output: Option<&Path>,
    force: bool,
) -> anyhow::Result<()> {
    let options = db_introspect::Options {
        schemas: schemas.to_vec(),
        ..db_introspect::Options::default()
    };
    let outcome = db_introspect::introspect_with(url, &options)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    for w in &outcome.warnings {
        eprintln!("{}: warning: {w}", db_introspect::mask_url(url));
    }
    write_output(&outcome.dbml, output, force)
}

fn read_password_stdin() -> anyhow::Result<String> {
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("failed to read the password from stdin")?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

fn write_output(content: &str, output: Option<&Path>, force: bool) -> anyhow::Result<()> {
    if let Some(out_path) = output {
        if let Err(err) = write_atomic(out_path, content.as_bytes(), force) {
            if err.kind() == std::io::ErrorKind::AlreadyExists {
                anyhow::bail!(
                    "output file `{}` already exists (pass --force to overwrite)",
                    out_path.display()
                );
            }
            return Err(err)
                .with_context(|| format!("failed to write output file `{}`", out_path.display()));
        }
    } else {
        print!("{content}");
    }
    Ok(())
}

fn run_import(
    file: &Path,
    dialect: ImportDialect,
    output: Option<&Path>,
    force: bool,
) -> anyhow::Result<()> {
    let src = std::fs::read_to_string(file)
        .with_context(|| format!("failed to read SQL file `{}`", file.display()))?;
    let outcome = sql_import::import_sql_ddl(&src, dialect.to_sql_dialect());
    for w in &outcome.warnings {
        eprintln!(
            "{}:{}:{}: warning: {}",
            file.display(),
            w.line,
            w.column,
            w.message
        );
    }
    write_output(&outcome.dbml, output, force)
}

fn write_atomic(path: &Path, bytes: &[u8], force: bool) -> std::io::Result<()> {
    use std::io::Write as _;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output.dbml");
    let mut n = 0;
    let (tmp, mut file) = loop {
        let tmp = parent.join(format!(".{file_name}.{}.{n}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(file) => break (tmp, file),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(err) => return Err(err),
        }
    };
    let written = file.write_all(bytes);
    drop(file);
    let result = written.and_then(|()| {
        if force {
            std::fs::rename(&tmp, path)
        } else {
            std::fs::hard_link(&tmp, path)
        }
    });
    let _ = std::fs::remove_file(&tmp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_import_url_reads_sqlite_file_read_only() {
        let dir = std::env::temp_dir();
        let db = dir.join(format!("dbml_cli_url_{}.sqlite", std::process::id()));
        let out = dir.join(format!("dbml_cli_url_{}.dbml", std::process::id()));
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(&out);
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
            .unwrap();
        drop(conn);
        run_import_url(db.to_str().unwrap(), &[], Some(&out), false)
            .await
            .unwrap();
        let dbml = std::fs::read_to_string(&out).unwrap();
        assert!(
            dbml.contains("Table t {") && dbml.contains("name text [not null]"),
            "{dbml}"
        );
        assert!(
            run_import_url(db.to_str().unwrap(), &[], Some(&out), false)
                .await
                .is_err()
        );
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn run_import_writes_output_and_refuses_overwrite_without_force() {
        let dir = std::env::temp_dir().join(format!("dbml_cli_import_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sql_path = dir.join("schema.sql");
        let out_path = dir.join("schema.dbml");
        std::fs::write(&sql_path, "CREATE TABLE users (id serial PRIMARY KEY);").unwrap();

        run_import(&sql_path, ImportDialect::Postgres, Some(&out_path), false).unwrap();
        let dbml = std::fs::read_to_string(&out_path).unwrap();
        assert!(dbml.contains("Table users {"));

        let err =
            run_import(&sql_path, ImportDialect::Postgres, Some(&out_path), false).unwrap_err();
        assert!(err.to_string().contains("--force"));

        run_import(&sql_path, ImportDialect::Postgres, Some(&out_path), true).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_output_preserves_existing_file_and_writes_new_file() {
        let dir = std::env::temp_dir().join(format!("dbml_cli_export_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("schema.sql");
        let _ = std::fs::remove_file(&out);
        write_output("first", Some(&out), false).unwrap();
        let err = write_output("second", Some(&out), false).unwrap_err();
        assert!(err.to_string().contains("pass --force to overwrite"));
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "first");
        write_output("second", Some(&out), true).unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "second");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
