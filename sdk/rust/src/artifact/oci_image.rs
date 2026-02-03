use crate::{
    api::artifact::ArtifactSystem::{Aarch64Linux, X8664Linux},
    artifact::{crane::Crane, get_env_key, rsync::Rsync, step, Artifact},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

pub struct OciImage<'a> {
    aliases: Vec<&'a str>,
    artifacts: Vec<&'a str>,
    crane: Option<&'a str>,
    name: &'a str,
    rootfs: &'a str,
    rsync: Option<&'a str>,
}

impl<'a> OciImage<'a> {
    pub fn new(name: &'a str, rootfs: &'a str) -> Self {
        Self {
            aliases: vec![],
            artifacts: vec![],
            crane: None,
            name,
            rootfs,
            rsync: None,
        }
    }

    pub fn with_aliases(mut self, aliases: Vec<&'a str>) -> Self {
        self.aliases = aliases;
        self
    }

    pub fn with_artifacts(mut self, artifacts: Vec<&'a str>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_crane(mut self, crane: &'a str) -> Self {
        self.crane = Some(crane);
        self
    }

    pub fn with_rsync(mut self, rsync: &'a str) -> Self {
        self.rsync = Some(rsync);
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        if self.name != self.name.to_lowercase() {
            anyhow::bail!("container image name must be lowercase: '{}'", self.name);
        }

        for c in self.name.chars() {
            if !matches!(c, 'a'..='z' | '0'..='9' | '/' | ':' | '-' | '.' | '_') {
                anyhow::bail!(
                    "container image name invalid character '{}': '{}'. \
                     Allowed: lowercase letters, digits, and / : - . _",
                    c,
                    self.name
                );
            }
        }

        let crane = match self.crane {
            Some(val) => val.to_string(),
            None => Crane::new().build(context).await?,
        };

        let rsync = match self.rsync {
            Some(val) => val.to_string(),
            None => Rsync::new().build(context).await?,
        };

        let rootfs = self.rootfs.to_string();

        let artifacts_list = self.artifacts.join(" ");

        let step_script = formatdoc! {"
            OCI_IMAGE_ARTIFACTS=\"{artifacts_list}\"
            OCI_IMAGE_CRANE=\"{crane}\"
            OCI_IMAGE_NAME=\"{name}\"
            OCI_IMAGE_ROOTFS=\"{rootfs}\"
            OCI_IMAGE_RSYNC=\"{rsync}\"
            OUTPUT_TAR=${{PWD}}/rootfs.tar
            ROOTFS_DIR=${{PWD}}/rootfs
            STORE_PREFIX=var/lib/vorpal/store/artifact/output/{namespace}

            # Detect platform based on build architecture
            case \"$(uname -m)\" in
                x86_64)  OCI_PLATFORM=\"linux/amd64\" ;;
                aarch64) OCI_PLATFORM=\"linux/arm64\" ;;
                *)       OCI_PLATFORM=\"linux/$(uname -m)\" ;;
            esac

            mkdir -pv ${{ROOTFS_DIR}}

            for artifact in ${{OCI_IMAGE_ARTIFACTS}}; do
                SOURCE_DIR=/${{STORE_PREFIX}}/${{artifact}}
                TARGET_PATH=${{STORE_PREFIX}}/${{artifact}}

                mkdir -p ${{ROOTFS_DIR}}/${{TARGET_PATH}}

                echo \"Copying artifact layer ${{artifact}}...\"

                ${{OCI_IMAGE_RSYNC}}/bin/rsync -aW ${{SOURCE_DIR}}/ ${{ROOTFS_DIR}}/${{TARGET_PATH}}

                echo \"Copied artifact layer ${{artifact}}\"

                # Symlink bin files to /usr/local/bin
                if [ -d \"${{SOURCE_DIR}}/bin\" ]; then
                    mkdir -p ${{ROOTFS_DIR}}/usr/local/bin
                    for bin_file in ${{SOURCE_DIR}}/bin/*; do
                        if [ -f \"${{bin_file}}\" ]; then
                            bin_name=$(basename \"${{bin_file}}\")
                            ln -sf /${{TARGET_PATH}}/bin/${{bin_name}} ${{ROOTFS_DIR}}/usr/local/bin/${{bin_name}}
                            echo \"Symlinked ${{bin_name}} to /usr/local/bin\"
                        fi
                    done
                fi
            done

            echo \"Copying Vorpal operating system files...\"

            ${{OCI_IMAGE_RSYNC}}/bin/rsync -aW ${{OCI_IMAGE_ROOTFS}}/ ${{ROOTFS_DIR}}

            echo \"Copied Vorpal operating system files\"

            echo \"Creating output tarball...\"

            tar -cf ${{OUTPUT_TAR}} -C ${{ROOTFS_DIR}} .

            echo \"Created output tarball\"

            mkdir -p ${{VORPAL_OUTPUT}}

            echo \"Creating OCI image ${{OCI_IMAGE_NAME}}:latest\"

            ${{OCI_IMAGE_CRANE}}/bin/crane append \\
                --new_layer ${{OUTPUT_TAR}} \\
                --new_tag ${{OCI_IMAGE_NAME}}:latest \\
                --oci-empty-base \\
                --output ${{VORPAL_OUTPUT}}/image.tar \\
                --platform ${{OCI_PLATFORM}}

            echo \"Setting platform metadata in image config...\"

            ${{OCI_IMAGE_CRANE}}/bin/crane mutate \\
                --set-platform ${{OCI_PLATFORM}} \\
                --output ${{VORPAL_OUTPUT}}/image.tar \\
                --tag ${{OCI_IMAGE_NAME}}:latest \\
                ${{VORPAL_OUTPUT}}/image.tar",
            artifacts_list = artifacts_list,
            crane = get_env_key(&crane),
            name = self.name,
            namespace = context.get_artifact_namespace(),
            rootfs = get_env_key(&rootfs),
            rsync = get_env_key(&rsync),
        };

        let mut step_artifacts = vec![crane, rsync, rootfs];

        for artifact in &self.artifacts {
            step_artifacts.push(artifact.to_string());
        }

        let step = step::shell(context, step_artifacts, vec![], step_script, vec![]).await?;

        let systems = vec![Aarch64Linux, X8664Linux];

        Artifact::new(self.name, vec![step], systems)
            .with_aliases(self.aliases.into_iter().map(String::from).collect())
            .build(context)
            .await
    }
}
