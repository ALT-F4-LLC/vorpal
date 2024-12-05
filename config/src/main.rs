use anyhow::Result;
use vorpal_schema::vorpal::config::v0::Config;
use vorpal_sdk::config::{
    artifact::{add_systems, language::rust},
    cli::execute,
    ContextConfig,
};

// 1. Create a function that returns a populated configuration
fn config(context: &mut ContextConfig) -> Result<Config> {
    // NOTE: custom logic can be added anywhere in this function

    // 2. Define artifact parameters
    let artifact_excludes = vec![".env", ".packer", ".vagrant", "script"];
    let artifact_name = "vorpal";
    let artifact_systems = add_systems(vec!["aarch64-linux", "aarch64-macos"])?;

    // 3. Create artifact (rust)
    let artifact = rust::artifact(context, artifact_excludes, artifact_name, artifact_systems)?;

    // 4. Return config with artifact
    Ok(Config {
        artifacts: vec![artifact],
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    // 5. Execute the configuration
    execute(config).await
}
