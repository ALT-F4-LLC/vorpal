use anyhow::Result;
use tracing::info;
use vorpal_sdk::{
    artifact::system::get_system_default_str,
    context::parse_artifact_alias,
};

pub async fn run(alias: &str, args: &[String]) -> Result<()> {
    let parsed = parse_artifact_alias(alias)?;
    let system = get_system_default_str();

    info!(
        "run: name={}, namespace={}, system={system}, tag={}",
        parsed.name, parsed.namespace, parsed.tag
    );
    info!("run: args={args:?}");

    Ok(())
}
