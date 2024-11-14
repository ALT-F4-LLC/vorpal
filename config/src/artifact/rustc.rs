use crate::{
    artifact::{build_artifact, rust_std},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

pub fn artifact(context: &mut ContextConfig) -> Result<ArtifactId> {
    let rust_std = rust_std::artifact(context)?;

    let name = "rustc";

    // let hash = match context.get_target() {
    //     Aarch64Linux => "bc6c0e0f309805c4a9b704bbfe6be6b3c28b029ac6958c58ab5b90437a9e36ed",
    //     Aarch64Macos => "1512db881f5bdd7f4bbcfede7f5217bd51ca03dc6741c3577b4d071863690211",
    //     X8664Linux => "2e94d4f43ca8390d1558b8c58a096e654cdb98ada396b539c3e518e95ce04746",
    //     X8664Macos => "",
    //     UnknownSystem => bail!("Unsupported rustc system: {:?}", context.get_target()),
    // };

    // let target = match context.get_target() {
    //     Aarch64Linux => "aarch64-unknown-linux-gnu",
    //     Aarch64Macos => "aarch64-apple-darwin",
    //     X8664Linux => "x86_64-unknown-linux-gnu",
    //     X8664Macos => "x86_64-apple-darwin",
    //     UnknownSystem => bail!("Unsupported rustc target: {:?}", context.get_target()),
    // };

    // let version = "1.78.0";

    let artifact = Artifact {
        environments: vec![],
        name: name.to_string(),
        artifacts: vec![rust_std],
        sandbox: None,
        script: formatdoc! {"
            cp -pr ./rustc/rustc/* \"$output/.\"
            cat \"$rust_std/manifest.in\" >> \"$output/manifest.in\"
            cp -pr \"$rust_std/lib\" \"$output\""
        },
        sources: vec![],
        // source: vec![ArtifactSource {
        //     excludes: vec![],
        //     hash: Some(hash.to_string()),
        //     includes: vec![],
        //     name: name.to_string(),
        //     strip_prefix: true,
        //     uri: format!(
        //         "https://static.rust-lang.org/dist/2024-05-02/rustc-{}-{}.tar.gz",
        //         version, target
        //     ),
        // }],
        systems: vec![
            Aarch64Linux.into(),
            Aarch64Macos.into(),
            X8664Linux.into(),
            X8664Macos.into(),
        ],
    };

    build_artifact(context, artifact)
}
