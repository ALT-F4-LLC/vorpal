use crate::config::artifact::{add_artifact, language::rust::get_toolchain_target, ConfigContext};
use anyhow::{bail, Result};
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSource,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let hash = match context.get_target() {
        Aarch64Linux => "2104734509330997f1c21fb8e1f8430e24f38f1a856e116c3f4fc7b54c6201fb",
        Aarch64Macos => "d29a89d06ac9acdae514dddf8b9168a8fd4d2be3434b8f20e9fe22091e86e30f",
        X8664Linux => "686ed59c5df4cf2785f8e8c50665ac1949e4084283854a00cef47ca633501351",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    let name = "rust-analyzer";

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
