use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_sdk::{
    api::artifact::{
        ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    artifact::{
        get_env_key, gh, go, goimports, gopls, grpcurl,
        language::go::{get_goarch, get_goos},
        language::rust::RustBuilder,
        protoc, protoc_gen_go, protoc_gen_go_grpc, script, staticcheck, ArtifactArgumentBuilder,
        ArtifactProcessBuilder, ArtifactTaskBuilder,
    },
    context::{get_context, ConfigContext},
};

const SYSTEMS: [ArtifactSystem; 4] = [Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

async fn vorpal(context: &mut ConfigContext) -> Result<String> {
    let name = "vorpal";

    let digest = RustBuilder::new(name, SYSTEMS.to_vec())
        .with_bins(vec![name])
        .with_includes(vec!["cli", "sdk/rust"])
        .with_packages(vec!["vorpal-cli", "vorpal-sdk"])
        .build(context)
        .await?;

    Ok(digest)
}

async fn vorpal_devenv(context: &mut ConfigContext) -> Result<String> {
    let go = go::build(context).await?;
    let goimports = goimports::build(context).await?;
    let gopls = gopls::build(context).await?;
    let grpcurl = grpcurl::build(context).await?;
    let protoc = protoc::build(context).await?;
    let protoc_gen_go = protoc_gen_go::build(context).await?;
    let protoc_gen_go_grpc = protoc_gen_go_grpc::build(context).await?;
    let staticcheck = staticcheck::build(context).await?;

    let artifacts = vec![
        go,
        goimports,
        gopls,
        grpcurl,
        protoc,
        protoc_gen_go,
        protoc_gen_go_grpc,
        staticcheck,
    ];

    let goarch = get_goarch(context.get_system())?;
    let goos = get_goos(context.get_system())?;

    let environments = vec![
        "CGO_ENABLED=0".to_string(),
        format!("GOARCH={}", goarch),
        format!("GOOS={}", goos),
    ];

    script::devenv(
        context,
        artifacts,
        environments,
        "vorpal-devenv",
        vec![],
        SYSTEMS.to_vec(),
    )
    .await
}

async fn vorpal_process(context: &mut ConfigContext) -> Result<String> {
    let vorpal = vorpal(context).await?;
    let entrypoint = format!("{}/bin/vorpal", get_env_key(&vorpal));

    ArtifactProcessBuilder::new("vorpal-process", entrypoint.as_str(), SYSTEMS.to_vec())
        .with_arguments(vec![
            "--registry",
            "https://localhost:50051",
            "start",
            "--port",
            "50051",
        ])
        .with_artifacts(vec![vorpal])
        .build(context)
        .await
}

async fn vorpal_release(context: &mut ConfigContext) -> Result<String> {
    let aarch64_darwin = ArtifactArgumentBuilder::new("aarch64-darwin")
        .with_require()
        .build(context)?;

    let aarch64_linux = ArtifactArgumentBuilder::new("aarch64-linux")
        .with_require()
        .build(context)?;

    let branch_name = ArtifactArgumentBuilder::new("branch-name")
        .with_require()
        .build(context)?;

    let x8664_darwin = ArtifactArgumentBuilder::new("x8664-darwin")
        .with_require()
        .build(context)?;

    let x8664_linux = ArtifactArgumentBuilder::new("x8664-linux")
        .with_require()
        .build(context)?;

    // Fetch artifacts

    let aarch64_darwin = context.fetch_artifact(&aarch64_darwin.unwrap()).await?;
    let aarch64_linux = context.fetch_artifact(&aarch64_linux.unwrap()).await?;
    let gh = gh::build(context).await?;
    let x8664_darwin = context.fetch_artifact(&x8664_darwin.unwrap()).await?;
    let x8664_linux = context.fetch_artifact(&x8664_linux.unwrap()).await?;

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
        branch_name = branch_name.unwrap(),
        x8664_darwin = get_env_key(&x8664_darwin),
        x8664_linux = get_env_key(&x8664_linux),
    };

    ArtifactTaskBuilder::new("vorpal-release", script, SYSTEMS.to_vec())
        .with_artifacts(artifacts)
        .build(context)
        .await
}

async fn vorpal_test(context: &mut ConfigContext) -> Result<String> {
    let vorpal = vorpal(context).await?;
    let script = format!("{}/bin/vorpal --version", get_env_key(&vorpal));

    ArtifactTaskBuilder::new("vorpal-test", script, SYSTEMS.to_vec())
        .with_artifacts(vec![vorpal])
        .build(context)
        .await
}

async fn vorpal_userenv(context: &mut ConfigContext) -> Result<String> {
    let vorpal = vorpal(context).await?;

    let home_dir = home::home_dir();

    if home_dir.is_none() {
        bail!("home directory not found");
    }

    let home_dir = home_dir.unwrap();

    script::userenv(
        context,
        vec![vorpal.clone()],
        vec![],
        "vorpal-userenv",
        vec![(
            format!("/var/lib/vorpal/store/artifact/output/{vorpal}/bin/vorpal"),
            format!("{}/.vorpal/bin/vorpal", home_dir.display()),
        )],
        SYSTEMS.to_vec(),
    )
    .await
}

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;
    let context_artifact = context.get_artifact_name();

    match context_artifact {
        "vorpal" => vorpal(context).await?,
        "vorpal-devenv" => vorpal_devenv(context).await?,
        "vorpal-process" => vorpal_process(context).await?,
        "vorpal-release" => vorpal_release(context).await?,
        "vorpal-test" => vorpal_test(context).await?,
        "vorpal-userenv" => vorpal_userenv(context).await?,
        _ => bail!("unknown artifact: {}", context_artifact),
    };

    context.run().await
}
