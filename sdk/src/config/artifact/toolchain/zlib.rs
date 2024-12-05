use crate::{
    artifact::{build_artifact, step_env_artifact},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

pub fn artifact(context: &mut ContextConfig) -> Result<ArtifactId> {
    let name = "zlib";

    let systems = vec![
        Aarch64Linux.into(),
        Aarch64Macos.into(),
        X8664Linux.into(),
        X8664Macos.into(),
    ];

    let source = build_artifact(
        context,
        vec![],
        vec![],
        format!("{}-source", name).as_str(),
        formatdoc! {"
            curl -L -o ./zlib-{version}.tar.gz \
                https://zlib.net/fossils/zlib-{version}.tar.gz
            tar -xvf ./zlib-{version}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
            version = "1.3.1",
        },
        vec![],
        systems.clone(),
    )?;

    build_artifact(
        context,
        vec![source.clone()],
        vec![],
        name,
        formatdoc! {"
            mkdir -p ./zlib

            cp -prv {zlib}/. .

            ./configure \
                --prefix=\"$VORPAL_OUTPUT/usr\"

            make
            make check
            make install

            rm -fv $VORPAL_OUTPUT/usr/lib/libz.a",
            zlib = step_env_artifact(&source),
        },
        vec![],
        systems,
    )
}
