use anyhow::{bail, Result};
use petgraph::algo::toposort;
use petgraph::graphmap::DiGraphMap;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::Package;

pub fn load_config(
    packages: &HashMap<String, Package>,
) -> Result<(HashMap<String, Package>, Vec<String>)> {
    let mut graph = DiGraphMap::<&str, Package>::new();
    let mut map = HashMap::<String, Package>::new();

    for package in packages.values() {
        if package.packages.is_empty() {
            graph.add_node(&package.name);
        }

        add_edges(package, &mut graph, &mut map);
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

fn add_edges<'a>(
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
        add_edges(dependency, graph, map);
    }
}
