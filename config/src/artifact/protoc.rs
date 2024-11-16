use crate::{artifact::build_artifact, ContextConfig};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

// let hash = match context.get_target() {
//     Aarch64Linux => "8a592a0dd590e92b1c0d77631e683fc743d1ed8158e0b093b6cfabf0685089af",
//     Aarch64Macos => "d105abb1c1d2c024f29df884f0592f1307984d63aeb10f0e61ccb94aee2c2feb",
//     X8664Linux => "d5e8fb327ea9568fd1ce2de3557740948a2168faff79c0e02e64bd9f040964d9",
//     X8664Macos => "",
//     UnknownSystem => bail!("Unsupported system: {:?}", context.get_target()),
// };

pub fn artifact(context: &mut ContextConfig) -> Result<ArtifactId> {
    let name = "protoc";

    build_artifact(
        context,
        vec![],
        vec![],
        name.to_string(),
        formatdoc! {"
            curl -L -o ./protoc-{version}-{target}.zip \
                https://github.com/protocolbuffers/protobuf/releases/download/v{version}/protoc-{version}-{target}.zip

            cp -r ./{name}/bin \"$output/bin\"

            cp -r ./{name}/include \"$output/include\"

            chmod +x \"$output/bin/protoc\"",
            target = match context.get_target() {
                Aarch64Linux => "linux-aarch_64",
                Aarch64Macos => "osx-aarch_64",
                X8664Linux => "linux-x86_64",
                X8664Macos => "osx-x86_64",
                UnknownSystem => bail!("Unsupported system: {:?}", context.get_target()),
            },
            name = name,
            version = "25.4",
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
