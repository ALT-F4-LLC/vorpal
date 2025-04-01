use anyhow::Result;
use vorpal_sdk::{
    artifact::{go, goimports, gopls, language::rust, protoc, protoc_gen_go, protoc_gen_go_grpc},
    context::ConfigContext,
};

pub async fn build_shell(context: &mut ConfigContext) -> Result<String> {
    let name = "vorpal-shell";

    let go = go::build(context).await?;
    let goimports = goimports::build(context).await?;
    let gopls = gopls::build(context).await?;
    let protoc = protoc::build(context).await?;
    let protoc_gen_go = protoc_gen_go::build(context).await?;
    let protoc_gen_go_grpc = protoc_gen_go_grpc::build(context).await?;

    let artifacts = vec![
        go.clone(),
        goimports.clone(),
        gopls.clone(),
        protoc.clone(),
        protoc_gen_go.clone(),
        protoc_gen_go_grpc.clone(),
    ];

    rust::build_shell(context, artifacts, name).await
}

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "vorpal";

    let protoc = protoc::build(context).await?;

    let artifacts = vec![protoc.clone()];

    let excludes = vec![
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
        "vorpal-domains.svg",
        "vorpal-purpose.jpg",
    ];

    rust::build(context, artifacts, name, excludes).await
}
