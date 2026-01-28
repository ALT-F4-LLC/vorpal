use crate::command::store::paths::{
    get_root_artifact_alias_dir_path, get_root_artifact_archive_dir_path,
    get_root_artifact_config_dir_path, get_root_artifact_output_dir_path,
    get_root_sandbox_dir_path,
};
use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::fs::{create_dir_all, remove_dir_all};
use tracing::{info, warn};
use walkdir::WalkDir;

fn calculate_dir_size(path: PathBuf) -> u64 {
    if !path.exists() {
        return 0;
    }

    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

async fn calculate_dir_size_async(path: &Path) -> u64 {
    let path_display = path.display().to_string();
    let p = path.to_path_buf();
    tokio::task::spawn_blocking(move || calculate_dir_size(p))
        .await
        .unwrap_or_else(|e| {
            warn!(
                "Task failed calculating directory size for {}: {}",
                path_display, e
            );
            0
        })
}

pub async fn run(
    all: bool,
    artifact_aliases: bool,
    artifact_archives: bool,
    artifact_configs: bool,
    artifact_outputs: bool,
    sandboxes: bool,
) -> Result<()> {
    let mut total_freed: u64 = 0;

    if artifact_aliases || all {
        let artifact_alias_dir_path = get_root_artifact_alias_dir_path();
        let size = calculate_dir_size_async(&artifact_alias_dir_path).await;
        total_freed += size;

        remove_dir_all(&artifact_alias_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove artifact aliases: {}", e))?;

        create_dir_all(&artifact_alias_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create artifact aliases directory: {}", e))?;

        info!("Pruned artifact aliases: freed {}", format_bytes(size));
    }

    if artifact_archives || all {
        let artifact_archive_dir_path = get_root_artifact_archive_dir_path();
        let size = calculate_dir_size_async(&artifact_archive_dir_path).await;
        total_freed += size;

        remove_dir_all(&artifact_archive_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove artifact archives: {}", e))?;

        create_dir_all(&artifact_archive_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create artifact archives directory: {}", e))?;

        info!("Pruned artifact archives: freed {}", format_bytes(size));
    }

    if artifact_configs || all {
        let artifact_config_dir_path = get_root_artifact_config_dir_path();
        let size = calculate_dir_size_async(&artifact_config_dir_path).await;
        total_freed += size;

        remove_dir_all(&artifact_config_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove artifact configs: {}", e))?;

        create_dir_all(&artifact_config_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create artifact configs directory: {}", e))?;

        info!("Pruned artifact configs: freed {}", format_bytes(size));
    }

    if artifact_outputs || all {
        let artifact_output_dir_path = get_root_artifact_output_dir_path();
        let size = calculate_dir_size_async(&artifact_output_dir_path).await;
        total_freed += size;

        remove_dir_all(&artifact_output_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove artifact outputs: {}", e))?;

        create_dir_all(&artifact_output_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create artifact outputs directory: {}", e))?;

        info!("Pruned artifact outputs: freed {}", format_bytes(size));
    }

    if sandboxes || all {
        let sandbox_dir_path = get_root_sandbox_dir_path();
        let size = calculate_dir_size_async(&sandbox_dir_path).await;
        total_freed += size;

        remove_dir_all(&sandbox_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to remove sandboxes: {}", e))?;

        create_dir_all(&sandbox_dir_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create sandboxes directory: {}", e))?;

        info!("Pruned sandboxes: freed {}", format_bytes(size));
    }

    info!("Total space freed: {}", format_bytes(total_freed));

    Ok(())
}
