use crate::{
    artifact::{
        get_env_key, linux_vorpal::LinuxVorpal, rsync::Rsync, step, system, Artifact,
        ArtifactSource,
    },
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

/// Builds a slimmed-down `linux_vorpal` artifact by rsyncing the full
/// `linux_vorpal` rootfs and stripping it via `script/linux-vorpal-slim.sh`.
#[derive(Default)]
pub struct LinuxVorpalSlim<'a> {
    linux_vorpal: Option<&'a str>,
    rsync: Option<&'a str>,
}

impl<'a> LinuxVorpalSlim<'a> {
    /// Creates a builder for the `linux-vorpal-slim` artifact.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reuses an existing `linux_vorpal` artifact digest instead of building one.
    #[must_use]
    pub fn with_linux_vorpal(mut self, linux_vorpal: &'a str) -> Self {
        self.linux_vorpal = Some(linux_vorpal);
        self
    }

    /// Reuses an existing `rsync` artifact digest instead of building one.
    #[must_use]
    pub fn with_rsync(mut self, rsync: &'a str) -> Self {
        self.rsync = Some(rsync);
        self
    }

    /// Builds the `linux-vorpal-slim` artifact.
    ///
    /// # Errors
    ///
    /// Returns an error if building the `linux_vorpal` or `rsync` dependency fails
    /// (when not supplied via `with_linux_vorpal`/`with_rsync`), or if the rsync/strip
    /// shell step fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let linux_vorpal = match self.linux_vorpal {
            Some(val) => val,
            None => &LinuxVorpal::new().build(context).await?,
        };

        let rsync = match self.rsync {
            Some(val) => val,
            None => &Rsync::new().build(context).await?,
        };

        let name = "linux-vorpal-slim";

        let version = "latest";

        let source = ArtifactSource::new(name, ".")
            .with_includes(vec!["script/linux-vorpal-slim.sh".to_string()])
            .build();

        let step_script = formatdoc! {"
            mkdir -p ./source/linux-vorpal

            {rsync}/bin/rsync -aPW {linux_vorpal}/ $VORPAL_OUTPUT

            pushd ./source

            ./{name}/script/linux-vorpal-slim.sh --execute --no-confirm $VORPAL_OUTPUT",
            linux_vorpal = get_env_key(linux_vorpal),
            rsync = get_env_key(rsync),
        };

        let artifacts = vec![linux_vorpal.to_string(), rsync.to_string()];

        let steps = vec![step::shell(context, &artifacts, &[], step_script, &[]).await?];

        Artifact::new(name, steps, system::SYSTEMS)
            .with_aliases(vec![format!("{name}:{version}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
