use std::net::SocketAddr;

use anyhow::Result;
use tonic::transport::Server;
use tracing::info;

pub mod pb {
    tonic::include_proto!("fathom.v1");
}

mod runtime;
mod service;
mod session;
mod util;

use pb::runtime_service_server::RuntimeServiceServer;
pub use service::FathomRuntimeService;

pub async fn serve(addr: SocketAddr) -> Result<()> {
    info!(%addr, "starting grpc server");

    Server::builder()
        .add_service(RuntimeServiceServer::new(FathomRuntimeService::default()))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests;
