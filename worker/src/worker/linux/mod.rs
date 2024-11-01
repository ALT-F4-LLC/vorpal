use anyhow::{anyhow, bail, Result};
use itertools::Itertools;
use std::collections::HashMap;
use std::path::Path;
use tokio::{fs::create_dir_all, process::Command};
use vorpal_schema::vorpal::package::v0::PackageSandboxPath;

#[allow(clippy::too_many_arguments)]
pub async fn build(
    env_vars: HashMap<String, String>,
    home_path: &Path,
    package_path: &Path,
    package_paths: Vec<String>,
    package_sandbox_paths: Vec<PackageSandboxPath>,
    script_path: &Path,
    source_path: &Path,
) -> Result<Command> {
    create_dir_all(&home_path)
        .await
        .map_err(|err| anyhow!("failed to create home directory: {:?}", err))?;

    let mut command_args = vec![
        vec![
            "--bind",
            home_path.to_str().unwrap(),
            home_path.to_str().unwrap(),
        ],
        vec![
            "--bind",
            source_path.to_str().unwrap(),
            source_path.to_str().unwrap(),
        ],
        vec![
            "--bind",
            package_path.to_str().unwrap(),
            package_path.to_str().unwrap(),
        ],
        vec!["--chdir", source_path.to_str().unwrap()],
        vec!["--clearenv"],
        vec!["--dev", "/dev"],
        vec!["--proc", "/proc"],
        vec!["--setenv", "HOME", home_path.to_str().unwrap()],
        vec![
            "--ro-bind",
            script_path.to_str().unwrap(),
            script_path.to_str().unwrap(),
        ],
        vec!["--tmpfs", "/tmp"],
        vec!["--unshare-all"],
        vec!["--share-net"],
    ];

    let mut sandbox_library_paths = vec![];

    for sandbox_path in &package_sandbox_paths {
        let source_path = Path::new(&sandbox_path.source).to_path_buf();

        if !source_path.exists() {
            bail!("sandbox 'source' path does not exist: {:?}", source_path);
        }

        // Add library paths for executables

        if let Ok(Some(kind)) = infer::get_from_path(source_path.clone()) {
            if kind.mime_type() == "application/x-executable" {
                let analyzer = lddtree::DependencyAnalyzer::default();
                let analyzer_deps = analyzer.analyze(source_path.clone()).unwrap();

                analyzer_deps.libraries.iter().for_each(|(_, library)| {
                    sandbox_library_paths.push(library.path.display().to_string());
                });
            }
        }

        // Add path with mapping

        command_args.push(vec![
            "--ro-bind",
            sandbox_path.source.as_str(),
            sandbox_path.target.as_str(),
        ]);
    }

    // Deduplicate library paths

    let sandbox_library_paths = sandbox_library_paths.iter().unique().collect::<Vec<_>>();

    // Add library paths to command

    for library_path in sandbox_library_paths {
        command_args.push(vec![
            "--ro-bind",
            library_path.as_str(),
            library_path.as_str(),
        ]);
    }

    // Add package paths to command

    for package in &package_paths {
        command_args.push(vec!["--ro-bind", package.as_str(), package.as_str()]);
    }

    // Add environment variables to command

    for (key, value) in &env_vars {
        command_args.push(vec!["--setenv", key, value]);
    }

    // Add script path to command

    command_args.push(vec![script_path.to_str().unwrap()]);

    // Create command

    let mut command = Command::new("bwrap");

    command.args(command_args.iter().flatten());

    println!("command: {:?}", command);

    Ok(command)
}
