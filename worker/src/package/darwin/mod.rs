use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use tera::Tera;
use tokio::fs::write;
use tokio::process::Command;
use vorpal_store::temps::create_temp_file;

mod profile;

pub async fn build(
    bin_paths: Vec<String>,
    env_var: HashMap<String, String>,
    script_path: &Option<PathBuf>,
    source_dir_path: &PathBuf,
    stdenv_dir_path: Option<PathBuf>,
) -> Result<Command> {
    if script_path.is_none() {
        bail!("script path is not provided")
    }

    let script_file_path = script_path.as_ref().unwrap();

    let stdenv_dir_path = stdenv_dir_path.expect("failed to get stdenv path");

    let profile_file_path = create_temp_file("sb").await?;

    let mut tera = Tera::default();

    tera.add_raw_template("sandbox_default", profile::SANDBOX_DEFAULT)
        .unwrap();

    let profile_file_context = tera::Context::new();

    let profile_file_data = tera
        .render("sandbox_default", &profile_file_context)
        .unwrap();

    write(&profile_file_path, profile_file_data)
        .await
        .expect("failed to write sandbox profile");

    let command_args = [
        "-f",
        profile_file_path.to_str().unwrap(),
        script_file_path.to_str().unwrap(),
    ];

    let mut command = Command::new("/usr/bin/sandbox-exec");

    command.args(command_args);

    command.current_dir(source_dir_path);

    for (key, value) in env_var.clone().into_iter() {
        command.env(key, value);
    }

    let mut path = format!(
        "{}:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
        stdenv_dir_path.join("bin").to_str().unwrap()
    );

    if !bin_paths.is_empty() {
        path = format!("{}:{}", bin_paths.join(":"), path);
    }

    command.env("PATH", path);

    Ok(command)
}
