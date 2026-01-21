use crate::artifact::{vorpal::Vorpal, SYSTEMS};
use anyhow::Result;
use vorpal_sdk::{
    artifact::{get_env_key, Process},
    context::ConfigContext,
};

#[derive(Default)]
pub struct VorpalProcess {}

impl VorpalProcess {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let vorpal = Vorpal::new().build(context).await?;

        Process::new(
            "vorpal-process",
            format!("{}/bin/vorpal", get_env_key(&vorpal)).as_str(),
            SYSTEMS.to_vec(),
        )
        .with_arguments(vec![
            "--registry",
            "https://localhost:50051",
            "services",
            "start",
            "--port",
            "50051",
        ])
        .with_artifacts(vec![vorpal])
        .build(context)
        .await
    }
}
