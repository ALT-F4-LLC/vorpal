use anyhow::Result;
use std::env;
use vorpal::api::build_service_client::BuildServiceClient;
use vorpal::api::PackageRequest;

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
    let mut client = BuildServiceClient::connect("http://[::1]:23151").await?;

    let foo = client
        .package(PackageRequest {
            build_deps: Vec::new(),
            build_phase: "echo \"foo\" >> foo.txt && cat foo.txt".to_string(),
            ignore_paths: vec![
                ".direnv".to_string(),
                ".git".to_string(),
                "target".to_string(),
            ],
            install_deps: Vec::new(),
            install_phase: "cp foo.txt $OUTPUT".to_string(),
            name: "foo".to_string(),
            source: env::current_dir()?.to_string_lossy().to_string(),
        })
        .await?
        .into_inner();

    client
        .package(PackageRequest {
            build_deps: vec![foo],
            build_phase: "echo \"bar\" >> bar.txt && cat bar.txt".to_string(),
            ignore_paths: vec![
                ".direnv".to_string(),
                ".git".to_string(),
                "target".to_string(),
            ],
            install_deps: Vec::new(),
            install_phase: "mkdir -p $OUTPUT && cp bar.txt $OUTPUT/bar.txt".to_string(),
            name: "bar".to_string(),
            source: env::current_dir()?.to_string_lossy().to_string(),
        })
        .await?;

    Ok(())
}
