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
        Aarch64Linux => "42781c7ae909a5cd01c955cb4343754ce33d75783b2599a3f1a3b3752a0947af",
        Aarch64Macos => "e88e4babfc20e0546fe28bc2ba3f71a467f83e9fb1be76c9a078d327379ee4d0",
        X8664Linux => "62091f43974e3e24583cceae24db710e9bd6863f366b9a5891bd7a5aa3d8c0fd",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    let name = "cargo";

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
