use anyhow::Result;
use std::env;
use vorpal::api::build_service_client::BuildServiceClient;
use vorpal::api::{PackageRequest, PackageSource, PackageSourceKind};

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
    let mut client = BuildServiceClient::connect("http://[::1]:23151").await?;

    // Example: local source directory
    client
        .package(PackageRequest {
            build_deps: Vec::new(),
            build_phase: "echo \"example\" >> example.txt && cat example.txt".to_string(),
            install_phase: "cp example.txt $OUTPUT".to_string(),
            install_deps: Vec::new(),
            name: "example-rust".to_string(),
            source: Some(PackageSource {
                hash: None,
                ignore_paths: vec![
                    ".direnv".to_string(),
                    ".git".to_string(),
                    "target".to_string(),
                ],
                kind: PackageSourceKind::Local.into(),
                uri: env::current_dir()?.to_string_lossy().to_string(),
            }),
        })
        .await?;

    Ok(())
}
