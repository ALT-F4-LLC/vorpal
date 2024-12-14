use crate::config::artifact::{add_artifact, language::rust::get_toolchain_target, ConfigContext};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let name = "rust-analyzer";

    let target = get_toolchain_target(context.get_target())?;

    add_artifact(
        context,
        vec![],
        vec![],
        name,
        formatdoc! {"
            curl -L -o ./{name}-{version}-{target}.tar.gz \
                https://static.rust-lang.org/dist/{name}-{version}-{target}.tar.gz

            mkdir -pv source

            tar -xvf ./{name}-{version}-{target}.tar.gz -C source --strip-components=1

            cp -prv source/{name}-preview/. \"$VORPAL_OUTPUT\"",
        },
        vec![],
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ],
    )
    .await
}
