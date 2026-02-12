use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use tonic::transport::Server;
use tracing::info;

pub mod pb {
    tonic::include_proto!("fathom.v1");
}

mod agent;
mod fs;
mod runtime;
mod service;
mod session;
mod util;

use pb::runtime_service_server::RuntimeServiceServer;
pub use service::FathomRuntimeService;

pub async fn serve(addr: SocketAddr) -> Result<()> {
    serve_with_workspace_root(addr, None).await
}

pub async fn serve_with_workspace_root(
    addr: SocketAddr,
    workspace_root: Option<PathBuf>,
) -> Result<()> {
    info!(%addr, "starting grpc server");
    let service = match workspace_root {
        Some(workspace_root) => FathomRuntimeService::with_workspace_root(workspace_root)?,
        None => FathomRuntimeService::default(),
    };

    Server::builder()
        .add_service(RuntimeServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
