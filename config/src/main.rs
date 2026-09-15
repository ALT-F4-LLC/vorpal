//! Vorpal build config for the Vorpal repository itself: selects the
//! requested artifact by name and hands it to the SDK context.

use crate::artifact::{
    vorpal::Vorpal, vorpal_container_image::VorpalContainerImage, vorpal_job::VorpalJob,
    vorpal_process::VorpalProcess, vorpal_release::VorpalRelease, vorpal_shell::VorpalShell,
    vorpal_user::VorpalUser, vorpal_website::VorpalWebsite,
};
use anyhow::Result;
use vorpal_sdk::context::get_context;

mod artifact;

#[tokio::main]
async fn main() -> Result<()> {
    let mut context = get_context().await?;

    match context.get_artifact_name() {
        "vorpal" => Vorpal::new().build(&mut context).await?,
        "vorpal-container-image" => VorpalContainerImage::new().build(&mut context).await?,
        "vorpal-job" => VorpalJob::new().build(&mut context).await?,
        "vorpal-process" => VorpalProcess::new().build(&mut context).await?,
        "vorpal-release" => VorpalRelease::new().build(&mut context).await?,
        "vorpal-shell" => VorpalShell::new().build(&mut context).await?,
        "vorpal-user" => VorpalUser::new().build(&mut context).await?,
        "vorpal-website" => VorpalWebsite::new().build(&mut context).await?,
        _ => String::new(),
    };

    context.run().await
}
