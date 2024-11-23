use crate::{
    artifact::{build_artifact, step_env_artifact},
    ContextConfig,
};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub fn artifact(context: &mut ContextConfig) -> Result<ArtifactId> {
    let name = "rust-std";

    let target = match context.get_target() {
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        Aarch64Macos => "aarch64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        X8664Macos => "x86_64-apple-darwin",
        UnknownSystem => bail!("Unsupported system: {:?}", context.get_target()),
    };

    let systems = vec![
        Aarch64Linux.into(),
        Aarch64Macos.into(),
        X8664Linux.into(),
        X8664Macos.into(),
    ];

    let version = "1.78.0";

    let source = build_artifact(
        context,
        vec![],
        vec![],
        format!("{}-source", name).as_str(),
        formatdoc! {"
            curl -L -o ./rust-std-{version}-{target}.tar.gz \
                https://static.rust-lang.org/dist/rust-std-{version}-{target}.tar.gz

            tar -xvf ./rust-std-{version}-{target}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
        },
        vec![],
        systems.clone(),
    )?;

    build_artifact(
        context,
        vec![source.clone()],
        vec![],
        name,
        format!(
            "cp -prv {}/rust-std-{target}/. \"$VORPAL_OUTPUT\"",
            step_env_artifact(&source)
        ),
        vec![],
        systems,
    )
}
