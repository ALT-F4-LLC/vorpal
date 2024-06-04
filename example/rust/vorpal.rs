use anyhow::Result;
use std::env;
use vorpal::api::build_service_client::BuildServiceClient;
use vorpal::api::PackageRequest;

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
    let example = PackageRequest {
        build_phase: r#"
            echo "hello, world!" >> example.txt
            cat example.txt
        "#
        .to_string(),
        ignore_paths: vec![
            ".direnv".to_string(),
            ".git".to_string(),
            "target".to_string(),
        ],
        install_phase: r#"
            mkdir -p $OUTPUT
            cp example.txt $OUTPUT/example.txt
        "#
        .to_string(),
        name: "example".to_string(),
        source: env::current_dir()?.to_string_lossy().to_string(),
    };

    let mut client = BuildServiceClient::connect("http://[::1]:23151").await?;
    let response = client.package(example).await?;
    let response_data = response.into_inner();
    println!("{:?}", response_data);

    Ok(())
}
