use anyhow::Result;
use sha256::digest;
use std::path::Path;
use tokio::fs::write;
use tokio::process::Command;
use vorpal_schema::{api::package::PackageSystem, Config};
use vorpal_store::temps::create_temp_file;

pub async fn load_config(
    config: &String,
    system: PackageSystem,
) -> Result<(Config, String), anyhow::Error> {
    let config_file_path = Path::new(config);

    if !config_file_path.exists() {
        anyhow::bail!("config not found: {}", config);
    }

    let config_system = match system {
        PackageSystem::Aarch64Linux => "aarch64-linux",
        PackageSystem::Aarch64Macos => "aarch64-macos",
        PackageSystem::X8664Linux => "x86_64-linux",
        PackageSystem::X8664Macos => "x86_64-macos",
        PackageSystem::Unknown => anyhow::bail!("unknown target"),
    };

    let config_str = format!(
        "let config = import \"{}\" in config \"{}\"",
        config_file_path.display(),
        config_system,
    );

    let sandbox_file = create_temp_file("ncl").await?;

    write(&sandbox_file, config_str).await?;

    let current_path = std::env::current_dir()?;

    let packages_path = current_path.join(".vorpal/packages");

    let mut command = Command::new("nickel");

    let command = command
        .arg("export")
        .arg("--import-path")
        .arg(current_path.display().to_string())
        .arg("--import-path")
        .arg(packages_path.display().to_string())
        .arg(sandbox_file.display().to_string());

    println!("=> Running: {:?}", command);

    let data = command.output().await?.stdout;

    let data = String::from_utf8(data)?;

    Ok((serde_json::from_str(&data)?, digest(data)))
}
