use crate::config::{
    artifact::{add_artifact, language::rust::get_rust_toolchain_target, ConfigContext},
    ArtifactSource,
};
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let hash = match context.get_target() {
        Aarch64Linux => "f5e5eac428b2a62ffc14324e3a6e171fb3032921f24973b27959834e456388b1",
        Aarch64Macos => "d022dd6d61a7039c12834f90a0a5410c884bfb9ef1e38b085ad4d3f59a5bf04a",
        X8664Linux => "fb18b7bb9dd94a5eeb445af1e4dd636836b6034f5dc731d534548bf5f9cb3d6f",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    let name = "rustc";

    let target = get_rust_toolchain_target(context.get_target())?;

    add_artifact(
        context,
        vec![],
        BTreeMap::new(),
        name,
        format!(
            "cp -prv \"./source/{name}/{name}-{version}-{target}/{name}/.\" \"$VORPAL_OUTPUT\""
        ),
        BTreeMap::from([(
            name,
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!("https://static.rust-lang.org/dist/{name}-{version}-{target}.tar.gz"),
            },
        )]),
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ],
    )
    .await
}
