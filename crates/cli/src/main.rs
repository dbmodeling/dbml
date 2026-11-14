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
    }) = cli.command
    {
        let src = std::fs::read_to_string(&file)?;
        let out = match format {
            ExportFormat::Sql => dbml_analysis::export_sql_ddl(&src, dialect.to_sql_dialect()),
        };
        if let Some(out_path) = output {
            std::fs::write(&out_path, out)?;
        } else {
            print!("{out}");
        }
        return Ok(());
    }
    if let Some(Commands::Import {
        file,
        dialect,
        output,
        force,
    }) = cli.command
    {
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
    let listener = loop {
        let addr: SocketAddr = format!("{}:{}", config.host, port).parse()?;
        match TcpListener::bind(addr).await {
            Ok(l) => break l,
            Err(_) if port < config.port + 50 => {
                port += 1;
            }
            Err(e) => return Err(e.into()),
        }
    };

    let url = format!("http://{}:{}/", config.host, port);

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
    let fp_display = state.file_path.read().await.display().to_string();
    let lp_display = state.layout_path.read().await.display().to_string();
    println!("  {} {}", "File:    ".bold(), fp_display.cyan());
    println!("  {} {}", "Sidecar: ".bold(), lp_display.cyan());
    println!(
        "  {} {}",
        "Server:  ".bold(),
        url.green().bold().underline()
    );
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
    if let Some(out_path) = output {
        if out_path.exists() && !force {
            anyhow::bail!(
                "output file `{}` already exists (pass --force to overwrite)",
                out_path.display()
            );
        }
        write_atomic(out_path, outcome.dbml.as_bytes())?;
    } else {
        print!("{}", outcome.dbml);
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output.dbml");
    let tmp = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)
        .with_context(|| format!("failed to write temporary file `{}`", tmp.display()))?;
    if let Err(err) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err)
            .with_context(|| format!("failed to write output file `{}`", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
