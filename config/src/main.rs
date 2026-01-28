use crate::artifact::{
    vorpal::Vorpal, vorpal_job::VorpalJob, vorpal_process::VorpalProcess,
    vorpal_release::VorpalRelease, vorpal_shell::VorpalShell, vorpal_user::VorpalUser,
};
use anyhow::Result;
use vorpal_sdk::{
    artifact::{linux_vorpal::LinuxVorpal, linux_vorpal_slim::LinuxVorpalSlim, rsync::Rsync},
    context::get_context,
};

mod artifact;

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    match context.get_artifact_name() {
        "vorpal" => Vorpal::new().build(context).await?,
        "vorpal-image" => {
            let linux_vorpal = LinuxVorpal::new().build(context).await?;
            let rsync = Rsync::new().build(context).await?;

            // TODO: migrate oci artifact here

            LinuxVorpalSlim::new()
                .with_linux_vorpal(&linux_vorpal)
                .with_rsync(&rsync)
                .build(context)
                .await?
        }
        "vorpal-job" => VorpalJob::new().build(context).await?,
        "vorpal-process" => VorpalProcess::new().build(context).await?,
        "vorpal-release" => VorpalRelease::new().build(context).await?,
        "vorpal-shell" => VorpalShell::new().build(context).await?,
        "vorpal-user" => VorpalUser::new().build(context).await?,
        _ => "".to_string(),
    };

    context.run().await
}
