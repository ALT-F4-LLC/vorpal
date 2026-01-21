use crate::artifact::SYSTEMS;
use anyhow::Result;
use vorpal_sdk::{artifact::language::rust::Rust, context::ConfigContext};

#[derive(Default)]
pub struct Vorpal {}

impl Vorpal {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        Rust::new("vorpal", SYSTEMS.to_vec())
            .with_bins(vec!["vorpal"])
            .with_includes(vec!["cli", "sdk/rust"])
            .with_packages(vec!["vorpal-cli", "vorpal-sdk"])
            .build(context)
            .await
    }
}
