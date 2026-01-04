use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tonic::Status;
use vorpal_sdk::api::artifact::{
    Artifact, ArtifactFunction, ArtifactFunctionParam, ArtifactSource, ArtifactStep,
    ArtifactStepSecret, ArtifactSystem,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ArtifactFunctionDefinition {
    pub meta: ArtifactFunction,
    pub artifact_template: Artifact,
}

pub fn resolve_function(
    definition: ArtifactFunctionDefinition,
    params: HashMap<String, String>,
    system: ArtifactSystem,
) -> Result<Artifact, Status> {
    let resolved_params = resolve_params(&definition.meta.params, params)?;
    let mut artifact = definition.artifact_template;

    if system != ArtifactSystem::UnknownSystem {
        artifact.target = system as i32;
    }

    apply_params_to_artifact(&mut artifact, &resolved_params);

    Ok(artifact)
}

fn resolve_params(
    params_spec: &[ArtifactFunctionParam],
    mut params: HashMap<String, String>,
) -> Result<HashMap<String, String>, Status> {
    for param in params_spec {
        if params.contains_key(&param.name) {
            continue;
        }

        if !param.default.is_empty() {
            params.insert(param.name.clone(), param.default.clone());
            continue;
        }

        if param.required {
            return Err(Status::invalid_argument(format!(
                "missing required param '{}'",
                param.name
            )));
        }
    }

    Ok(params)
}

fn apply_params_to_artifact(artifact: &mut Artifact, params: &HashMap<String, String>) {
    artifact.name = render_template(&artifact.name, params);

    for alias in artifact.aliases.iter_mut() {
        *alias = render_template(alias, params);
    }

    for source in artifact.sources.iter_mut() {
        apply_params_to_source(source, params);
    }

    for step in artifact.steps.iter_mut() {
        apply_params_to_step(step, params);
    }
}

fn apply_params_to_source(source: &mut ArtifactSource, params: &HashMap<String, String>) {
    if let Some(digest) = source.digest.as_mut() {
        *digest = render_template(digest, params);
    }

    for exclude in source.excludes.iter_mut() {
        *exclude = render_template(exclude, params);
    }

    for include in source.includes.iter_mut() {
        *include = render_template(include, params);
    }

    source.name = render_template(&source.name, params);
    source.path = render_template(&source.path, params);
}

fn apply_params_to_step(step: &mut ArtifactStep, params: &HashMap<String, String>) {
    if let Some(entrypoint) = step.entrypoint.as_mut() {
        *entrypoint = render_template(entrypoint, params);
    }

    if let Some(script) = step.script.as_mut() {
        *script = render_template(script, params);
    }

    for secret in step.secrets.iter_mut() {
        apply_params_to_secret(secret, params);
    }

    for argument in step.arguments.iter_mut() {
        *argument = render_template(argument, params);
    }

    for artifact in step.artifacts.iter_mut() {
        *artifact = render_template(artifact, params);
    }

    for environment in step.environments.iter_mut() {
        *environment = render_template(environment, params);
    }
}

fn apply_params_to_secret(secret: &mut ArtifactStepSecret, params: &HashMap<String, String>) {
    secret.name = render_template(&secret.name, params);
    secret.value = render_template(&secret.value, params);
}

fn render_template(value: &str, params: &HashMap<String, String>) -> String {
    let mut rendered = value.to_string();

    for (key, param) in params {
        let placeholder = format!("{{{{{key}}}}}");
        rendered = rendered.replace(&placeholder, param);
    }

    rendered
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::Code;

    fn param(name: &str, required: bool, default: &str) -> ArtifactFunctionParam {
        ArtifactFunctionParam {
            name: name.to_string(),
            required,
            description: String::new(),
            default: default.to_string(),
        }
    }

    fn params_map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_resolve_params_missing_required() {
        let spec = vec![param("version", true, "")];
        let params = HashMap::new();

        let err = resolve_params(&spec, params).expect_err("expected error");
        assert_eq!(err.code(), Code::InvalidArgument);
        assert!(err.message().contains("missing required param"));
    }

    #[test]
    fn test_resolve_params_default_applied() {
        let spec = vec![param("version", false, "1.0.0")];
        let params = HashMap::new();

        let resolved = resolve_params(&spec, params).expect("resolved");
        assert_eq!(resolved.get("version"), Some(&"1.0.0".to_string()));
    }

    #[test]
    fn test_resolve_params_override_default() {
        let spec = vec![param("version", false, "1.0.0")];
        let params = params_map(&[("version", "2.0.0")]);

        let resolved = resolve_params(&spec, params).expect("resolved");
        assert_eq!(resolved.get("version"), Some(&"2.0.0".to_string()));
    }

    #[test]
    fn test_resolve_params_preserves_extra_params() {
        let spec = Vec::new();
        let params = params_map(&[("extra", "value")]);

        let resolved = resolve_params(&spec, params).expect("resolved");
        assert_eq!(resolved.get("extra"), Some(&"value".to_string()));
    }

    #[test]
    fn test_apply_params_to_artifact_nested_fields() {
        let params = params_map(&[("version", "1.2.3")]);
        let mut artifact = Artifact {
            target: ArtifactSystem::UnknownSystem as i32,
            sources: vec![
                ArtifactSource {
                    digest: Some("digest-{{version}}".to_string()),
                    excludes: vec!["exclude-{{version}}".to_string()],
                    includes: vec!["include-{{version}}".to_string()],
                    name: "source-{{version}}".to_string(),
                    path: "/opt/{{version}}".to_string(),
                },
                ArtifactSource {
                    digest: None,
                    excludes: Vec::new(),
                    includes: Vec::new(),
                    name: "static".to_string(),
                    path: "/opt/static".to_string(),
                },
            ],
            steps: vec![
                ArtifactStep {
                    entrypoint: Some("run-{{version}}".to_string()),
                    script: Some("echo {{version}}".to_string()),
                    secrets: vec![ArtifactStepSecret {
                        name: "secret-{{version}}".to_string(),
                        value: "value-{{version}}".to_string(),
                    }],
                    arguments: vec!["--ver={{version}}".to_string()],
                    artifacts: vec!["artifact-{{version}}".to_string()],
                    environments: vec!["ENV={{version}}".to_string()],
                },
                ArtifactStep {
                    entrypoint: None,
                    script: None,
                    secrets: Vec::new(),
                    arguments: Vec::new(),
                    artifacts: Vec::new(),
                    environments: Vec::new(),
                },
            ],
            systems: Vec::new(),
            aliases: vec!["alias-{{version}}".to_string()],
            name: "name-{{version}}".to_string(),
        };

        apply_params_to_artifact(&mut artifact, &params);

        assert_eq!(artifact.name, "name-1.2.3");
        assert_eq!(artifact.aliases[0], "alias-1.2.3");

        assert_eq!(artifact.sources[0].digest.as_deref(), Some("digest-1.2.3"));
        assert_eq!(artifact.sources[0].excludes[0], "exclude-1.2.3");
        assert_eq!(artifact.sources[0].includes[0], "include-1.2.3");
        assert_eq!(artifact.sources[0].name, "source-1.2.3");
        assert_eq!(artifact.sources[0].path, "/opt/1.2.3");
        assert_eq!(artifact.sources[1].digest, None);

        assert_eq!(artifact.steps[0].entrypoint.as_deref(), Some("run-1.2.3"));
        assert_eq!(artifact.steps[0].script.as_deref(), Some("echo 1.2.3"));
        assert_eq!(artifact.steps[0].secrets[0].name, "secret-1.2.3");
        assert_eq!(artifact.steps[0].secrets[0].value, "value-1.2.3");
        assert_eq!(artifact.steps[0].arguments[0], "--ver=1.2.3");
        assert_eq!(artifact.steps[0].artifacts[0], "artifact-1.2.3");
        assert_eq!(artifact.steps[0].environments[0], "ENV=1.2.3");
        assert!(artifact.steps[1].entrypoint.is_none());
        assert!(artifact.steps[1].script.is_none());
    }
}
