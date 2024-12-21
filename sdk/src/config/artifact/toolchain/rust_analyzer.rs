use crate::config::{ArtifactSource, artifact::{add_artifact, language::rust::get_toolchain_target, ConfigContext}};
use std::collections::BTreeMap;
use anyhow::{bail, Result};
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let hash = match context.get_target() {
        Aarch64Linux => "79fbf7077b846a4b28935fa6a22259d589baed2197c08bfc5c362f1e3f54db44",
        Aarch64Macos => "ba92aa08cdada8fad8d772623b0522cb3d6e659a8edb9e037453fab998772a19",
        X8664Linux => "686ed59c5df4cf2785f8e8c50665ac1949e4084283854a00cef47ca633501351",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    let name = "rust-analyzer";

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
