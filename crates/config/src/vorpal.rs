use crate::artifact::{go, goimports, gopls, protoc_gen_go, protoc_gen_go_grpc};
use anyhow::{bail, Result};
use vorpal_schema::config::v0::{
    ConfigArtifactSystem,
    ConfigArtifactSystem::{Aarch64Darwin, Aarch64Linux, UnknownSystem, X8664Darwin, X8664Linux},
};
use vorpal_sdk::{artifact::language::rust, context::ConfigContext};

fn protoc_hash(target: ConfigArtifactSystem) -> Result<&'static str> {
    let hash = match target {
        Aarch64Darwin => "c07583ea769b3a2bb08c32af98b43d2158a4fd4b6bfd0f6e83737e4e8db8e7c8",
        Aarch64Linux => "",
        X8664Darwin => "",
        X8664Linux => "",
        UnknownSystem => bail!("unsupported 'protoc' system"),
    };

    Ok(hash)
}

pub async fn devshell(context: &mut ConfigContext) -> Result<String> {
    let name = "vorpal-shell";

    let target = context.get_target();

    let go = go::build(context).await?;
    let goimports = goimports::build(context).await?;
    let gopls = gopls::build(context).await?;
    let protoc = context.fetch_artifact(protoc_hash(target)?).await?;
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

    rust::devshell(context, artifacts, name).await
}

pub async fn package(context: &mut ConfigContext) -> Result<String> {
    let name = "vorpal";

    let protoc = context
        .fetch_artifact(protoc_hash(context.get_target())?)
        .await?;

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

    rust::package(context, artifacts, name, excludes).await
}
