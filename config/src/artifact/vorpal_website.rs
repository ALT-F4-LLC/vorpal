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

        let name = "vorpal-website";

        let source = ArtifactSource::new(name, ".")
            .with_includes(vec!["website".to_string()])
            .with_excludes(vec![
                "website/.astro".to_string(),
                "website/README.md".to_string(),
                "website/dist".to_string(),
                "website/node_modules".to_string(),
            ])
            .build();

        let step_script = formatdoc! {r#"
            pushd ./source/vorpal-website/website
            {bun_bin}/bun install
            {bun_bin}/bun run build
            cp -r dist/* $VORPAL_OUTPUT/
        "#};

        let steps = vec![
            step::shell(
                context,
                vec![bun.clone()],
                vec![
                    "ASTRO_TELEMETRY_DISABLED=1".to_string(),
                    format!("PATH={bun_bin}"),
                ],
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
