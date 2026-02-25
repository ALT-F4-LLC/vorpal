use crate::{
    api,
    api::artifact::ArtifactSystem,
    artifact::{bun::Bun, get_env_key, step, Artifact, ArtifactSource, DevelopmentEnvironment},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::BTreeMap;

pub struct TypeScript<'a> {
    aliases: Vec<String>,
    artifacts: Vec<String>,
    entrypoint: Option<&'a str>,
    environments: Vec<&'a str>,
    name: &'a str,
    node_modules: BTreeMap<String, String>,
    secrets: Vec<api::artifact::ArtifactStepSecret>,
    source_includes: Vec<&'a str>,
    source_scripts: Vec<String>,
    systems: Vec<ArtifactSystem>,
    vorpal_sdk: bool,
    working_dir: Option<String>,
}

impl<'a> TypeScript<'a> {
    pub fn new(name: &'a str, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            aliases: vec![],
            artifacts: vec![],
            entrypoint: None,
            environments: vec![],
            name,
            node_modules: BTreeMap::new(),
            secrets: vec![],
            source_includes: vec![],
            source_scripts: vec![],
            systems,
            vorpal_sdk: true,
            working_dir: None,
        }
    }

    pub fn with_aliases(mut self, aliases: Vec<String>) -> Self {
        for alias in aliases {
            if !self.aliases.contains(&alias) {
                self.aliases.push(alias);
            }
        }
        self
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_entrypoint(mut self, entrypoint: &'a str) -> Self {
        self.entrypoint = Some(entrypoint);
        self
    }

    pub fn with_environments(mut self, environments: Vec<&'a str>) -> Self {
        self.environments = environments;
        self
    }

    pub fn with_includes(mut self, includes: Vec<&'a str>) -> Self {
        self.source_includes = includes;
        self
    }

    pub fn with_node_modules(mut self, modules: Vec<(&str, String)>) -> Self {
        for (name, digest) in modules {
            self.node_modules.insert(name.to_string(), digest);
        }
        self
    }

    pub fn with_secrets(mut self, secrets: Vec<(String, String)>) -> Self {
        for (name, value) in secrets {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets
                    .push(api::artifact::ArtifactStepSecret { name, value });
            }
        }

        self
    }

    pub fn with_source_scripts(mut self, scripts: Vec<String>) -> Self {
        for script in scripts {
            if !self.source_scripts.contains(&script) {
                self.source_scripts.push(script);
            }
        }
        self
    }

    pub fn with_vorpal_sdk(mut self, include: bool) -> Self {
        self.vorpal_sdk = include;
        self
    }

    pub fn with_working_dir(mut self, dir: &str) -> Self {
        self.working_dir = Some(dir.to_string());
        self
    }

    pub async fn build(mut self, context: &mut ConfigContext) -> Result<String> {
        // Setup artifacts

        let bun = Bun::new().build(context).await?;
        let bun_bin = format!("{}/bin", get_env_key(&bun));

        // Setup source

        let source_path = ".";

        let mut source_builder = ArtifactSource::new(self.name, source_path);

        if !self.source_includes.is_empty() {
            source_builder = source_builder
                .with_includes(self.source_includes.iter().map(|s| s.to_string()).collect());
        }

        let source = source_builder.build();

        // Setup step source directory

        let step_source_dir = format!("{}/source/{}", source_path, source.name);

        let step_source_dir = match &self.working_dir {
            Some(working_dir) => format!("{}/{}", step_source_dir, working_dir),
            None => step_source_dir.clone(),
        };

        // Setup node modules in script

        let mut step_package_json_js_parts = Vec::new();

        step_package_json_js_parts.push("const fs=require('fs')".to_string());
        step_package_json_js_parts
            .push("const p=JSON.parse(fs.readFileSync('package.json','utf8'))".to_string());

        for (package_name, digest) in &self.node_modules {
            let env_key = get_env_key(digest);

            step_package_json_js_parts.push(format!(
                    "if(p.dependencies?.['{package_name}'])p.dependencies['{package_name}']='file:{env_key}'"
                ));

            step_package_json_js_parts.push(format!(
                    "if(p.devDependencies?.['{package_name}'])p.devDependencies['{package_name}']='file:{env_key}'"
                ));
        }

        step_package_json_js_parts
            .push("fs.writeFileSync('package.json',JSON.stringify(p,null,2))".to_string());

        let step_package_json_js = step_package_json_js_parts.join(";") + ";";
        let step_package_json_script = format!("{bun_bin}/bun -e \"{step_package_json_js}\"\n");

        // Update bun.lock file if it exists

        let mut step_bun_lock_js_parts = Vec::new();

        step_bun_lock_js_parts.push("const fs=require('fs')".to_string());
        step_bun_lock_js_parts.push(
            "if(fs.existsSync('bun.lock')){var t=fs.readFileSync('bun.lock','utf8');var q=String.fromCharCode(34)".to_string(),
        );

        for (package_name, digest) in &self.node_modules {
            let env_key = get_env_key(digest);

            // Replace workspace dependency value: "package": "file:/old" -> "package": "file:<env_key>"
            step_bun_lock_js_parts.push(format!(
                "var p1=q+'{package_name}'+q+': '+q+'file:';var i=t.indexOf(p1);while(i>=0){{var s=i+p1.length;var e=t.indexOf(q,s);t=t.substring(0,s)+'{env_key}'+t.substring(e);i=t.indexOf(p1,s)}}"
            ));

            // Replace packages resolved specifier: "package@file:/old" -> "package@file:<env_key>"
            step_bun_lock_js_parts.push(format!(
                "var p2=q+'{package_name}@file:';var i=t.indexOf(p2);while(i>=0){{var s=i+p2.length;var e=t.indexOf(q,s);t=t.substring(0,s)+'{env_key}'+t.substring(e);i=t.indexOf(p2,s)}}"
            ));
        }

        step_bun_lock_js_parts.push("fs.writeFileSync('bun.lock',t)}".to_string());

        let step_bun_lock_js = step_bun_lock_js_parts.join(";") + ";";
        let step_bun_lock_script = format!("{bun_bin}/bun -e \"{step_bun_lock_js}\"\n");

        // Setup build command

        let step_build_command = match self.entrypoint {
            Some(entrypoint) => formatdoc! {r#"
                mkdir -p $VORPAL_OUTPUT/bin

                {bun_bin}/bun build --compile {entrypoint} --outfile {name}

                cp {name} $VORPAL_OUTPUT/bin/{name}"#,
                name = self.name,
            },
            None => formatdoc! {r#"
                mkdir -p $VORPAL_OUTPUT

                {bun_bin}/bun x tsc --project tsconfig.json --outDir dist

                cp package.json $VORPAL_OUTPUT/
                cp -r dist $VORPAL_OUTPUT/
                cp -r node_modules $VORPAL_OUTPUT/"#,
            },
        };

        // Build step script

        let step_script = formatdoc! {r#"
            pushd {step_source_dir}

            {step_source_scripts}
            {step_package_json_script}
            {step_bun_lock_script}

            {bun_bin}/bun install --frozen-lockfile

            {step_build_command}"#,
            step_source_scripts = self.source_scripts.join("\n")
        };

        let mut step_environments = vec![format!("PATH={bun_bin}")];

        for env in self.environments {
            step_environments.push(env.to_string());
        }

        let mut step_artifacts = vec![bun.clone()];

        step_artifacts.extend(self.artifacts);

        for digest in self.node_modules.values() {
            step_artifacts.push(digest.clone());
        }

        // Sort for deterministic output

        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

        let steps = vec![
            step::shell(
                context,
                step_artifacts,
                step_environments,
                step_script,
                self.secrets,
            )
            .await?,
        ];

        Artifact::new(self.name, steps, self.systems)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}

pub struct TypeScriptDevelopmentEnvironment<'a> {
    artifacts: Vec<String>,
    environments: Vec<String>,
    name: &'a str,
    node_modules: BTreeMap<String, String>,
    secrets: Vec<(&'a str, &'a str)>,
    systems: Vec<ArtifactSystem>,
}

impl<'a> TypeScriptDevelopmentEnvironment<'a> {
    pub fn new(name: &'a str, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            artifacts: vec![],
            environments: vec![],
            name,
            node_modules: BTreeMap::new(),
            secrets: vec![],
            systems,
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts.extend(artifacts);
        self
    }

    pub fn with_environments(mut self, environments: Vec<String>) -> Self {
        self.environments.extend(environments);
        self
    }

    pub fn with_node_module(mut self, package_name: &str, digest: String) -> Self {
        self.node_modules.insert(package_name.to_string(), digest);
        self
    }

    pub fn with_secrets(mut self, secrets: Vec<(&'a str, &'a str)>) -> Self {
        for secret in secrets {
            if !self.secrets.iter().any(|(name, _)| *name == secret.0) {
                self.secrets.push(secret);
            }
        }
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let bun = Bun::new().build(context).await?;

        let mut artifacts = vec![bun];
        artifacts.extend(self.artifacts);

        // Add node module digests to artifacts (BTreeMap iterates in sorted key order)
        for digest in self.node_modules.values() {
            artifacts.push(digest.clone());
        }

        let mut environments = self.environments;

        // Construct NODE_PATH from node module artifacts
        if !self.node_modules.is_empty() {
            let node_path = self
                .node_modules
                .values()
                .map(|digest| format!("{}/..", get_env_key(digest)))
                .collect::<Vec<String>>()
                .join(":");

            environments.push(format!("NODE_PATH={node_path}"));
        }

        let mut devenv = DevelopmentEnvironment::new(self.name, self.systems)
            .with_artifacts(artifacts)
            .with_environments(environments);

        if !self.secrets.is_empty() {
            devenv = devenv.with_secrets(self.secrets);
        }

        devenv.build(context).await
    }
}
