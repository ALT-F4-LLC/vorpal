use anyhow::{bail, Result};
use inquire::{InquireError, Select};
use std::{collections::BTreeMap, env::current_dir};
use tokio::fs::{create_dir_all, write};
use tracing::{info, subscriber, warn, Level};
use tracing_subscriber::{fmt::writer::MakeWriterExt, FmtSubscriber};

pub async fn run(level: Level) -> Result<()> {
    // Setup logging

    let subscriber_writer = std::io::stderr.with_max_level(level);

    let mut subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .with_writer(subscriber_writer)
        .without_time();

    if [Level::DEBUG, Level::TRACE].contains(&level) {
        subscriber = subscriber.with_file(true).with_line_number(true);
    }

    let subscriber = subscriber.finish();

    subscriber::set_global_default(subscriber).expect("setting default subscriber");

    let options: Vec<&str> = vec!["Go", "Rust"];

    let answer: Result<&str, InquireError> =
        Select::new("Which language would you like to use?", options).prompt();

    let mut template = BTreeMap::new();

    match answer {
        Ok(choice) => match choice {
            "Go" => {
                template.insert("Vorpal.lock", include_str!("template/go/Vorpal.lock"));
                template.insert("Vorpal.toml", include_str!("template/go/Vorpal.toml"));
                template.insert("go.mod", include_str!("template/go/go.mod"));
                template.insert("go.sum", include_str!("template/go/go.sum"));
                template.insert("main.go", include_str!("template/go/main.go"));
                template.insert("vorpal.go", include_str!("template/go/vorpal.go"));
            }

            "Rust" => {
                template.insert("src/main.rs", include_str!("template/rust/src/main.rs"));
                template.insert("src/vorpal.rs", include_str!("template/rust/src/vorpal.rs"));
                template.insert("Cargo.toml", include_str!("template/rust/Cargo.toml"));
                template.insert("Vorpal.toml", include_str!("template/rust/Vorpal.toml"));
            }

            _ => bail!("invalid 'language' choice"),
        },

        Err(_) => {
            bail!("failed to get user input");
        }
    }

    for (path, content) in template {
        let path = path.to_string();
        let content = content.to_string();

        let mut file_path = current_dir().expect("failed to get current directory");

        file_path.push(path.clone());

        if let Some(parent) = file_path.parent() {
            create_dir_all(parent)
                .await
                .expect("failed to create directory");
        }

        if file_path.exists() {
            warn!("File already exists: {}", path);
            continue;
        }

        write(file_path, content)
            .await
            .expect("failed to write file");

        info!("Created file: {}", path);
    }

    Ok(())
}
