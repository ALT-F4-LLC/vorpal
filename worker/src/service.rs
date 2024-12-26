use crate::artifact::ArtifactServer;
use anyhow::Result;
use std::env::consts::{ARCH, OS};
use tonic::transport::Server;
use vorpal_schema::{
    get_artifact_system, vorpal::artifact::v0::artifact_service_server::ArtifactServiceServer,
};
use vorpal_store::paths::get_public_key_path;

pub async fn listen(registry: &str, port: u16) -> Result<()> {
    let public_key_path = get_public_key_path();

    if !public_key_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal keys generate' or copy from agent"
        ));
    }

    let system = get_artifact_system(format!("{}-{}", ARCH, OS).as_str());

    let addr = format!("[::]:{}", port)
        .parse()
        .expect("failed to parse address");

    let artifact_service =
        ArtifactServiceServer::new(ArtifactServer::new(registry.to_string(), system));

    Server::builder()
        .add_service(artifact_service)
        .serve(addr)
        .await
        .expect("failed to start worker server");

    Ok(())
}
