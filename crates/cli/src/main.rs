use anyhow::Context as _;
use clap::Parser;
use colored::Colorize;
use dbml_cli::args::{Cli, Commands, ExportFormat, ImportDialect};
use dbml_cli::net;
use dbml_cli::server;
use dbml_cli::state::AppState;
use dbml_cli::watcher;
use std::path::Path;

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
                Some(("--password-stdin", read_password_stdin()?))
            } else {
                std::env::var("DBML_PASSWORD")
                    .ok()
                    .filter(|p| !p.is_empty())
                    .map(|p| ("DBML_PASSWORD", p))
            };
            let (url, warning) = apply_password(&url, password);
            if let Some(warning) = warning {
                eprintln!("warning: {warning}");
            }
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

    // Resolve o host (nome ou IP) e tenta a porta pedida, subindo se estiver ocupada
    let listener = net::bind_with_retry(&config.host, config.port).await?;
    let local_addr = listener.local_addr()?;
    let url = net::browser_url(local_addr, state.token());

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
    if let Some(warning) = server::non_loopback_warning(local_addr.ip()) {
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

    let app = server::create_router(state.clone());
    let shutdown_state = state.clone();

    // Encerramento gracioso com Ctrl+C
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            shutdown_state.lifecycle.begin_shutdown();
            println!(
                "\n{}",
                "Shutting down DB Modeling server gracefully...".yellow()
            );
        })
        .await?;

    // Espera sockets e watcher e as gravações pendentes antes de sair.
    state.shutdown(std::time::Duration::from_secs(5)).await;

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

/// Whether the URL already carries `user:password@`.
fn url_has_password(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    let authority = rest.split_once('/').map_or(rest, |(a, _)| a);
    authority
        .rsplit_once('@')
        .is_some_and(|(userinfo, _)| userinfo.contains(':'))
}

/// Injects the password given via `--password-stdin`/`DBML_PASSWORD` into `url`. When the URL
/// already has one, the URL wins (as before) and a warning says so, so the ignored value is not a
/// silent surprise.
fn apply_password(url: &str, password: Option<(&str, String)>) -> (String, Option<String>) {
    let Some((source, password)) = password else {
        return (url.to_owned(), None);
    };
    if url_has_password(url) {
        let warning = format!(
            "the URL already contains a password; the one from {source} is ignored and the URL's password is used"
        );
        return (url.to_owned(), Some(warning));
    }
    (db_introspect::with_password(url, &password), None)
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
    fn warns_when_the_url_already_has_a_password() {
        assert!(url_has_password("postgres://u:p@h/db"));
        assert!(!url_has_password("postgres://u@h/db"));
        assert!(!url_has_password("postgres://h:5432/db"));
        assert!(!url_has_password("app.sqlite"));

        let (url, warning) =
            apply_password("postgres://u:p@h/db", Some(("DBML_PASSWORD", "x".into())));
        assert_eq!(url, "postgres://u:p@h/db");
        let warning = warning.unwrap();
        assert!(warning.contains("DBML_PASSWORD") && warning.contains("URL's password is used"));
        assert!(!warning.contains(":p@"), "o aviso não pode ecoar a senha");

        let (url, warning) =
            apply_password("postgres://u@h/db", Some(("--password-stdin", "s3".into())));
        assert_eq!(url, "postgres://u:s3@h/db");
        assert!(warning.is_none());
        let (url, warning) = apply_password("postgres://u:p@h/db", None);
        assert_eq!((url.as_str(), warning), ("postgres://u:p@h/db", None));
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
