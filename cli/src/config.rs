use anyhow::Result;
use std::collections::{HashMap, HashSet};
use vorpal_schema::{Config, Package};

pub struct PackageStructures {
    pub graph: HashMap<String, HashSet<String>>,
    pub map: HashMap<String, Package>,
}

pub fn build_structures(config: &Config) -> PackageStructures {
    let mut package_graph: HashMap<String, HashSet<String>> = HashMap::new();
    let mut package_map = HashMap::new();

    for package_name in config.packages.keys() {
        add_to_graph(&mut package_graph, &config.packages[package_name]);
        add_to_map(&mut package_map, &config.packages[package_name]);
    }

    PackageStructures {
        graph: package_graph,
        map: package_map,
    }
}

pub fn topological_sort(
    package_graph: &HashMap<String, HashSet<String>>,
    build_order: &mut Vec<String>,
    visited: &mut HashSet<String>,
    stack: &mut HashSet<String>,
    package: &str,
) -> Result<()> {
    if stack.contains(package) {
        anyhow::bail!(format!("Circular dependency detected: {}", package));
    }

    if visited.contains(package) {
        return Ok(());
    }

    stack.insert(package.to_string());

    if let Some(deps) = package_graph.get(package) {
        for dep in deps {
            topological_sort(package_graph, build_order, visited, stack, dep)?;
        }
    }

    stack.remove(package);

    visited.insert(package.to_string());

    build_order.push(package.to_string());

    Ok(())
}

pub fn add_to_map(package_map: &mut HashMap<String, Package>, package: &Package) {
    package_map.insert(package.name.clone(), package.clone());

    for dep in &package.packages {
        add_to_map(package_map, dep);
    }
}

pub fn add_to_graph(package_graph: &mut HashMap<String, HashSet<String>>, package: &Package) {
    let dependencies: HashSet<String> = package.packages.iter().map(|p| p.name.clone()).collect();

    package_graph.insert(package.name.clone(), dependencies);

    for dep in &package.packages {
        add_to_graph(package_graph, dep);
    }
}

pub fn get_build_order(graph: &HashMap<String, HashSet<String>>) -> Result<Vec<String>> {
    let mut build_order = Vec::new();
    let mut stack = HashSet::new();
    let mut visited = HashSet::new();

    for package in graph.keys() {
        if !visited.contains(package) {
            topological_sort(graph, &mut build_order, &mut visited, &mut stack, package)?;
        }
    }

    Ok(build_order)
}
