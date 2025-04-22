use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "nginx";

    let source_digest = "35764d85e27eac8e8c9eebfe6c5a593224ef63fdd1f2ea0916bf8b4d2335acc0";

    let source_version = "1.27.5";

    let source_path =
        format!("https://github.com/nginx/nginx/archive/refs/tags/release-{source_version}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = formatdoc! {"
        mkdir -pv \"$VORPAL_OUTPUT/bin\"

        pushd ./source/{name}/nginx-release-{source_version}

        ./auto/configure \
            --prefix=$VORPAL_OUTPUT \
            --without-http_rewrite_module

        make
        make install

        ln -svf $VORPAL_OUTPUT/sbin/nginx $VORPAL_OUTPUT/bin/nginx",
    };

    let step = step::shell(context, vec![], vec![], step_script).await?;

    ArtifactBuilder::new(name)
        .with_source(source)
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
        .await
}
