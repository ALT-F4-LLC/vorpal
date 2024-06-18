use anyhow::Result;
// use std::env;
use tokio_stream::StreamExt;
use vorpal::api::config_service_client::ConfigServiceClient;
use vorpal::api::{
    ConfigPackageBuild, ConfigPackageRequest, ConfigPackageSource, ConfigPackageSourceKind,
};

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
    let mut client = ConfigServiceClient::connect("http://[::1]:15323").await?;

    let coreutils = client
        .package(ConfigPackageRequest {
            build: Some(ConfigPackageBuild {
                packages: vec![],
                script: r#"
                    cd coreutils-9.5
                    test -f configure || ./bootstrap
                    ./configure --prefix=$OUTPUT
                    make
                    make install
                "#
                .to_string(),
            }),
            name: "coreutils".to_string(),
            source: Some(ConfigPackageSource {
                hash: Some("af6d643afd6241ec35c7781b7f999b97a66c84bea4710ad2bb15e75a5caf11b4".to_string()),
                ignore_paths: vec![],
                kind: ConfigPackageSourceKind::Http.into(),
                uri: "https://ftp.gnu.org/gnu/coreutils/coreutils-9.5.tar.gz".to_string(),
            }),
        })
        .await?;

    let mut stream = coreutils.into_inner();
    while let Some(response) = stream.next().await {
        let res = response?;
        if !res.log_output.is_empty() {
            println!("{}", res.log_output);
        }
    }

    // let example = client
    //     .package(ConfigPackageRequest {
    //         build: Some(ConfigPackageBuild {
    //             packages: vec![],
    //             script: r#"
    //             mkdir -p $OUTPUT/bin
    //             touch $OUTPUT/bin/example.txt
    //             cat src/main.rs >> $OUTPUT/bin/example.txt
    //             cat $OUTPUT/bin/example.txt
    //         "#
    //             .to_string(),
    //         }),
    //         name: "example".to_string(),
    //         source: Some(ConfigPackageSource {
    //             hash: None,
    //             ignore_paths: vec![
    //                 ".git".to_string(),
    //                 ".gitignore".to_string(),
    //                 "target".to_string(),
    //             ],
    //             kind: ConfigPackageSourceKind::Local.into(),
    //             uri: env::current_dir()?.to_string_lossy().to_string(),
    //         }),
    //     })
    //     .await?;
    //
    // let mut stream = example.into_inner();
    // while let Some(package_response) = stream.next().await {
    //     let response = package_response?;
    //     if !response.log_output.is_empty() {
    //         println!("{}", response.log_output);
    //     }
    // }

    Ok(())
}
