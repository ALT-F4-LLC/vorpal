use crate::config::artifact::{add_artifact, language::rust::get_toolchain_target, ConfigContext};
use anyhow::{bail, Result};
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSource,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let hash = match context.get_target() {
        Aarch64Linux => "53b7ebc9645f5a5f12429f1a06e2d379a9ae8c7756f1ace865b4b53e65945d70",
        Aarch64Macos => "f7fb80e784da221199778d086fe3320769b86bc316f13ffcfe4de72b02d39df3",
        X8664Linux => "1234567890",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    let name = "rustfmt";

    let target = get_toolchain_target(context.get_target())?;

    add_artifact(
        context,
        vec![],
        vec![],
        name,
        format!("cp -prv \"./source/{name}/{name}-{version}-{target}/{name}-preview/.\" \"$VORPAL_OUTPUT\""),
        vec![ArtifactSource {
            excludes: vec![],
            hash: Some(hash.to_string()),
            includes: vec![],
            name: name.to_string(),
            path: format!("https://static.rust-lang.org/dist/{name}-{version}-{target}.tar.gz"),
        }],
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ],
    )
    .await
}
