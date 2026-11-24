use vorpal_sdk::{
    artifact::{get_env_key, system, Job},
    context::get_context,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut ctx = get_context().await?;

    let tool = build_my_tool(&mut ctx).await?;
    let script = format!("{}/bin/my-tool --version", get_env_key(&tool));

    Job::new("my-job", script, system::SYSTEMS)
        .with_artifacts(vec![tool])
        .build(&mut ctx)
        .await?;

    ctx.run().await
}
