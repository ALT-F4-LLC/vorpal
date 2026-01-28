use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

#[derive(Default)]
pub struct Rsync;

impl Rsync {
    pub fn new() -> Self {
        Self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "rsync";
        let version = "3.4.1";

        let path = format!("https://download.samba.org/pub/rsync/src/rsync-{version}.tar.gz");
        let source = ArtifactSource::new(name, &path).build();

        let step_script = formatdoc! {"
            mkdir -pv \"$VORPAL_OUTPUT\"
            pushd ./source/{name}/{name}-{version}
            ./configure \
                --prefix=\"$VORPAL_OUTPUT\" \
                --disable-openssl \
                --disable-xxhash \
                --disable-zstd \
                --disable-lz4
            make
            make install",
        };

        let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Artifact::new(name, steps, systems)
            .with_aliases(vec![format!("{name}:{version}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
