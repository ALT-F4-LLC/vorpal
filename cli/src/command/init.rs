use anyhow::{bail, Result};
use inquire::{InquireError, Select};
use std::{collections::BTreeMap, path::Path};
use tokio::fs::{create_dir_all, write};
use tracing::{info, warn};

pub async fn run(name: &str, path: &Path) -> Result<()> {
    let options: Vec<&str> = vec!["Go", "Rust", "TypeScript"];

    let answer: Result<&str, InquireError> =
        Select::new("Which language would you like to use?", options).prompt();

    let mut template = BTreeMap::new();

    match answer {
        Ok(choice) => match choice {
            "Go" => {
                template.insert(
                    "cmd/example/main.go",
                    include_str!("template/go/cmd/example/main.go"),
                );
                template.insert(
                    "cmd/vorpal/main.go",
                    include_str!("template/go/cmd/vorpal/main.go"),
                );
                template.insert("go.mod", include_str!("template/go/go.mod"));
                template.insert("go.sum", include_str!("template/go/go.sum"));
                template.insert("Vorpal.toml", include_str!("template/go/Vorpal.toml"));
            }

            "Rust" => {
                template.insert("src/main.rs", include_str!("template/rust/src/main.rs"));
                template.insert("src/vorpal.rs", include_str!("template/rust/src/vorpal.rs"));
                template.insert("Cargo.lock", include_str!("template/rust/Cargo.lock"));
                template.insert("Cargo.toml", include_str!("template/rust/Cargo.toml"));
                template.insert("Vorpal.toml", include_str!("template/rust/Vorpal.toml"));
            }

            "TypeScript" => {
                template.insert(
                    "src/vorpal.ts",
                    include_str!("template/typescript/src/vorpal.ts"),
                );
                template.insert(
                    "src/main.ts",
                    include_str!("template/typescript/src/main.ts"),
                );
                template.insert(
                    "package.json",
                    include_str!("template/typescript/package.json"),
                );
                template.insert(
                    "tsconfig.json",
                    include_str!("template/typescript/tsconfig.json"),
                );
                template.insert(
                    "Vorpal.toml",
                    include_str!("template/typescript/Vorpal.toml"),
                );
            }

            _ => bail!("invalid 'language' choice"),
        },

        Err(_) => {
            bail!("failed to get user input");
        }
    }

    for (template_path, content) in template {
        // Replace "example" with the provided name in file paths
        let template_path = template_path.replace("cmd/example", &format!("cmd/{}", name));

        // Replace "example" with the provided name in file content
        let content = content.replace("example", name);

        let mut file_path = path.to_path_buf();
        file_path.push(&template_path);

        if let Some(parent) = file_path.parent() {
            create_dir_all(parent)
                .await
                .expect("failed to create directory");
        }

        if file_path.exists() {
            warn!("File already exists: {}", template_path);
            continue;
        }

        write(file_path, content)
            .await
            .expect("failed to write file");

        info!("Created file: {}", template_path);
    }

    Ok(())
}
