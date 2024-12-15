use crate::config::artifact::{add_artifact, language::rust::get_toolchain_target, ConfigContext};
use anyhow::{bail, Result};
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSource,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let hash = match context.get_target() {
        Aarch64Linux => "dfb132cf163ca931909584dbdc6f5f2a7b6debadca59b83d179f60703773f72d",
        Aarch64Macos => "10f1f06f6e7aebb6759483a4a8b24c9e6463b13f7cac1fd15bd42b0cc786fe05",
        X8664Linux => "1234567890",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    let name = "rustc";

    let target = get_toolchain_target(context.get_target())?;

    add_artifact(
        context,
        vec![],
        vec![],
        name,
        format!(
            "cp -prv \"./source/{name}/{name}-{version}-{target}/{name}/.\" \"$VORPAL_OUTPUT\""
        ),
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
