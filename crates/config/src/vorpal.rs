use anyhow::Result;
use indoc::formatdoc;
use vorpal_sdk::{
    artifact::{
        gh, go, goimports, gopls, grpcurl,
        language::rust::{RustBuilder, RustShellBuilder},
        protoc, protoc_gen_go, protoc_gen_go_grpc, ArtifactTaskBuilder, VariableBuilder,
    },
    context::ConfigContext,
};

pub async fn package(context: &mut ConfigContext) -> Result<()> {
    let artifacts = vec![protoc::build(context).await?];

    let excludes = vec![
        ".cargo",
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
        "vendor",
        "vorpal-config",
        "vorpal-domains.svg",
        "vorpal-purpose.jpg",
    ];

    RustBuilder::new("vorpal")
        .with_artifacts(artifacts)
        .with_excludes(excludes)
        .build(context)
        .await?;

    Ok(())
}

pub async fn shell(context: &mut ConfigContext) -> Result<()> {
    let artifacts = vec![
        gh::build(context).await?,
        go::build(context).await?,
        goimports::build(context).await?,
        gopls::build(context).await?,
        grpcurl::build(context).await?,
        protoc::build(context).await?,
        protoc_gen_go::build(context).await?,
        protoc_gen_go_grpc::build(context).await?,
    ];

    RustShellBuilder::new("vorpal-shell")
        .with_artifacts(artifacts)
        .build(context)
        .await?;

    Ok(())
}

pub async fn release(context: &mut ConfigContext) -> Result<()> {
    let aarch64_darwin = VariableBuilder::new("aarch64-darwin")
        .with_require()
        .build(context)?
        .unwrap();

    // let aarch64_linux = VariableBuilder::new("aarch64-linux")
    //     .with_require()
    //     .build(context)?
    //     .unwrap();
    //
    // let x8664_darwin = VariableBuilder::new("x8664-darwin")
    //     .with_require()
    //     .build(context)?
    //     .unwrap();
    //
    // let x8664_linux = VariableBuilder::new("x8664-linux")
    //     .with_require()
    //     .build(context)?
    //     .unwrap();

    let branch_name = VariableBuilder::new("branch-name")
        .with_require()
        .build(context)?
        .unwrap();

    let aarch64_darwin = context.fetch_artifact(&aarch64_darwin).await?;
    // let aarch64_linux = context.fetch_artifact(&aarch64_linux).await?;
    // let x8664_darwin = context.fetch_artifact(&x8664_darwin).await?;
    // let x8664_linux = context.fetch_artifact(&x8664_linux).await?;

    let artifacts = vec![
        gh::build(context).await?,
        aarch64_darwin.clone(),
        // aarch64_linux.clone(),
        // x8664_darwin.clone(),
        // x8664_linux.clone(),
    ];

    let script = formatdoc! {r#"
        git clone \
            --branch {branch_name} \
            --depth 1 \
            git@github.com:ALT-F4-LLC/vorpal.git

        pushd vorpal

        git fetch --tags
        git tag --delete nightly || true
        git push origin :refs/tags/nightly || true
        gh release delete --yes nightly || true

        git tag nightly
        git push --tags

        gh release create \
            --notes "Nightly builds from main branch." \
            --prerelease \
            --title "nightly" \
            --verify-tag \
            nightly \
            /var/lib/vorpal/store/{aarch64_darwin}.tar.zst"#
    };

    ArtifactTaskBuilder::new("vorpal-release", script)
        .with_artifacts(artifacts)
        .build(context)
        .await?;

    Ok(())
}
