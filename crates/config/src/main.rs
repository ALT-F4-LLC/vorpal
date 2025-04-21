use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_sdk::{
    artifact::{
        get_env_key, gh, go, goimports, gopls, grpcurl,
        language::rust::{RustBuilder, RustShellBuilder},
        nginx, protoc, protoc_gen_go, protoc_gen_go_grpc, staticcheck, ArtifactProcessBuilder,
        ArtifactTaskBuilder, ArtifactVariableBuilder,
    },
    context::{get_context, ConfigContext},
};

async fn vorpal(context: &mut ConfigContext) -> Result<String> {
    let protoc = protoc::build(context).await?;

    let name = "vorpal";

    RustBuilder::new(name)
        .with_artifacts(vec![protoc])
        .with_bins(vec![name])
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
        .await
}

async fn vorpal_process(context: &mut ConfigContext) -> Result<String> {
    let vorpal = vorpal(context).await?;

    let entrypoint = format!("{}/bin/vorpal", get_env_key(&vorpal));

    ArtifactProcessBuilder::new("vorpal-process", entrypoint.as_str())
        .with_arguments(vec![
            "--registry",
            "http://localhost:50051",
            "start",
            "--port",
            "50051",
        ])
        .with_artifacts(vec![vorpal])
        .build(context)
        .await
}

async fn vorpal_release(context: &mut ConfigContext) -> Result<String> {
    let aarch64_darwin = ArtifactVariableBuilder::new("aarch64-darwin")
        .with_require()
        .build(context)?
        .unwrap();

    let aarch64_linux = ArtifactVariableBuilder::new("aarch64-linux")
        .with_require()
        .build(context)?
        .unwrap();

    let branch_name = ArtifactVariableBuilder::new("branch-name")
        .with_require()
        .build(context)?
        .unwrap();

    let x8664_darwin = ArtifactVariableBuilder::new("x8664-darwin")
        .with_require()
        .build(context)?
        .unwrap();

    let x8664_linux = ArtifactVariableBuilder::new("x8664-linux")
        .with_require()
        .build(context)?
        .unwrap();

    // Fetch artifacts

    let aarch64_darwin = context.fetch_artifact(&aarch64_darwin).await?;
    let aarch64_linux = context.fetch_artifact(&aarch64_linux).await?;
    let gh = gh::build(context).await?;
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
            {aarch64_darwin}.tar.zst \
            {aarch64_linux}.tar.zst \
            {x8664_darwin}.tar.zst \
            {x8664_linux}.tar.zst"#,
        aarch64_darwin = get_env_key(&aarch64_darwin),
        aarch64_linux = get_env_key(&aarch64_linux),
        x8664_darwin = get_env_key(&x8664_darwin),
        x8664_linux = get_env_key(&x8664_linux),
    };

    ArtifactTaskBuilder::new("vorpal-release", script)
        .with_artifacts(artifacts)
        .build(context)
        .await
}

async fn vorpal_shell(context: &mut ConfigContext) -> Result<String> {
    let gh = gh::build(context).await?;
    let go = go::build(context).await?;
    let goimports = goimports::build(context).await?;
    let gopls = gopls::build(context).await?;
    let grpcurl = grpcurl::build(context).await?;
    let nginx = nginx::build(context).await?;
    let protoc = protoc::build(context).await?;
    let protoc_gen_go = protoc_gen_go::build(context).await?;
    let protoc_gen_go_grpc = protoc_gen_go_grpc::build(context).await?;
    let staticcheck = staticcheck::build(context).await?;
    let vorpal_process = vorpal_process(context).await?;

    RustShellBuilder::new("vorpal-shell")
        .with_artifacts(vec![
            gh,
            go,
            goimports,
            gopls,
            grpcurl,
            nginx,
            protoc,
            protoc_gen_go,
            protoc_gen_go_grpc,
            staticcheck,
            vorpal_process,
        ])
        .build(context)
        .await
}

async fn vorpal_test(context: &mut ConfigContext) -> Result<String> {
    let vorpal = vorpal(context).await?;

    let script = format!("{}/bin/vorpal --version", get_env_key(&vorpal));

    ArtifactTaskBuilder::new("vorpal-test", script)
        .with_artifacts(vec![vorpal])
        .build(context)
        .await
}

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;
    let context_artifact = context.get_artifact_name();

    match context_artifact {
        "vorpal" => vorpal(context).await?,
        "vorpal-process" => vorpal_process(context).await?,
        "vorpal-release" => vorpal_release(context).await?,
        "vorpal-shell" => vorpal_shell(context).await?,
        "vorpal-test" => vorpal_test(context).await?,
        _ => bail!("unknown artifact: {}", context_artifact),
    };

    context.run().await
}
