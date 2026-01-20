use crate::artifact::{vorpal::Vorpal, SYSTEMS};
use anyhow::Result;
use vorpal_sdk::{
    artifact::{get_env_key, Job},
    context::ConfigContext,
};

#[derive(Default)]
pub struct VorpalJob {}

impl VorpalJob {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let vorpal = Vorpal::new().build(context).await?;

        let script = format!("{}/bin/vorpal --version", get_env_key(&vorpal));

        Job::new("vorpal-job", script, SYSTEMS.to_vec())
            .with_artifacts(vec![vorpal.clone()])
            .build(context)
            .await
    }
}
