use anyhow::Result;
use vorpal_sdk::config::{
    artifact::language::rust::{rust_package, rust_shell},
    get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Get the context
    let context = &mut get_context().await?;

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

    // Create artifacts
    let artifacts = vec![
        rust_package(context, "vorpal", excludes).await?,
        rust_shell(context, "vorpal").await?,
    ];

    // Run the context
    context.run(artifacts).await
}
