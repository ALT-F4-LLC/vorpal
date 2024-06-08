use anyhow::Result;
use std::env;
use vorpal::api::build_service_client::BuildServiceClient;
use vorpal::api::{PackageRequest, PackageSource, PackageSourceKind};

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
    let mut client = BuildServiceClient::connect("http://[::1]:23151").await?;

    let example_rust = client
        .package(PackageRequest {
            build_deps: Vec::new(),
            build_phase: r#"
                cat vorpal.rs
            "#
            .to_string(),
            install_phase: r#"
                mkdir -p $OUTPUT
                cp vorpal.rs $OUTPUT/build.rs
            "#
            .to_string(),
            install_deps: Vec::new(),
            name: "example-rust".to_string(),
            source: Some(PackageSource {
                hash: None,
                ignore_paths: vec![".git".to_string(), "target".to_string()],
                kind: PackageSourceKind::Local.into(),
                uri: env::current_dir()?.to_string_lossy().to_string(),
            }),
        })
        .await?
        .into_inner();

    println!("{:?}", example_rust);

    Ok(())
}
