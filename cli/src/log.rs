use console::style;
use std::path::Path;
use vorpal_schema::vorpal::package::v0::PackageOutput;

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

pub fn print_build_order(build_order: &[String]) {
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

pub fn format_package_name(package_name: &str) -> String {
    format!(
        "{}{} {} ➜",
        connector_start(),
        connector_half(),
        style(package_name).bold().on_color256(238),
    )
}

pub fn print_packages(build_order: &[String]) {
    println!(
        "{} {} {} total",
        style(CONNECTOR_START).dim(),
        style("Packages:").bold(),
        style(build_order.len()),
    );
}

pub fn print_source_cache(package_name: &str, source_cache: &str) {
    println!(
        "{} Source cache: {} {}",
        format_package_name(package_name),
        style(source_cache).italic(),
        badge_success(),
    );
}

pub enum SourceStatus {
    Complete,
    Pending,
}

pub fn print_source_url(package_name: &str, status: SourceStatus, url: &str) {
    let badge = match status {
        SourceStatus::Complete => style("[✓]").green(),
        SourceStatus::Pending => style("[…]").color256(208),
    };

    println!(
        "{} Source: {} {}",
        format_package_name(package_name),
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

pub fn print_packages_list(package_name: &str, packages: &[String]) {
    println!(
        "{} Packages: {}",
        format_package_name(package_name),
        style(packages.join(", ")).cyan()
    );
}

pub fn print_package_hash(package_name: &str, package_hash: &str) {
    println!(
        "{} Hash: {} {}",
        format_package_name(package_name),
        style(package_hash).italic(),
        badge_success(),
    );
}

pub fn print_package_archive(package_name: &str, package_archive: &Path) {
    println!(
        "{} Archive: {}",
        format_package_name(package_name),
        style(package_archive.display().to_string()).green()
    );
}

pub fn print_package_output(package_name: &str, package_output: &PackageOutput) {
    println!(
        "{} Output: {}",
        format_package_name(package_name),
        style(package_output.hash.clone()).green()
    );
}

pub fn print_package_log(package_name: &str, package_log: &String) {
    println!(
        "{} {}",
        format_package_name(package_name),
        style(package_log)
    );
}
