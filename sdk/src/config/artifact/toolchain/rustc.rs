use crate::config::{
    artifact::{add_artifact, get_artifact_envkey, toolchain::rust_std},
    ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    let rust_std = rust_std::artifact(context).await?;

    let name = "rustc";

    let systems = vec![
        "aarch64-linux",
        "aarch64-macos",
        "x86_64-linux",
        "x86_64-macos",
    ];

    let target = match context.get_target() {
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        Aarch64Macos => "aarch64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        X8664Macos => "x86_64-apple-darwin",
        UnknownSystem => bail!("Unsupported rustc target: {:?}", context.get_target()),
    };

    let version = "1.78.0";

    let source = add_artifact(
        context,
        vec![],
        vec![],
        format!("{}-source", name).as_str(),
        formatdoc! {"
            curl -L -o ./rustc-{version}-{target}.tar.gz \
                https://static.rust-lang.org/dist/rustc-{version}-{target}.tar.gz

            tar -xvf ./rustc-{version}-{target}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
        },
        vec![],
        systems.clone(),
    )
    .await?;

    add_artifact(
        context,
        vec![rust_std.clone(), source.clone()],
        vec![],
        name,
        formatdoc! {"
            cp -prv {rustc_source}/rustc/. \"$VORPAL_OUTPUT\"

            cat \"{rust_std}/manifest.in\" >> \"$VORPAL_OUTPUT/manifest.in\"

            cp -prv \"{rust_std}/lib\" \"$VORPAL_OUTPUT\"",
            rust_std = get_artifact_envkey(&rust_std),
            rustc_source = get_artifact_envkey(&source),
        },
        vec![],
        systems,
    )
    .await
}
