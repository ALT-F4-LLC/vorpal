use anyhow::Result;
use petgraph::algo::toposort;
use petgraph::graphmap::DiGraphMap;
use sha256::digest;
use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;
use vorpal_schema::{api::package::PackageSystem, Config, Package};

pub async fn load_config(
    config_path: &Path,
    system: PackageSystem,
) -> Result<(Config, String), anyhow::Error> {
    let nickel_version = Command::new("nickel").arg("--version").output().await;

    if let Ok(output) = nickel_version {
        if output.status.success() {
            let _ = String::from_utf8_lossy(&output.stdout).trim();
        } else {
            anyhow::bail!("Nickel command not found or not working");
        }
    } else {
        anyhow::bail!("Nickel command not found or not working");
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
        config_path.display(),
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

    let data = command.output().await?.stdout;

    let data = String::from_utf8(data)?;

    Ok((serde_json::from_str(&data)?, digest(data)))
}

pub fn load_config_build(
    packages: &HashMap<String, Package>,
) -> Result<(DiGraphMap<&str, Package>, HashMap<&str, Package>, Vec<&str>)> {
    let mut graph = DiGraphMap::<&str, Package>::new();
    let mut map = HashMap::<&str, Package>::new();

    for package in packages.values() {
        add_graph_edges(package, &mut graph, &mut map);
    }

    let mut order = match toposort(&graph, None) {
        Err(err) => anyhow::bail!("{:?}", err),
        Ok(order) => order,
    };

    order.reverse();

    Ok((graph, map, order))
}

fn add_graph_edges<'a>(
    package: &'a Package,
    graph: &mut DiGraphMap<&'a str, Package>,
    map: &mut HashMap<&'a str, Package>,
) {
    if map.contains_key(package.name.as_str()) {
        return;
    }

    map.insert(&package.name, package.clone());

    for dependency in &package.packages {
        graph.add_edge(&package.name, &dependency.name, dependency.clone());
        add_graph_edges(&dependency, graph, map);
    }
}
