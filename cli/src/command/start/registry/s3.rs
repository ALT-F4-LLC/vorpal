use vorpal_sdk::api::artifact::ArtifactSystem;

pub fn get_artifact_alias_key(
    name: &str,
    namespace: &str,
    system: ArtifactSystem,
    tag: &str,
) -> String {
    let system = system.as_str_name();

    format!("artifact/alias/{namespace}/{system}/{name}/{tag}")
}

pub fn get_artifact_archive_key(digest: &str, namespace: &str) -> String {
    format!("artifact/archive/{namespace}/{digest}.tar.zst")
}

pub fn get_artifact_config_key(digest: &str, namespace: &str) -> String {
    format!("artifact/config/{namespace}/{digest}.json")
}

pub fn get_artifact_function_key(name: &str, namespace: &str, tag: &str) -> String {
    format!("artifact/function/{namespace}/{name}/{tag}.json")
}
