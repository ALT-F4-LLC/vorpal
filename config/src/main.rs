use crate::artifact::{
    vorpal::Vorpal, vorpal_job::VorpalJob, vorpal_process::VorpalProcess,
    vorpal_release::VorpalRelease, vorpal_shell::VorpalShell, vorpal_user::VorpalUser,
};
use anyhow::Result;
use vorpal_sdk::context::get_context;

mod artifact;

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    match context.get_artifact_name() {
        "vorpal" => Vorpal::new().build(context).await?,
        "vorpal-job" => VorpalJob::new().build(context).await?,
        "vorpal-process" => VorpalProcess::new().build(context).await?,
        "vorpal-release" => VorpalRelease::new().build(context).await?,
        "vorpal-shell" => VorpalShell::new().build(context).await?,
        "vorpal-user" => VorpalUser::new().build(context).await?,
        _ => "".to_string(),
    };

    context.run().await
}
