use anyhow::Result;
use indoc::formatdoc;
use vorpal_sdk::{
    api::artifact::{
        ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    artifact::{
        get_env_key,
        gh::Gh,
        go::Go,
        goimports::Goimports,
        gopls::Gopls,
        grpcurl::Grpcurl,
        language::go::{get_goarch, get_goos},
        language::rust::Rust,
        protoc::Protoc,
        protoc_gen_go::ProtocGenGo,
        protoc_gen_go_grpc::ProtocGenGoGrpc,
        staticcheck::Staticcheck,
        Argument, Job, Process, ProjectEnvironment, UserEnvironment,
    },
    context::get_context,
};

const SYSTEMS: [ArtifactSystem; 4] = [Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    // Dependencies

    let gh = Gh::new().build(context).await?;
    let go = Go::new().build(context).await?;
    let goimports = Goimports::new().build(context).await?;
    let gopls = Gopls::new().build(context).await?;
    let grpcurl = Grpcurl::new().build(context).await?;
    let protoc = Protoc::new().build(context).await?;
    let protoc_gen_go = ProtocGenGo::new().build(context).await?;
    let protoc_gen_go_grpc = ProtocGenGoGrpc::new().build(context).await?;
    let staticcheck = Staticcheck::new().build(context).await?;

    // Rust artifact

    let vorpal = Rust::new("vorpal", SYSTEMS.to_vec())
        .with_bins(vec!["vorpal"])
        .with_includes(vec!["cli", "sdk/rust"])
        .with_packages(vec!["vorpal-cli", "vorpal-sdk"])
        .build(context)
        .await?;

    // Project environments

    ProjectEnvironment::new("vorpal-dev", SYSTEMS.to_vec())
        .with_artifacts(vec![
            go,
            goimports,
            gopls,
            grpcurl,
            protoc,
            protoc_gen_go,
            protoc_gen_go_grpc,
            staticcheck,
        ])
        .with_environments(vec![
            "CGO_ENABLED=0".to_string(),
            format!("GOARCH={}", get_goarch(context.get_system())?),
            format!("GOOS={}", get_goos(context.get_system())?),
        ])
        .with_secrets(vec![])
        .build(context)
        .await?;

    // Processes

    Process::new(
        "vorpal-process",
        format!("{}/bin/vorpal", get_env_key(&vorpal)).as_str(),
        SYSTEMS.to_vec(),
    )
    .with_arguments(vec![
        "--registry",
        "https://localhost:50051",
        "services",
        "start",
        "--port",
        "50051",
    ])
    .with_artifacts(vec![vorpal.clone()])
    .build(context)
    .await?;

    // Jobs

    Job::new(
        "vorpal-test",
        format!("{}/bin/vorpal --version", get_env_key(&vorpal)),
        SYSTEMS.to_vec(),
    )
    .with_artifacts(vec![vorpal.clone()])
    .build(context)
    .await?;

    // User environments

    UserEnvironment::new("vorpal-user", SYSTEMS.to_vec())
        .with_artifacts(vec![])
        .with_environments(vec!["PATH=$HOME/.vorpal/bin".to_string()])
        .with_symlinks(vec![
            (
                "$HOME/Development/repository/github.com/ALT-F4-LLC/vorpal.git/main/target/debug/vorpal",
                "$HOME/.vorpal/bin/vorpal",
            ),
        ])
        .build(context)
        .await?;

    // Vorpal release

    if context.get_artifact_name() == "vorpal-release" {
        // Setup arguments

        let branch_name = Argument::new("branch-name").with_require().build(context)?;

        let darwin_aarch64 = Argument::new("aarch64-darwin")
            .with_require()
            .build(context)?;

        let darwin_x8664 = Argument::new("x8664-darwin")
            .with_require()
            .build(context)?;

        let linux_aarch64 = Argument::new("aarch64-linux")
            .with_require()
            .build(context)?;

        let linux_x8664 = Argument::new("x8664-linux").with_require().build(context)?;

        // Fetch artifacts

        let aarch64_darwin = context.fetch_artifact(&darwin_aarch64.unwrap()).await?;
        let aarch64_linux = context.fetch_artifact(&linux_aarch64.unwrap()).await?;
        let x8664_darwin = context.fetch_artifact(&darwin_x8664.unwrap()).await?;
        let x8664_linux = context.fetch_artifact(&linux_x8664.unwrap()).await?;

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

        Job::new("vorpal-release", script, SYSTEMS.to_vec())
            .with_artifacts(vec![
                aarch64_darwin,
                aarch64_linux,
                gh,
                x8664_darwin,
                x8664_linux,
            ])
            .build(context)
            .await?;
    }

    context.run().await
}
