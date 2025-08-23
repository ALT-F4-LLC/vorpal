use crate::command::store::paths::{
    get_artifact_alias_dir_path, get_artifact_archive_dir_path, get_artifact_config_dir_path,
    get_artifact_output_dir_path, get_root_sandbox_dir_path,
};
use anyhow::Result;
use tokio::fs::{create_dir_all, remove_dir_all};
use tracing::info;

pub async fn run(
    aliases: bool,
    all: bool,
    archives: bool,
    configs: bool,
    outputs: bool,
    sandboxes: bool,
) -> Result<()> {
    if aliases || all {
        info!("Pruning artifact aliases...");

        let artifact_alias_dir_path = get_artifact_alias_dir_path();

        remove_dir_all(&artifact_alias_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove artifact aliases: {}", e))?;

        create_dir_all(&artifact_alias_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create artifact aliases directory: {}", e))?;
    }

    if archives || all {
        info!("Pruning artifact archives...");

        let artifact_archive_dir_path = get_artifact_archive_dir_path();

        remove_dir_all(&artifact_archive_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove artifact archives: {}", e))?;

        create_dir_all(&artifact_archive_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create artifact archives directory: {}", e))?;
    }

    if configs || all {
        info!("Pruning artifact configs...");

        let artifact_config_dir_path = get_artifact_config_dir_path();

        remove_dir_all(&artifact_config_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove artifact configs: {}", e))?;

        create_dir_all(&artifact_config_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create artifact configs directory: {}", e))?;
    }

    if outputs || all {
        info!("Pruning artifact outputs...");

        let artifact_output_dir_path = get_artifact_output_dir_path();

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
