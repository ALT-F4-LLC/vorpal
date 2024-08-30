use anyhow::Result;
use sha256::digest;
use tokio::fs::write;
use tokio::process::Command;
use vorpal_schema::{api::package::PackageSystem, Config};
use vorpal_store::temps::create_temp_file;

pub async fn load_config(
    config: &String,
    system: PackageSystem,
) -> Result<(Config, String), anyhow::Error> {
    let config_system = match system {
        PackageSystem::Aarch64Linux => "aarch64-linux",
        PackageSystem::Aarch64Macos => "aarch64-macos",
        PackageSystem::X8664Linux => "x86_64-linux",
        PackageSystem::X8664Macos => "x86_64-macos",
        PackageSystem::Unknown => anyhow::bail!("unknown target"),
    };

    let config_str = format!(
        "let config = import \"{}\" in config \"{}\"",
        config, config_system,
    );

    println!("config_str: {}", config_str);

    let temp_file = create_temp_file("ncl").await?;

    write(&temp_file, config_str).await?;

    println!("temp_file: {}", temp_file.display());

    let data = Command::new("nickel")
        .arg("export")
        .arg("--import-path")
        .arg(".")
        .arg("--import-path")
        .arg(".vorpal/packages")
        .arg(temp_file.display().to_string())
        .output()
        .await?
        .stdout;

    let data = String::from_utf8(data)?;

    println!("data: {}", data);

    Ok((serde_json::from_str(&data)?, digest(data)))
}
