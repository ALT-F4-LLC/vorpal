use anyhow::Result;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;
use vorpal_sdk::config::{
    artifact::language::rust::{rust_package, rust_shell},
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
    rust_shell(context, "vorpal").await
}
