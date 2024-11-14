use console::style;
use std::path::Path;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;

pub static CONNECTOR_START: &str = "├─";

pub static CONNECTOR_HALF: &str = "──";

pub static CONNECTOR_END: &str = "└─";

pub fn badge_success() -> String {
    style("[✓]").green().to_string()
}

pub fn connector_end() -> String {
    style(CONNECTOR_END).dim().to_string()
}

pub fn connector_half() -> String {
    style(CONNECTOR_HALF).dim().to_string()
}

pub fn connector_start() -> String {
    style(CONNECTOR_START).dim().to_string()
}

pub fn bold(text: &str) -> String {
    style(text).bold().to_string()
}

pub fn print_build_order(build_order: &[ArtifactId]) {
    let build_order = build_order
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<String>>();

    println!(
        "{} {} {}",
        connector_start(),
        bold("Building:"),
        build_order.join(", "),
    );
}

// pub fn print_config(file_path: &Path) {
//     println!(
//         "{} {} {} {}",
//         connector_start(),
//         bold("Config:"),
//         file_path.display(),
//         badge_success(),
//     );
// }

pub fn format_artifact_name(artifact_name: &str) -> String {
    format!(
        "{}{} {} ➜",
        connector_start(),
        connector_half(),
        style(artifact_name).bold().on_color256(238),
    )
}

pub fn print_artifacts(build_order: &[ArtifactId]) {
    println!(
        "{} {} {} total",
        style(CONNECTOR_START).dim(),
        style("Artifacts:").bold(),
        style(build_order.len()),
    );
}

pub fn print_source_cache(artifact_name: &str, source_cache: &str) {
    println!(
        "{} Source cache: {} {}",
        format_artifact_name(artifact_name),
        style(source_cache).italic(),
        badge_success(),
    );
}

pub enum SourceStatus {
    Complete,
    Pending,
}

pub fn print_source_url(artifact_name: &str, status: SourceStatus, url: &str) {
    let badge = match status {
        SourceStatus::Complete => style("[✓]").green(),
        SourceStatus::Pending => style("[…]").color256(208),
    };

    println!(
        "{} Source: {} {}",
        format_artifact_name(artifact_name),
        style(url).italic(),
        badge,
    );
}

// pub fn print_system(system: &str) {
//     println!(
//         "{} {} {} {}",
//         connector_start(),
//         bold("System:"),
//         system,
//         badge_success(),
//     );
// }

pub fn print_artifacts_list(artifact_name: &str, artifacts: &[String]) {
    println!(
        "{} Artifacts: {}",
        format_artifact_name(artifact_name),
        style(artifacts.join(", ")).cyan()
    );
}

pub fn print_artifact_hash(artifact_name: &str, artifact_hash: &str) {
    println!(
        "{} Hash: {} {}",
        format_artifact_name(artifact_name),
        style(artifact_hash).italic(),
        badge_success(),
    );
}

pub fn print_artifact_archive(artifact_name: &str, artifact_archive: &Path) {
    println!(
        "{} Archive: {}",
        format_artifact_name(artifact_name),
        style(artifact_archive.display().to_string()).green()
    );
}

pub fn print_artifact_output(artifact_name: &str, artifact_output: &ArtifactId) {
    println!(
        "{} {}",
        format_artifact_name(artifact_name),
        style(artifact_output.hash.clone()).green()
    );
}

pub fn print_artifact_log(artifact_name: &str, artifact_log: &String) {
    println!(
        "{} {}",
        format_artifact_name(artifact_name),
        style(artifact_log)
    );
}
