use crate::log::{print_artifacts, print_artifacts_total};
use anyhow::{bail, Result};
use petgraph::algo::toposort;
use petgraph::graphmap::DiGraphMap;
use std::collections::HashMap;
use tonic::transport::Channel;
use vorpal_schema::vorpal::{
    artifact::v0::{Artifact, ArtifactId},
    config::v0::{config_service_client::ConfigServiceClient, ConfigRequest},
};

pub async fn load_artifacts(
    map: &mut HashMap<ArtifactId, Artifact>,
    artifacts: Vec<ArtifactId>,
    service: &mut ConfigServiceClient<Channel>,
) -> Result<()> {
    for artifact_id in artifacts.iter() {
        if !map.contains_key(artifact_id) {
            let request = tonic::Request::new(artifact_id.clone());

            let response = match service.get_artifact(request).await {
                Ok(res) => res,
                Err(error) => {
                    bail!("failed to evaluate config: {}", error);
                }
            };

            let artifact = response.into_inner();

            map.insert(artifact_id.clone(), artifact.clone());

            if !artifact.artifacts.is_empty() {
                Box::pin(load_artifacts(map, artifact.artifacts, service)).await?
            }
        }
    }

    Ok(())
}

pub async fn load_config<'a>(
    artifact: &String,
    service: &mut ConfigServiceClient<Channel>,
) -> Result<(HashMap<ArtifactId, Artifact>, Vec<ArtifactId>)> {
    let response = match service.get_config(ConfigRequest {}).await {
        Ok(res) => res,
        Err(error) => {
            bail!("failed to evaluate config: {}", error);
        }
    };

    let config = response.into_inner();

    if !config.artifacts.iter().any(|p| p.name == artifact.as_str()) {
        bail!("Artifact not found: {}", artifact);
    }

    let mut artifacts_map = HashMap::<ArtifactId, Artifact>::new();

    load_artifacts(&mut artifacts_map, config.artifacts.clone(), service).await?;

    let mut artifacts_graph = DiGraphMap::<&ArtifactId, Artifact>::new();

    for (artifact_id, artifact) in artifacts_map.iter() {
        artifacts_graph.add_node(artifact_id);

        for output in artifact.artifacts.iter() {
            artifacts_graph.add_edge(artifact_id, output, artifact.clone());

            add_edges(
                &mut artifacts_graph,
                &artifacts_map,
                artifact,
                output,
                service,
            )
            .await?;
        }
    }

    let artifacts_order = match toposort(&artifacts_graph, None) {
        Err(err) => bail!("{:?}", err),
        Ok(order) => order,
    };

    let mut artifacts_order: Vec<ArtifactId> = artifacts_order.into_iter().cloned().collect();

    artifacts_order.reverse();

    print_artifacts(&artifacts_order);
    print_artifacts_total(&artifacts_order);

    Ok((artifacts_map, artifacts_order))
}

async fn add_edges<'a>(
    graph: &mut DiGraphMap<&'a ArtifactId, Artifact>,
    map: &HashMap<ArtifactId, Artifact>,
    artifact: &'a Artifact,
    artifact_output: &'a ArtifactId,
    service: &mut ConfigServiceClient<Channel>,
) -> Result<()> {
    if map.contains_key(artifact_output) {
        return Ok(());
    }

    graph.add_node(artifact_output);

    for output in artifact.artifacts.iter() {
        graph.add_edge(artifact_output, output, artifact.clone());

        Box::pin(add_edges(graph, map, artifact, output, service)).await?;
    }

    Ok(())
}
