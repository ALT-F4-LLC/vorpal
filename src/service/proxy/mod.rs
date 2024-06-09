use crate::api::build_service_server::BuildServiceServer;
use crate::notary;
use crate::store;
use tonic::transport::Server;
use tracing::info;

mod package;
mod service;

pub async fn start(port: u16) -> Result<(), anyhow::Error> {
    store::init().await?;
    notary::init()?;

    let addr = format!("[::1]:{}", port).parse()?;
    let proxy = service::Proxy::default();

    info!("service listening on: {}", addr);

    Server::builder()
        .add_service(BuildServiceServer::new(proxy))
        .serve(addr)
        .await?;

    Ok(())
}
