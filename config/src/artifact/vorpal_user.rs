use crate::artifact::SYSTEMS;
use anyhow::Result;
use vorpal_sdk::{artifact::UserEnvironment, context::ConfigContext};

#[derive(Default)]
pub struct VorpalUser {}

impl VorpalUser {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        UserEnvironment::new("vorpal-user", SYSTEMS.to_vec())
            .with_artifacts(vec![])
            .with_environments(vec!["PATH=$HOME/.vorpal/bin".to_string()])
            .with_symlinks(vec![
                (
                    "$HOME/Development/repository/github.com/ALT-F4-LLC/vorpal.git/main/target/debug/vorpal",
                    "$HOME/.vorpal/bin/vorpal",
                ),
            ])
            .build(context)
            .await
    }
}
