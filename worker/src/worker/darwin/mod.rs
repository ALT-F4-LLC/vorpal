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
) -> Result<Command> {
    let mut tera = Tera::default();

    let profile_path = build_path.join("package.sb");

    tera.add_raw_template("build_default", profile::STDENV_DEFAULT)
        .unwrap();

    let profile_context = tera::Context::new();

    let profile_data = tera.render("build_default", &profile_context).unwrap();

    write(&profile_path, profile_data)
        .await
        .expect("failed to write sandbox profile");

    let script_path = build_path.join("package.sh");

    if !script_path.exists() {
        return Err(anyhow::anyhow!("build 'package.sh' not found"));
    }

    let command_args = [
        "-f",
        profile_path.to_str().unwrap(),
        script_path.to_str().unwrap(),
    ];

    let mut command = Command::new("sandbox-exec");

    command.args(command_args);

    let source_path = build_path.join("source");

    if !source_path.exists() {
        return Err(anyhow::anyhow!("build 'source' not found"));
    }

    command.current_dir(source_path);

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
