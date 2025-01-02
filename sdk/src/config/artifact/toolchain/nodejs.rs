use std::collections::BTreeMap;

use crate::config::{artifact::add_artifact, ArtifactSource, ConfigContext};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    // https://nodejs.org/dist/v<version>/SHASUMS256.txt
    let hash = match context.get_target() {
        Aarch64Linux => "8cfd5a8b9afae5a2e0bd86b0148ca31d2589c0ea669c2d0b11c132e35d90ed68",
        Aarch64Macos => "a9907c2b99eecf30a7b2e51f652ae53048aed258cec51e34a7b8c4aee5d65fbf",
        X8664Linux => "22982235e1b71fa8850f82edd09cdae7e3f32df1764a9ec298c72d25ef2c164f",
        X8664Macos => "d68ef0c4c19b3b3b88c0e7408668d0a539607c136a14668e079feed0c6ec8bec",
        UnknownSystem => bail!("Invalid nodejs system: {:?}", context.get_target()),
    };

    let platform = match context.get_target() {
        Aarch64Linux => "linux-arm64",
        Aarch64Macos => "darwin-arm64",
        X8664Linux => "linux-x64",
        X8664Macos => "darwin-x64",
        UnknownSystem => bail!("Invalid nodejs system: {:?}", context.get_target()),
    };

    let name = "nodejs";

    add_artifact(
        context,
        vec![],
        BTreeMap::new(),
        name,
        formatdoc! {"
            export NODE_SRC=\"./source/nodejs/node-v{version}-{platform}\"
            mv \"$NODE_SRC/bin/npm\" \"$NODE_SRC/bin/.npm-unwrapped\"
            cat << EOJ > \"$NODE_SRC/bin/npm\"
#!/usr/bin/env sh
export PATH=\"$VORPAL_OUTPUT/bin:$PATH\"
exec \"$VORPAL_OUTPUT/bin/.npm-unwrapped\" \"$@\"
EOJ
            chmod +x \"$NODE_SRC/bin/npm\"
            cp -prv \"$NODE_SRC/.\" \"$VORPAL_OUTPUT\"
        "},
        BTreeMap::from([(
            name,
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!(
                    "https://nodejs.org/dist/v{version}/node-v{version}-{platform}.tar.xz"
                ),
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
