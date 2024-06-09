use anyhow::Result;
use std::env;
use vorpal::api::build_service_client::BuildServiceClient;
use vorpal::api::{PackageRequest, PackageSource, PackageSourceKind};

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
    let mut client = BuildServiceClient::connect("http://[::1]:23151").await?;

    let example_busybox = client
        .package(PackageRequest {
            build_deps: Vec::new(),
            build_phase: r#"
                cat "hello, world!" > example.txt
            "#
            .to_string(),
            install_phase: r#"
                mkdir -p $OUTPUT
                cp example.txt $OUTPUT/example.txt
            "#
            .to_string(),
            install_deps: Vec::new(),
            name: "busybox".to_string(),
            source: Some(PackageSource {
                hash: None,
                ignore_paths: vec![],
                kind: PackageSourceKind::Http.into(),
                uri: "https://busybox.net/downloads/busybox-1.36.1.tar.bz2".to_string(),
            }),
        })
        .await?
        .into_inner();

    println!("{:?}", example_busybox);

    Ok(())
}
