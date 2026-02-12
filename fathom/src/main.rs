use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "fathom")]
#[command(about = "Fathom control plane and TUI client")]
struct Cli {
    #[arg(long, global = true, default_value = "127.0.0.1:50051")]
    addr: SocketAddr,

    #[arg(long, global = true, default_value = "http://127.0.0.1:50051")]
    server: String,

    #[arg(long, global = true, default_value_t = 300)]
    startup_delay_ms: u64,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Server,
    Client,
    Both,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Command::Server) => fathom_server::serve(cli.addr).await,
        Some(Command::Client) => fathom_client::run_tui(&cli.server).await,
        Some(Command::Both) | None => {
            run_server_and_client(cli.addr, &cli.server, cli.startup_delay_ms).await
        }
    }
}

async fn run_server_and_client(
    addr: SocketAddr,
    server: &str,
    startup_delay_ms: u64,
) -> Result<()> {
    let server_task = tokio::spawn(async move { fathom_server::serve(addr).await });
    tokio::pin!(server_task);

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(startup_delay_ms)) => {}
        server_result = &mut server_task => {
            return match server_result {
                Ok(result) => result,
                Err(join_error) => Err(anyhow::anyhow!("server task failed: {join_error}")),
            };
        }
    }

    let readiness = tokio::select! {
        result = fathom_client::wait_for_server(server, Duration::from_secs(15)) => result,
        server_result = &mut server_task => {
            return match server_result {
                Ok(result) => result,
                Err(join_error) => Err(anyhow::anyhow!("server task failed: {join_error}")),
            };
        }
    };

    if let Err(error) = readiness {
        server_task.as_mut().abort();
        let _ = server_task.await;
        return Err(error);
    }

    let client_result = fathom_client::run_tui(server).await;
    server_task.as_mut().abort();
    let _ = server_task.await;
    client_result
}
