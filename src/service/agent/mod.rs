use crate::api::config_service_server::ConfigServiceServer;
use crate::notary;
use crate::store;
use tonic::transport::Server;
use tracing::info;

mod package;
mod service;

pub async fn start(port: u16) -> Result<(), anyhow::Error> {
    store::check_dirs().await?;

    notary::check_keys()?;

    let addr = format!("[::1]:{}", port).parse()?;

    info!("service listening on: {}", addr);

    Server::builder()
        .add_service(ConfigServiceServer::new(service::Agent::default()))
        .serve(addr)
        .await?;

    Ok(())
}
