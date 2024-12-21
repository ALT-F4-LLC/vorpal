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
        Aarch64Linux => "c51d5f913a093845f4c0ffa452e2dcc06543d41d94eab874ced496ba7d8227f2",
        Aarch64Macos => "3e0f5942dad4dc285d9aa54fee1e1b06a437459f1534112e62057c96e93e36ab",
        X8664Linux => "6298d92c2da6a89cb0ca83ddca9371dbcf8efb63caaaf6427e8588bfa086d6e0",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    let name = "cargo";

    let target = get_toolchain_target(context.get_target())?;

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
