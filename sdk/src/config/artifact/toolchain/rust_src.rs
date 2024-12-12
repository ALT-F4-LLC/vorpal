use crate::config::{artifact::add_artifact, ConfigContext};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;

pub async fn artifact(
    context: &mut ConfigContext,
    override_version: Option<String>,
) -> Result<ArtifactId> {
    let name = "rust-src";

    let mut version = "1.80.1".to_string();

    if let Some(v) = override_version {
        version = v;
    }

    add_artifact(
        context,
        vec![],
        vec![],
        name,
        formatdoc! {"
            curl -L -o ./{name}-{version}.tar.gz \
                https://static.rust-lang.org/dist/{name}-{version}.tar.gz

            tar -xvf ./{name}-{version}.tar.gz -C source --strip-components=1

            cp -prv \"./source/{name}/.\" \"$VORPAL_OUTPUT\"
        "},
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
