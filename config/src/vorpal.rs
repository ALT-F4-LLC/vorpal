use anyhow::Result;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;
use vorpal_sdk::{
    artifact::{
        get_artifact_envkey, go, goimports, gopls,
        language::rust::{
            get_rust_toolchain_target, get_rust_toolchain_version, rust_package, toolchain_artifact,
        },
        protoc, protoc_gen_go, protoc_gen_go_grpc,
        shell::shell_artifact,
    },
    context::ConfigContext,
};

pub async fn package(context: &mut ConfigContext) -> Result<ArtifactId> {
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

    rust_package(context, "vorpal", excludes).await
}

pub async fn shell(context: &mut ConfigContext) -> Result<ArtifactId> {
    let name = "vorpal";

    let go = go::artifact(context).await?;
    let goimports = goimports::artifact(context).await?;
    let gopls = gopls::artifact(context).await?;
    let protoc = protoc::artifact(context).await?;
    let protoc_gen_go = protoc_gen_go::artifact(context).await?;
    let protoc_gen_go_grpc = protoc_gen_go_grpc::artifact(context).await?;
    let rust_toolchain = toolchain_artifact(context, name).await?;
    let rust_toolchain_target = get_rust_toolchain_target(context.get_target())?;

    let artifacts = vec![
        go.clone(),
        goimports.clone(),
        gopls.clone(),
        protoc.clone(),
        protoc_gen_go.clone(),
        protoc_gen_go_grpc.clone(),
        rust_toolchain.clone(),
    ];

    let envs = vec![
        format!(
            "PATH={}/bin:{}/bin:{}/bin:{}/bin:{}/bin:{}/bin:{}/toolchains/{}-{}/bin:$PATH",
            get_artifact_envkey(&go),
            get_artifact_envkey(&goimports),
            get_artifact_envkey(&gopls),
            get_artifact_envkey(&protoc),
            get_artifact_envkey(&protoc_gen_go),
            get_artifact_envkey(&protoc_gen_go_grpc),
            get_artifact_envkey(&rust_toolchain),
            get_rust_toolchain_version(),
            rust_toolchain_target
        ),
        format!("RUSTUP_HOME={}", get_artifact_envkey(&rust_toolchain)),
        format!(
            "RUSTUP_TOOLCHAIN={}-{}",
            get_rust_toolchain_version(),
            rust_toolchain_target
        ),
    ];

    // Create shell artifact
    shell_artifact(context, artifacts, envs, name).await
}
