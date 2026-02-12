use std::net::SocketAddr;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "fathom")]
#[command(about = "Fathom control plane and TUI client")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Server {
        #[arg(long, default_value = "127.0.0.1:50051")]
        addr: SocketAddr,
    },
    Client {
        #[arg(long, default_value = "http://127.0.0.1:50051")]
        server: String,
    },
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
        Command::Server { addr } => fathom_server::serve(addr).await,
        Command::Client { server } => fathom_client::run_tui(&server).await,
    }
}
