use anyhow::Result;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;
use vorpal_sdk::config::{
    artifact::{
        get_artifact_envkey, go,
        language::rust::{
            get_rust_toolchain_target, get_rust_toolchain_version, rust_package, toolchain_artifact,
        },
        protoc,
        shell::shell_artifact,
    },
    ConfigContext,
};

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
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
    let rust_toolchain = toolchain_artifact(context, name).await?;
    let rust_toolchain_target = get_rust_toolchain_target(context.get_target())?;
    let protoc = protoc::artifact(context).await?;

    let artifacts = vec![go.clone(), protoc.clone(), rust_toolchain.clone()];

    let envs = vec![
        format!(
            "PATH={}/bin:{}/bin:{}/toolchains/{}-{}/bin:$PATH",
            get_artifact_envkey(&go),
            get_artifact_envkey(&protoc),
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
