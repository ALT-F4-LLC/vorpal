use crate::{
    artifact::{build_artifact, rust_std},
    ContextConfig,
};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

// let hash = match context.get_target() {
//     Aarch64Linux => "bc6c0e0f309805c4a9b704bbfe6be6b3c28b029ac6958c58ab5b90437a9e36ed",
//     Aarch64Macos => "1512db881f5bdd7f4bbcfede7f5217bd51ca03dc6741c3577b4d071863690211",
//     X8664Linux => "2e94d4f43ca8390d1558b8c58a096e654cdb98ada396b539c3e518e95ce04746",
//     X8664Macos => "",
//     UnknownSystem => bail!("Unsupported rustc system: {:?}", context.get_target()),
// };

pub fn artifact(context: &mut ContextConfig) -> Result<ArtifactId> {
    let rust_std = rust_std::artifact(context)?;

    let name = "rustc";

    build_artifact(
        context,
        vec![rust_std],
        vec![],
        name.to_string(),
        formatdoc! {"
            curl -L -o ./rustc-{version}-{target}.tar.gz \
                https://static.rust-lang.org/dist/2024-05-02/rustc-{version}-{target}.tar.gz

            tar -xvf ./rustc-{version}-{target}.tar.gz -C $output --strip-components=1

            cp -pr ./rustc/rustc/* \"$output/.\"

            cat \"$rust_std/manifest.in\" >> \"$output/manifest.in\"

            cp -pr \"$rust_std/lib\" \"$output\"",
            target = match context.get_target() {
                Aarch64Linux => "aarch64-unknown-linux-gnu",
                Aarch64Macos => "aarch64-apple-darwin",
                X8664Linux => "x86_64-unknown-linux-gnu",
                X8664Macos => "x86_64-apple-darwin",
                UnknownSystem => bail!("Unsupported rustc target: {:?}", context.get_target()),
            },
            version = "1.78.0",
        },
        vec![],
        vec![
            Aarch64Linux.into(),
            Aarch64Macos.into(),
            X8664Linux.into(),
            X8664Macos.into(),
        ],
    )
}
