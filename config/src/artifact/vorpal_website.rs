use crate::artifact::SYSTEMS;
use anyhow::Result;
use indoc::formatdoc;
use vorpal_sdk::{
    artifact::{bun::Bun, get_env_key, step, Artifact, ArtifactSource},
    context::ConfigContext,
};

#[derive(Default)]
pub struct VorpalWebsite {}

impl VorpalWebsite {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let bun = Bun::new().build(context).await?;
        let bun_bin = format!("{}/bin", get_env_key(&bun));

        let source = ArtifactSource::new("vorpal-website", "website")
            .with_excludes(vec!["node_modules".to_string(), "dist".to_string()])
            .build();

        let step_script = formatdoc! {r#"
            export SHARP_IGNORE_GLOBAL_LIBVIPS=1
            cd ./source/vorpal-website
            {bun_bin}/bun install
            {bun_bin}/bun run build
            cp -r dist/* $VORPAL_OUTPUT/
        "#};

        let steps = vec![
            step::shell(
                context,
                vec![bun.clone()],
                vec![format!("PATH={bun_bin}")],
                step_script,
                vec![],
            )
            .await?,
        ];

        Artifact::new("vorpal-website", steps, SYSTEMS.to_vec())
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
