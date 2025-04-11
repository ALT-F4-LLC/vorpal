use anyhow::Result;
use vorpal_sdk::{
    artifact::{
        go, goimports, gopls,
        language::rust::{RustBuilder, RustShellBuilder},
        protoc, protoc_gen_go, protoc_gen_go_grpc,
    },
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    let vorpal_shell = RustShellBuilder::new("vorpal-shell")
        .with_artifacts(vec![
            go::build(context).await?,
            goimports::build(context).await?,
            gopls::build(context).await?,
            protoc::build(context).await?,
            protoc_gen_go::build(context).await?,
            protoc_gen_go_grpc::build(context).await?,
        ])
        .build(context)
        .await?;

    let vorpal = RustBuilder::new("vorpal")
        .with_artifacts(vec![protoc::build(context).await?])
        .with_excludes(vec![
            ".env",
            ".envrc",
            ".github",
            ".gitignore",
            ".packer",
            ".vagrant",
            "Dockerfile",
            "Vagrantfile",
            "dist",
            "makefile",
            "script",
            "sdk/go",
            "shell.nix",
            "vorpal-config",
            "vorpal-domains.svg",
            "vorpal-purpose.jpg",
        ])
        .build(context)
        .await?;

    context.run(vec![vorpal_shell, vorpal]).await
}
