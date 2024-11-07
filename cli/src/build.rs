use crate::log::{print_build_order, print_packages};
use anyhow::{bail, Result};
use petgraph::algo::toposort;
use petgraph::graphmap::DiGraphMap;
use std::collections::HashMap;
use tonic::transport::Channel;
use vorpal_schema::vorpal::{
    config::v0::{config_service_client::ConfigServiceClient, ConfigRequest},
    package::v0::{Package, PackageOutput},
};

pub async fn load_packages(
    map: &mut HashMap<PackageOutput, Package>,
    packages: Vec<PackageOutput>,
    service: &mut ConfigServiceClient<Channel>,
) -> Result<()> {
    for package_output in packages.iter() {
        if map.contains_key(package_output) {
            continue;
        }

        let package_request = tonic::Request::new(package_output.clone());

        let package_response = match service.get_package(package_request).await {
            Ok(res) => res,
            Err(error) => {
                bail!("failed to evaluate config: {}", error);
            }
        };

        let package = package_response.into_inner();

        map.insert(package_output.clone(), package.clone());

        if package.packages.is_empty() {
            continue;
        }

        Box::pin(load_packages(map, package.packages, service)).await?
    }

    Ok(())
}

pub async fn load_config<'a>(
    package: &String,
    service: &mut ConfigServiceClient<Channel>,
) -> Result<(HashMap<PackageOutput, Package>, Vec<PackageOutput>)> {
    let response = match service.get_config(ConfigRequest {}).await {
        Ok(res) => res,
        Err(error) => {
            bail!("failed to evaluate config: {}", error);
        }
    };

    let config = response.into_inner();

    if !config.packages.iter().any(|p| p.name == package.as_str()) {
        bail!("Package not found: {}", package);
    }

    let mut packages_map = HashMap::<PackageOutput, Package>::new();

    load_packages(&mut packages_map, config.packages.clone(), service).await?;

    let mut packages_graph = DiGraphMap::<&PackageOutput, Package>::new();

    for (package_output, package) in packages_map.iter() {
        packages_graph.add_node(package_output);

        for output in package.packages.iter() {
            packages_graph.add_edge(package_output, output, package.clone());

            add_edges(&mut packages_graph, &packages_map, package, output, service).await?;
        }
    }

    let packages_order = match toposort(&packages_graph, None) {
        Err(err) => bail!("{:?}", err),
        Ok(order) => order,
    };

    let mut packages_order: Vec<PackageOutput> = packages_order.into_iter().cloned().collect();

    packages_order.reverse();

    print_packages(&packages_order);

    print_build_order(&packages_order);

    Ok((packages_map, packages_order))
}

async fn add_edges<'a>(
    graph: &mut DiGraphMap<&'a PackageOutput, Package>,
    map: &HashMap<PackageOutput, Package>,
    package: &'a Package,
    package_output: &'a PackageOutput,
    service: &mut ConfigServiceClient<Channel>,
) -> Result<()> {
    if map.contains_key(package_output) {
        return Ok(());
    }

    graph.add_node(package_output);

    for output in package.packages.iter() {
        graph.add_edge(package_output, output, package.clone());

        Box::pin(add_edges(graph, map, package, output, service)).await?;
    }

    Ok(())
}
