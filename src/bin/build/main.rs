use anyhow::Result;
use tonic::transport::Server;
use vorpal::api::package_service_server::PackageServiceServer;
use vorpal::service::Packager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:15323".parse()?;
    let packager = Packager::default();

    println!("Server listening on: {}", addr);

    Server::builder()
        .add_service(PackageServiceServer::new(packager))
        .serve(addr)
        .await?;

    Ok(())
}
