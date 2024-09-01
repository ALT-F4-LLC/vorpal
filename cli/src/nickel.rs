use anyhow::Result;
use sha256::digest;
use std::path::Path;
use tokio::process::Command;
use vorpal_schema::{api::package::PackageSystem, Config};

pub async fn load_config(
    config: &String,
    system: PackageSystem,
) -> Result<(Config, String), anyhow::Error> {
    let nickel_version = Command::new("nickel").arg("--version").output().await;

    match nickel_version {
        Ok(output) if output.status.success() => {
            println!(
                "=> Nickel: {}",
                String::from_utf8_lossy(&output.stdout).trim()
            );
        }
        _ => {
            anyhow::bail!("Nickel command not found or not working");
        }
    }

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

    let current_path = std::env::current_dir()?;

    let packages_path = current_path.join(".vorpal/packages");

    let config_str = format!(
        "let config = import \"{}\" in config \"{}\"",
        config_file_path.display(),
        config_system,
    );

    let command_str = format!(
        "echo '{}' | nickel export --import-path {} --import-path {}",
        config_str,
        packages_path.display(),
        current_path.display(),
    );

    let mut command = Command::new("sh");

    let command = command.arg("-c").arg(command_str);

    println!("=> {:?}", command);

    let data = command.output().await?.stdout;

    let data = String::from_utf8(data)?;

    Ok((serde_json::from_str(&data)?, digest(data)))
}
