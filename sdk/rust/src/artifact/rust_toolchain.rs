use crate::{
    api::artifact::{
        ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    artifact::{
        cargo::Cargo, clippy::Clippy, get_env_key, rust_analyzer::RustAnalyzer, rust_src::RustSrc,
        rust_std::RustStd, rustc::Rustc, rustfmt::Rustfmt, step, Artifact,
    },
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

pub fn target(system: ArtifactSystem) -> Result<String> {
    let target = match system {
        Aarch64Darwin => "aarch64-apple-darwin",
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        X8664Darwin => "x86_64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        _ => bail!("unsupported 'rust-toolchain' system: {:?}", system),
    };

    Ok(target.to_string())
}

pub fn version() -> String {
    "1.89.0".to_string()
}

#[derive(Default)]
pub struct RustToolchain<'a> {
    cargo: Option<&'a str>,
    clippy: Option<&'a str>,
    rust_analyzer: Option<&'a str>,
    rust_src: Option<&'a str>,
    rust_std: Option<&'a str>,
    rustc: Option<&'a str>,
    rustfmt: Option<&'a str>,
}

impl<'a> RustToolchain<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cargo(mut self, cargo: &'a str) -> Self {
        self.cargo = Some(cargo);
        self
    }

    pub fn with_clippy(mut self, clippy: &'a str) -> Self {
        self.clippy = Some(clippy);
        self
    }

    pub fn with_rust_analyzer(mut self, rust_analyzer: &'a str) -> Self {
        self.rust_analyzer = Some(rust_analyzer);
        self
    }

    pub fn with_rust_src(mut self, rust_src: &'a str) -> Self {
        self.rust_src = Some(rust_src);
        self
    }

    pub fn with_rust_std(mut self, rust_std: &'a str) -> Self {
        self.rust_std = Some(rust_std);
        self
    }

    pub fn with_rustc(mut self, rustc: &'a str) -> Self {
        self.rustc = Some(rustc);
        self
    }

    pub fn with_rustfmt(mut self, rustfmt: &'a str) -> Self {
        self.rustfmt = Some(rustfmt);
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let cargo = match self.cargo {
            Some(digest) => digest.to_string(),
            None => Cargo::new().build(context).await?,
        };

        let clippy = match self.clippy {
            Some(digest) => digest.to_string(),
            None => Clippy::new().build(context).await?,
        };

        let rust_analyzer = match self.rust_analyzer {
            Some(digest) => digest.to_string(),
            None => RustAnalyzer::new().build(context).await?,
        };

        let rust_src = match self.rust_src {
            Some(digest) => digest.to_string(),
            None => RustSrc::new().build(context).await?,
        };

        let rust_std = match self.rust_std {
            Some(digest) => digest.to_string(),
            None => RustStd::new().build(context).await?,
        };

        let rustc = match self.rustc {
            Some(digest) => digest.to_string(),
            None => Rustc::new().build(context).await?,
        };

        let rustfmt = match self.rustfmt {
            Some(digest) => digest.to_string(),
            None => Rustfmt::new().build(context).await?,
        };

        let artifacts = vec![
            cargo,
            clippy,
            rust_analyzer,
            rust_src,
            rust_std,
            rustc,
            rustfmt,
        ];

        let mut toolchain_component_paths = vec![];

        for component in &artifacts {
            toolchain_component_paths.push(get_env_key(component));
        }

        let toolchain_target = target(context.get_system())?;
        let toolchain_version = version();

        let step_script = formatdoc! {"
            toolchain_dir=\"$VORPAL_OUTPUT/toolchains/{toolchain_version}-{toolchain_target}\"

            mkdir -pv \"$toolchain_dir\"

            components=({component_paths})

            for component in \"${{components[@]}}\"; do
                find \"$component\" | while read -r file; do
                    relative_path=$(echo \"$file\" | sed -e \"s|$component||\")

                    echo \"Copying $file to $toolchain_dir$relative_path\"

                    if [[ \"$relative_path\" == \"/manifest.in\" ]]; then
                        continue
                    fi

                    if [ -d \"$file\" ]; then
                        mkdir -pv \"$toolchain_dir$relative_path\"
                    else
                        cp -pv \"$file\" \"$toolchain_dir$relative_path\"
                    fi
                done
            done

            cat > \"$VORPAL_OUTPUT/settings.toml\" << \"EOF\"
            auto_self_update = \"disable\"
            profile = \"minimal\"
            version = \"12\"

            [overrides]
            EOF",
            component_paths = toolchain_component_paths.join(" "),
        };

        let steps = vec![step::shell(context, artifacts, vec![], step_script, vec![]).await?];
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];
        let name = "rust-toolchain";

        Artifact::new(name, steps, systems)
            .with_aliases(vec![format!("{name}:{toolchain_version}")])
            .build(context)
            .await
    }
}
