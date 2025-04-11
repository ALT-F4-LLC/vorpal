use crate::artifact::WorkerServer;
use anyhow::Result;
use tonic::transport::Server;
use vorpal_schema::worker::v0::worker_service_server::WorkerServiceServer;
use vorpal_sdk::system::get_system_default;
use vorpal_store::paths::get_public_key_path;

pub async fn listen(registry: &str, port: u16) -> Result<()> {
    let public_key_path = get_public_key_path();

    if !public_key_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal keys generate' or copy from agent"
        ));
    }

    let addr = format!("[::]:{}", port)
        .parse()
        .expect("failed to parse address");

    let system = get_system_default()?;

    let service = WorkerServiceServer::new(WorkerServer::new(registry.to_string(), system));

    Server::builder()
        .add_service(service)
        .serve(addr)
        .await
        .expect("failed to start worker server");

    Ok(())
}
