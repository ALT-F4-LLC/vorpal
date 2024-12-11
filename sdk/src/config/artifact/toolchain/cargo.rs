use crate::config::{artifact::add_artifact, ConfigContext};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(
    context: &mut ConfigContext,
    override_version: Option<String>,
) -> Result<ArtifactId> {
    let name = "cargo";

    let target = match context.get_target() {
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        Aarch64Macos => "aarch64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        X8664Macos => "x86_64-apple-darwin",
        UnknownSystem => bail!("Invalid cargo target: {:?}", context.get_target()),
    };

    let mut version = "1.80.1".to_string();

    if let Some(v) = override_version {
        version = v;
    }

    add_artifact(
        context,
        vec![],
        vec![],
        name,
        formatdoc! {"
            curl -L -o ./{name}-{version}-{target}.tar.gz \
                https://static.rust-lang.org/dist/{name}-{version}-{target}.tar.gz

            tar -xvf ./{name}-{version}-{target}.tar.gz -C source --strip-components=1

            cp -prv \"./source/{name}/.\" \"$VORPAL_OUTPUT\"",
        },
        vec![],
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ],
    )
    .await
}
