use anyhow::{bail, Result};
use petgraph::algo::toposort;
use petgraph::graphmap::DiGraphMap;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use tokio::process::Command;
use vorpal_schema::{api::package::PackageSystem, Config, Package};

pub async fn load_config(config_path: &Path, system: PackageSystem) -> Result<Config> {
    let nickel_version = Command::new("nickel").arg("--version").output().await;

    if let Ok(output) = nickel_version {
        if output.status.success() {
            let _ = String::from_utf8_lossy(&output.stdout).trim();
        } else {
            bail!("Nickel command not found or not working");
        }
    } else {
        bail!("Nickel command not found or not working");
    }

    let config_system = match system {
        PackageSystem::Aarch64Linux => "aarch64-linux",
        PackageSystem::Aarch64Macos => "aarch64-macos",
        PackageSystem::X8664Linux => "x86_64-linux",
        PackageSystem::X8664Macos => "x86_64-macos",
        PackageSystem::Unknown => bail!("unknown target"),
    };

    let config_path_canoncicalized = config_path
        .canonicalize()
        .expect("failed to get config path");

    let config_root_dir_path = config_path_canoncicalized
        .parent()
        .expect("failed to get config parent path");

    let config_str = format!(
        "let config = import \"{}\" in config \"{}\"",
        config_path
            .canonicalize()
            .expect("failed to get config path")
            .display(),
        config_system,
    );

    let mut command_str = format!(
        "echo '{}' | nickel export --import-path {}",
        config_str,
        config_root_dir_path
            .canonicalize()
            .expect("failed to canonicalize")
            .display(),
    );

    let packages_path = config_root_dir_path.join(".vorpal/packages");

    if packages_path.exists() {
        command_str = format!(
            "{} --import-path {}",
            command_str,
            packages_path
                .canonicalize()
                .expect("failed to canonicalize")
                .display()
        );
    }

    let mut command = Command::new("sh");

    let command = command.arg("-c").arg(command_str);

    let command_output = match command.output().await {
        Err(err) => bail!("{:?}", err),
        Ok(output) => output,
    };

    if !command_output.status.success() {
        bail!("failed with status: {:?}", command_output.status);
    }

    let data = String::from_utf8(command_output.stdout).expect("failed to convert data to string");

    if data.is_empty() {
        bail!("failed to load config");
    }

    let config: Config = serde_json::from_str(&data).expect("failed to parse json");

    Ok(config)
}

pub fn load_config_build(
    packages: &BTreeMap<String, Package>,
) -> Result<(HashMap<String, Package>, Vec<String>)> {
    let mut graph = DiGraphMap::<&str, Package>::new();
    let mut map = HashMap::<String, Package>::new();

    for package in packages.values() {
        if package.packages.is_empty() {
            graph.add_node(&package.name);
        }

        if let Some(sandbox) = &package.sandbox {
            add_graph_edges(sandbox, &mut graph, &mut map);
        }

        add_graph_edges(package, &mut graph, &mut map);
    }

    let mut order = match toposort(&graph, None) {
        Err(err) => bail!("{:?}", err),
        Ok(order) => order,
    };

    order.reverse();

    let order = order
        .iter()
        .map(|name| name.to_string())
        .collect::<Vec<String>>();

    Ok((map, order))
}

fn add_graph_edges<'a>(
    package: &'a Package,
    graph: &mut DiGraphMap<&'a str, Package>,
    map: &mut HashMap<String, Package>,
) {
    if map.contains_key(package.name.as_str()) {
        return;
    }

    map.insert(package.name.clone(), package.clone());

    for dependency in &package.packages {
        graph.add_edge(&package.name, &dependency.name, dependency.clone());
        add_graph_edges(dependency, graph, map);
    }
}
