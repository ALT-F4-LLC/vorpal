use crate::config::artifact::{add_artifact, language::rust::get_toolchain_target, ConfigContext};
use anyhow::{bail, Result};
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSource,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let hash = match context.get_target() {
        Aarch64Linux => "a4c911c6e4da46c65ebfc61166201e7a2abbc36966ee3ae4942173c9eece15ae",
        Aarch64Macos => "53a9074aaa83ab133797df6336b32d23b7de876d0483394a96a00b39bb536a1a",
        X8664Linux => "72c08b50155a0647643126a16a15672ce0856773ba19e60726abe8913d90be19",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    let name = "clippy";

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
