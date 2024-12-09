use crate::config::{
    artifact::{add_artifact, get_artifact_envkey},
    ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    let name = "cargo";

    let systems = vec![
        "aarch64-linux",
        "aarch64-macos",
        "x86_64-linux",
        "x86_64-macos",
    ];

    let source = add_artifact(
        context,
        vec![],
        vec![],
        format!("{}-source", name).as_str(),
        formatdoc! {"
            curl -L -o ./cargo-{version}-{target}.tar.gz \
                https://static.rust-lang.org/dist/cargo-{version}-{target}.tar.gz

            tar -xvf ./cargo-{version}-{target}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
            target = match context.get_target() {
                Aarch64Linux => "aarch64-unknown-linux-gnu",
                Aarch64Macos => "aarch64-apple-darwin",
                UnknownSystem => bail!("Unsupported cargo target: {:?}", context.get_target()),
                X8664Linux => "x86_64-unknown-linux-gnu",
                X8664Macos => "x86_64-apple-darwin",
            },
            version = "1.78.0",
        },
        vec![],
        systems.clone(),
    )
    .await?;

    add_artifact(
        context,
        vec![source.clone()],
        vec![],
        name,
        format!(
            "cp -prv {}/cargo/. \"$VORPAL_OUTPUT\"/",
            get_artifact_envkey(&source)
        ),
        vec![],
        systems,
    )
    .await
}
