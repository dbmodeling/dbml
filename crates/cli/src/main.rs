use clap::Parser;
use colored::Colorize;
use dbml_cli::args::{Cli, Commands, ExportFormat};
use dbml_cli::server;
use dbml_cli::state::AppState;
use dbml_cli::watcher;
use std::net::SocketAddr;
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
