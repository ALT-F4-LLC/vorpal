use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use tera::Tera;
use tokio::fs::write;
use tokio::process::Command;

mod profile;

pub async fn build(
    build_bin_paths: Vec<String>,
    build_env: HashMap<String, String>,
    build_path: &Path,
    sandbox_path: &Path,
) -> Result<Command> {
    let mut tera = Tera::default();

    let build_profile_path = build_path.join("package.sb");

    tera.add_raw_template("build_default", profile::STDENV_DEFAULT)
        .unwrap();

    let build_profile_context = tera::Context::new();

    let build_profile_data = tera
        .render("build_default", &build_profile_context)
        .unwrap();

    write(&build_profile_path, build_profile_data)
        .await
        .expect("failed to write sandbox profile");

    let sandbox_script_path = sandbox_path.join("sandbox.sh");

    if !sandbox_script_path.exists() {
        return Err(anyhow::anyhow!("sandbox 'sandbox.sh' not found"));
    }

    let build_script_path = build_path.join("package.sh");

    if !build_script_path.exists() {
        return Err(anyhow::anyhow!("build 'package.sh' not found"));
    }

    let command_args = [
        "-f",
        build_profile_path.to_str().unwrap(),
        sandbox_script_path.to_str().unwrap(),
        build_script_path.to_str().unwrap(),
    ];

    let mut command = Command::new("sandbox-exec");

    command.args(command_args);

    let build_source_path = build_path.join("source");

    if !build_source_path.exists() {
        return Err(anyhow::anyhow!("build 'source' not found"));
    }

    command.current_dir(build_source_path);

    for (key, value) in build_env.clone().into_iter() {
        command.env(key, value);
    }

    let mut path = format!(
        "{}:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
        build_path.join("bin").to_str().unwrap()
    );

    if !build_bin_paths.is_empty() {
        path = format!("{}:{}", build_bin_paths.join(":"), path);
    }

    command.env("PATH", path);

    Ok(command)
}
