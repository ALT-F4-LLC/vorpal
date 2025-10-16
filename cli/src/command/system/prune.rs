use crate::command::store::paths::{
    get_root_artifact_alias_dir_path, get_root_artifact_archive_dir_path,
    get_root_artifact_config_dir_path, get_root_artifact_output_dir_path,
    get_root_sandbox_dir_path,
};
use anyhow::Result;
use tokio::fs::{create_dir_all, remove_dir_all};
use tracing::info;

pub async fn run(
    all: bool,
    artifact_aliases: bool,
    artifact_archives: bool,
    artifact_configs: bool,
    artifact_outputs: bool,
    sandboxes: bool,
) -> Result<()> {
    if artifact_aliases || all {
        info!("Pruning artifact aliases...");

        let artifact_alias_dir_path = get_root_artifact_alias_dir_path();

        remove_dir_all(&artifact_alias_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove artifact aliases: {}", e))?;

        create_dir_all(&artifact_alias_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create artifact aliases directory: {}", e))?;
    }

    if artifact_archives || all {
        info!("Pruning artifact archives...");

        let artifact_archive_dir_path = get_root_artifact_archive_dir_path();

        remove_dir_all(&artifact_archive_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove artifact archives: {}", e))?;

        create_dir_all(&artifact_archive_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create artifact archives directory: {}", e))?;
    }

    if artifact_configs || all {
        info!("Pruning artifact configs...");

        let artifact_config_dir_path = get_root_artifact_config_dir_path();

        remove_dir_all(&artifact_config_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove artifact configs: {}", e))?;

        create_dir_all(&artifact_config_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create artifact configs directory: {}", e))?;
    }

    if artifact_outputs || all {
        info!("Pruning artifact outputs...");

        let artifact_output_dir_path = get_root_artifact_output_dir_path();

        remove_dir_all(&artifact_output_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove artifact outputs: {}", e))?;

        create_dir_all(&artifact_output_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create artifact outputs directory: {}", e))?;
    }

    if sandboxes || all {
        info!("Pruning sandboxes...");

        let sandbox_dir_path = get_root_sandbox_dir_path();

        remove_dir_all(&sandbox_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove sandboxes: {}", e))?;

        create_dir_all(&sandbox_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create sandboxes directory: {}", e))?;
    }

    Ok(())
}
