use crate::config::{
    artifact::{add_artifact, language::rust::get_toolchain_target, ConfigContext},
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
        Aarch64Linux => "5e0b5cb7e8655501369a6f42cb10b1c5d4711a0edfcbe44483c5234da485819d",
        Aarch64Macos => "fe82bf19b064f6fca648b9be6a53ae210a9934023df364d669fc7c4ee5ccd485",
        X8664Linux => "84168586980d4dfa8f385c83d66af0dcc3256668f0a3109b57712340251660f1",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    let name = "clippy";

    let target = get_toolchain_target(context.get_target())?;

    add_artifact(
        context,
        vec![],
        BTreeMap::new(),
        name,
        format!("cp -prv \"./source/{name}/{name}-{version}-{target}/{name}-preview/.\" \"$VORPAL_OUTPUT\""),
        BTreeMap::from([(
            name,
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!("https://static.rust-lang.org/dist/{name}-{version}-{target}.tar.gz"),
            }
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
