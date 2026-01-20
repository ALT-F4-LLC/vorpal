use crate::artifact::SYSTEMS;
use anyhow::Result;
use indoc::formatdoc;
use vorpal_sdk::{
    artifact::{get_env_key, gh::Gh, Argument, Job},
    context::ConfigContext,
};

#[derive(Default)]
pub struct VorpalRelease {}

impl VorpalRelease {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        // Setup arguments

        let branch_name = Argument::new("branch-name").with_require().build(context)?;

        let darwin_aarch64 = Argument::new("aarch64-darwin")
            .with_require()
            .build(context)?;

        let darwin_x8664 = Argument::new("x8664-darwin")
            .with_require()
            .build(context)?;

        let linux_aarch64 = Argument::new("aarch64-linux")
            .with_require()
            .build(context)?;

        let linux_x8664 = Argument::new("x8664-linux").with_require().build(context)?;

        // Fetch artifacts

        let aarch64_darwin = context.fetch_artifact(&darwin_aarch64.unwrap()).await?;
        let aarch64_linux = context.fetch_artifact(&linux_aarch64.unwrap()).await?;
        let x8664_darwin = context.fetch_artifact(&darwin_x8664.unwrap()).await?;
        let x8664_linux = context.fetch_artifact(&linux_x8664.unwrap()).await?;

        let script = formatdoc! {r#"
            git clone \
                --branch {branch_name} \
                --depth 1 \
                    git@github.com:ALT-F4-LLC/vorpal.git

                pushd vorpal

                git fetch --tags
                git tag --delete nightly || true
                git push origin :refs/tags/nightly || true
                gh release delete --yes nightly || true

                git tag nightly
                git push --tags

                gh release create \
                --notes "Nightly builds from main branch." \
                --prerelease \
                --title "nightly" \
                --verify-tag \
                nightly \
                {aarch64_darwin}.tar.zst \
                {aarch64_linux}.tar.zst \
                {x8664_darwin}.tar.zst \
                {x8664_linux}.tar.zst"#,
            aarch64_darwin = get_env_key(&aarch64_darwin),
            aarch64_linux = get_env_key(&aarch64_linux),
            branch_name = branch_name.unwrap(),
            x8664_darwin = get_env_key(&x8664_darwin),
            x8664_linux = get_env_key(&x8664_linux),
        };

        let gh = Gh::new().build(context).await?;

        Job::new("vorpal-release", script, SYSTEMS.to_vec())
            .with_artifacts(vec![
                aarch64_darwin,
                aarch64_linux,
                gh,
                x8664_darwin,
                x8664_linux,
            ])
            .build(context)
            .await
    }
}
