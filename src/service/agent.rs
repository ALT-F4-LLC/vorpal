use crate::api::config_service_server::ConfigServiceServer;
use crate::notary;
use crate::service::config::{Config, ConfigWorker};
use crate::service::get_build_system;
use crate::store;
use std::env::consts::{ARCH, OS};
use tonic::transport::Server;
use tracing::{error, info, warn};

pub async fn start(port: &u16, workers: &[String]) -> Result<(), anyhow::Error> {
    store::check().await?;

    notary::check_agent()?;

    let system = get_build_system(format!("{}-{}", ARCH, OS).as_str());

    info!("service system: {:?}", system);

    let workers: Vec<ConfigWorker> = workers
        .iter()
        .map(|worker| {
            let parts: Vec<&str> = worker.split('=').collect();
            ConfigWorker {
                system: get_build_system(parts[0]),
                uri: parts[1].to_string(),
            }
        })
        .collect();

    if workers.is_empty() {
        error!("no workers specified");
        return Ok(());
    }

    if !workers.iter().any(|w| w.system == system) {
        warn!("no workers for the current system");
    }

    info!("service workers: {:?}", workers);

    let address = format!("[::]:{}", port).parse()?;

    info!("service address: {}", address);

    Server::builder()
        .add_service(ConfigServiceServer::new(Config::new(&workers)))
        .serve(address)
        .await?;

    Ok(())
}
