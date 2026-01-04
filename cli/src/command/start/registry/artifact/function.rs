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
