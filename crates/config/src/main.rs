use anyhow::{anyhow, Result};
use indoc::formatdoc;
use vorpal_sdk::{artifact::ArtifactTaskBuilder, context::get_context};

mod vorpal;

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    match context.get_artifact_name() {
        "vorpal-shell" => vorpal::shell(context).await?,
        "vorpal" => vorpal::package(context).await?,
        "vorpal-release" => vorpal::release(context).await?,
        "vorpal-example" => {
            let script = formatdoc! {r#"
                vorpal --version
            "#};

            ArtifactTaskBuilder::new("vorpal-example", script)
                .with_artifacts(vec![vorpal::package(context).await?])
                .build(context)
                .await?
        }
        _ => {
            return Err(anyhow!("unknown: {}", context.get_artifact_name()));
        }
    };

    context.run().await
}
