use crate::artifact::{
    go, goimports, gopls, protoc, protoc_gen_go, protoc_gen_go_grpc, rust_toolchain,
};
use anyhow::Result;
use vorpal_sdk::{
    artifact::{get_env_key, language::rust, shell},
    context::ConfigContext,
};

pub async fn devshell(context: &mut ConfigContext) -> Result<String> {
    let name = "vorpal-shell";

    let target = context.get_target();

    let go = go::build(context).await?;
    let goimports = goimports::build(context).await?;
    let gopls = gopls::build(context).await?;
    let protoc = protoc::build(context).await?;
    let protoc_gen_go = protoc_gen_go::build(context).await?;
    let protoc_gen_go_grpc = protoc_gen_go_grpc::build(context).await?;
    let rust_toolchain = rust_toolchain::build(context).await?;
    let rust_toolchain_target = rust::get_toolchain_target(target)?;
    let rust_toolchain_version = rust::get_toolchain_version();

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
            get_env_key(&go),
            get_env_key(&goimports),
            get_env_key(&gopls),
            get_env_key(&protoc),
            get_env_key(&protoc_gen_go),
            get_env_key(&protoc_gen_go_grpc),
            get_env_key(&rust_toolchain),
            rust_toolchain_version,
            rust_toolchain_target
        ),
        format!("RUSTUP_HOME={}", get_env_key(&rust_toolchain)),
        format!(
            "RUSTUP_TOOLCHAIN={}-{}",
            rust_toolchain_version, rust_toolchain_target
        ),
    ];

    // Create shell artifact
    shell::build(context, artifacts, envs, name).await
}

// pub async fn package(context: &mut ConfigContext) -> Result<String> {
//     let excludes = vec![
//         ".env",
//         ".envrc",
//         ".github",
//         ".gitignore",
//         ".packer",
//         ".vagrant",
//         "Dockerfile",
//         "Vagrantfile",
//         "dist",
//         "makefile",
//         "script",
//         "sdk/go",
//         "shell.nix",
//         "vorpal-domains.svg",
//         "vorpal-purpose.jpg",
//     ];
//
//     rust::package(context, "vorpal", excludes).await
// }
