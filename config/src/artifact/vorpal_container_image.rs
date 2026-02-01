use crate::Vorpal;
use anyhow::Result;
use vorpal_sdk::{
    artifact::{linux_vorpal_slim::LinuxVorpalSlim, oci_image::OciImage},
    context::ConfigContext,
};

#[derive(Default)]
pub struct VorpalContainerImage {}

impl VorpalContainerImage {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let rootfs = LinuxVorpalSlim::new().build(context).await?;
        let vorpal = Vorpal::new().build(context).await?;

        OciImage::new("vorpal-container-image", &rootfs)
            .with_artifacts(vec![&vorpal])
            .build(context)
            .await
    }
}
