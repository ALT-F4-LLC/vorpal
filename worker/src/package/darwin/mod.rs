use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tera::Tera;
use tokio::fs::write;
use tokio::process::Command;
use vorpal_store::temps::create_temp_file;

mod profile;

pub async fn build(
    bin_paths: Vec<String>,
    env_var: HashMap<String, String>,
    script_path: &Path,
    source_dir_path: &PathBuf,
    stdenv_dir_path: Option<PathBuf>,
) -> Result<Command> {
    let stdenv_dir_path = stdenv_dir_path.expect("failed to get stdenv path");

    let sandbox_profile_path = create_temp_file("sb").await?;

    let mut tera = Tera::default();

    tera.add_raw_template("sandbox_default", profile::SANDBOX_DEFAULT)
        .unwrap();

    let sandbox_profile_context = tera::Context::new();

    let sandbox_profile_data = tera
        .render("sandbox_default", &sandbox_profile_context)
        .unwrap();

    write(&sandbox_profile_path, sandbox_profile_data)
        .await
        .expect("failed to write sandbox profile");

    let build_command_args = [
        "-f",
        sandbox_profile_path.to_str().unwrap(),
        script_path.to_str().unwrap(),
    ];

    let mut sandbox_command = Command::new("/usr/bin/sandbox-exec");

    sandbox_command.args(build_command_args);

    sandbox_command.current_dir(source_dir_path);

    for (key, value) in env_var.clone().into_iter() {
        sandbox_command.env(key, value);
    }

    let path_default = format!(
        "{}:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
        stdenv_dir_path.join("bin").to_str().unwrap()
    );

    sandbox_command.env("PATH", path_default.as_str());

    if !bin_paths.is_empty() {
        let path = format!("{}:{}", bin_paths.join(":"), path_default);

        sandbox_command.env("PATH", path);
    }

    Ok(sandbox_command)
}
