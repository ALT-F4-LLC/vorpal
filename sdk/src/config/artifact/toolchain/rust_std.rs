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
    let name = "rust-std";

    let target = match context.get_target() {
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        Aarch64Macos => "aarch64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        X8664Macos => "x86_64-apple-darwin",
        UnknownSystem => bail!("Unsupported system: {:?}", context.get_target()),
    };

    let systems = vec![
        "aarch64-linux",
        "aarch64-macos",
        "x86_64-linux",
        "x86_64-macos",
    ];

    let version = "1.78.0";

    let source = add_artifact(
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
    )
    .await?;

    add_artifact(
        context,
        vec![source.clone()],
        vec![],
        name,
        format!(
            "cp -prv {}/rust-std-{target}/. \"$VORPAL_OUTPUT/\"",
            get_artifact_envkey(&source)
        ),
        vec![],
        systems,
    )
    .await
}
