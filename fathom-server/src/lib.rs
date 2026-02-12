use std::net::SocketAddr;

use anyhow::Result;
use tonic::{Request, Response, Status, transport::Server};
use tracing::info;

pub mod pb {
    tonic::include_proto!("fathom.v1");
}

use pb::agent_service_server::{AgentService, AgentServiceServer};
use pb::{PingRequest, PingResponse};

#[derive(Default)]
pub struct FathomAgentService;

#[tonic::async_trait]
impl AgentService for FathomAgentService {
    async fn ping(&self, request: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        let message = request.into_inner().message;
        info!(%message, "received ping");

        Ok(Response::new(PingResponse {
            message: format!("pong: {message}"),
        }))
    }
}

pub async fn serve(addr: SocketAddr) -> Result<()> {
    info!(%addr, "starting grpc server");

    Server::builder()
        .add_service(AgentServiceServer::new(FathomAgentService))
        .serve(addr)
        .await?;

    Ok(())
}
