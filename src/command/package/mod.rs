use anyhow::Result;

use crate::api::Package;
use crate::package;

pub async fn run() -> Result<(), anyhow::Error> {
    let example = Package {
        build_phase: "echo \"hello, world!\" >> example.txt".to_string(),
        ignore_paths: vec![
            ".direnv".to_string(),
            ".git".to_string(),
            "target".to_string(),
            "vorpal-build".to_string(),
            "vorpal-cli".to_string(),
        ],
        install_phase: "cp example.txt $package".to_string(),
        name: "example".to_string(),
        source: ".".to_string(),
    };

    package::run(example).await
}
