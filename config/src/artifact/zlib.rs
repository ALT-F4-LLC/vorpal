use crate::{artifact::build_artifact, ContextConfig};
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
        format!("{}-source", name),
        formatdoc! {"
            curl -L -o ./zlib-{version}.tar.gz \
                https://zlib.net/fossils/zlib-{version}.tar.gz

            tar -xvf ./zlib-{version}.tar.gz -C $output --strip-components=1",
            version = "1.3.1",
        },
        vec![],
        systems.clone(),
    )?;

    build_artifact(
        context,
        vec![source],
        vec![],
        name.to_string(),
        formatdoc! {"
            mkdir -p ./zlib

            rsync -av --progress $zlib_source/ .

            ./configure \
                --prefix=\"$output/usr\"

            make
            make check
            make install

            rm -fv $output/usr/lib/libz.a"
        },
        vec![],
        systems,
    )
}
