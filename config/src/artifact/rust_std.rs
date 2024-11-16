use crate::{artifact::build_artifact, ContextConfig};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

// let source_hash = match context.get_target() {
//     Aarch64Linux => "72d4917bb58b693b3f2c589746ed470645f96895ece3dd27f7055d3c3f7f7a79",
//     Aarch64Macos => "0689a9b2dec87c272954db9212a8f3d5243f55f777f90d84d2b3aeb2aa938ba5",
//     X8664Linux => "ad734eb9699b0a9dffdd35034776ccaa4d7b45e1898fc32748be93b60453550d",
//     X8664Macos => "",
//     UnknownSystem => bail!("Unsupported system: {:?}", context.get_target()),
// };

pub fn artifact(context: &mut ContextConfig) -> Result<ArtifactId> {
    let name = "rust-std";

    build_artifact(
        context,
        vec![],
        vec![],
        name.to_string(),
        formatdoc! {"
            curl -L -o ./rust-std-{version}-{target}.tar.gz \
                https://static.rust-lang.org/dist/2024-05-02/{version}-{target}.tar.gz

            tar -xvf ./rust-std-{version}-{target}.tar.gz -C $output --strip-components=1",
            target = match context.get_target() {
                Aarch64Linux => "aarch64-unknown-linux-gnu",
                Aarch64Macos => "aarch64-apple-darwin",
                X8664Linux => "x86_64-unknown-linux-gnu",
                X8664Macos => "x86_64-apple-darwin",
                UnknownSystem => bail!("Unsupported system: {:?}", context.get_target()),
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
