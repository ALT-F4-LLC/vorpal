use anyhow::{bail, Result};
use petgraph::algo::toposort;
use petgraph::graphmap::DiGraphMap;
use std::collections::HashMap;
use tonic::transport::Channel;
use vorpal_schema::vorpal::{
    artifact::v0::{Artifact, ArtifactId},
    config::v0::config_service_client::ConfigServiceClient,
};

pub async fn get_artifacts(
    artifact: &Artifact,
    artifact_map: &mut HashMap<ArtifactId, Artifact>,
    config_service: &mut ConfigServiceClient<Channel>,
) -> Result<()> {
    for output in artifact.artifacts.iter() {
        let request = tonic::Request::new(output.clone());

        let response = match config_service.get_artifact(request).await {
            Ok(res) => res,
            Err(error) => {
                bail!("failed to evaluate config: {}", error);
            }
        };

        let artifact = response.into_inner();

        artifact_map.insert(output.clone(), artifact.clone());

        Box::pin(get_artifacts(&artifact, artifact_map, config_service)).await?;
    }

    Ok(())
}

pub async fn get_order<'a>(
    build_artifact: &'a HashMap<ArtifactId, Artifact>,
) -> Result<Vec<ArtifactId>> {
    // Populate the build graph

    let mut artifact_graph = DiGraphMap::<&ArtifactId, Artifact>::new();

    for (artifact_id, artifact) in build_artifact.iter() {
        artifact_graph.add_node(artifact_id);

        for output in artifact.artifacts.iter() {
            artifact_graph.add_edge(artifact_id, output, artifact.clone());
        }
    }

    let build_order = match toposort(&artifact_graph, None) {
        Err(err) => bail!("{:?}", err),
        Ok(order) => order,
    };

    let mut build_order: Vec<ArtifactId> = build_order.into_iter().cloned().collect();

    build_order.reverse();

    Ok(build_order)
}
