use anyhow::Result;
use indoc::formatdoc;
use vorpal_sdk::{
    artifact::{
        gh, go, goimports, gopls, grpcurl,
        language::rust::{RustBuilder, RustShellBuilder},
        protoc, protoc_gen_go, protoc_gen_go_grpc, rust_analyzer, staticcheck, ArtifactTaskBuilder,
        VariableBuilder,
    },
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    // Artifacts

    let gh = gh::build(context).await?;
    let go = go::build(context).await?;
    let goimports = goimports::build(context).await?;
    let gopls = gopls::build(context).await?;
    let grpcurl = grpcurl::build(context).await?;
    let protoc = protoc::build(context).await?;
    let protoc_gen_go = protoc_gen_go::build(context).await?;
    let protoc_gen_go_grpc = protoc_gen_go_grpc::build(context).await?;
    let rust_analyzer = rust_analyzer::build(context).await?;
    let staticcheck = staticcheck::build(context).await?;

    RustShellBuilder::new("vorpal-shell")
        .with_artifacts(vec![
            gh.clone(),
            go,
            goimports,
            gopls,
            grpcurl,
            protoc.clone(),
            protoc_gen_go,
            protoc_gen_go_grpc,
            rust_analyzer,
            staticcheck,
        ])
        .build(context)
        .await?;

    let vorpal = RustBuilder::new("vorpal")
        .with_artifacts(vec![protoc])
        .with_bins(vec!["vorpal"])
        .with_packages(vec![
            "crates/agent",
            "crates/cli",
            "crates/registry",
            "crates/schema",
            "crates/sdk",
            "crates/store",
            "crates/worker",
        ])
        .build(context)
        .await?;

    // Tasks

    match context.get_artifact_name() {
        "vorpal-example" => {
            ArtifactTaskBuilder::new("vorpal-example", "vorpal --version".to_string())
                .with_artifacts(vec![vorpal])
                .build(context)
                .await?;
        }

        "vorpal-release" => {
            let aarch64_darwin = VariableBuilder::new("aarch64-darwin")
                .with_require()
                .build(context)?
                .unwrap();

            let aarch64_linux = VariableBuilder::new("aarch64-linux")
                .with_require()
                .build(context)?
                .unwrap();

            let branch_name = VariableBuilder::new("branch-name")
                .with_require()
                .build(context)?
                .unwrap();

            let x8664_darwin = VariableBuilder::new("x8664-darwin")
                .with_require()
                .build(context)?
                .unwrap();

            let x8664_linux = VariableBuilder::new("x8664-linux")
                .with_require()
                .build(context)?
                .unwrap();

            // Fetch artifacts

            let aarch64_darwin = context.fetch_artifact(&aarch64_darwin).await?;
            let aarch64_linux = context.fetch_artifact(&aarch64_linux).await?;
            let x8664_darwin = context.fetch_artifact(&x8664_darwin).await?;
            let x8664_linux = context.fetch_artifact(&x8664_linux).await?;

            let artifacts = vec![
                aarch64_darwin.clone(),
                aarch64_linux.clone(),
                gh,
                x8664_darwin.clone(),
                x8664_linux.clone(),
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
                    /var/lib/vorpal/store/{aarch64_darwin}.tar.zst \
                    /var/lib/vorpal/store/{aarch64_linux}.tar.zst \
                    /var/lib/vorpal/store/{x8664_darwin}.tar.zst \
                    /var/lib/vorpal/store/{x8664_linux}.tar.zst"#
            };

            ArtifactTaskBuilder::new("vorpal-release", script)
                .with_artifacts(artifacts)
                .build(context)
                .await?;
        }

        _ => {}
    };

    context.run().await
}
