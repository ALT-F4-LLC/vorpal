use console::style;
use std::collections::HashMap;
use std::path::Path;
use vorpal_schema::api::package::PackageOutput;
use vorpal_schema::Package;
use vorpal_store::paths::{get_package_archive_path, get_package_path};

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

pub fn print_config(file_path: &Path) {
    println!(
        "{} {} {} {}",
        connector_start(),
        bold("Config:"),
        file_path.display(),
        badge_success(),
    );
}

pub fn format_package_name(package_name: &str) -> String {
    format!(
        "{}{} {} ➜",
        connector_start(),
        connector_half(),
        style(package_name).bold(),
    )
}

fn print_package(
    build_map: &HashMap<String, Package>,
    cached_count: &mut usize,
    package_name: &str,
    prefix: &str,
) {
    match build_map.get(package_name) {
        None => eprintln!("Package not found: {}", package_name),
        Some(package) => {
            let hash_default = "".to_string();

            let hash = package.source_hash.as_ref().unwrap_or(&hash_default);

            let exists = get_package_path(hash, package_name).exists();

            let exists_archive = get_package_archive_path(hash, package_name).exists();

            let cached = if exists || exists_archive {
                style("[✓]").green()
            } else {
                style("[✗]").red()
            };

            if exists || exists_archive {
                *cached_count += 1;
            }

            let prefix = style(prefix).dim();

            let mut short_hash = "unknown".to_string();

            if !hash.is_empty() {
                short_hash = hash[..7].to_string();
            }

            println!(
                "{}{}{} {} {} {}",
                style(CONNECTOR_START).dim(),
                prefix,
                style(CONNECTOR_HALF).dim(),
                style(package_name),
                style(format!("({})", short_hash)).dim().italic(),
                cached
            );

            for p in package.packages.iter() {
                print_package(
                    build_map,
                    cached_count,
                    p.name.as_str(),
                    &format!("{}{}", prefix, style(CONNECTOR_HALF).dim()),
                );
            }
        }
    }
}

pub fn print_packages(build_map: &HashMap<String, Package>, build_order: &[String]) {
    println!(
        "{} {} ({} total)",
        style(CONNECTOR_START).dim(),
        style("Packages:").bold(),
        style(build_order.len()).green(),
    );

    let mut cached_count = 0;

    for package_name in build_order.iter() {
        print_package(build_map, &mut cached_count, package_name, "");
    }

    println!(
        "{} {} {} {}",
        style(CONNECTOR_START).dim(),
        style("Progress:").bold(),
        style(cached_count).green(),
        style(format!("out of {} packages", build_order.len())).dim(),
    );
}

pub fn print_source_archive(package_name: &str, source_archive: &str) {
    println!(
        "{} Source archive: {} {}",
        format_package_name(package_name),
        style(source_archive).italic(),
        badge_success(),
    );
}

pub fn print_source_cache(source_cache: &str) {
    println!(
        "{} {} {} {}",
        connector_start(),
        bold("Source cache:"),
        source_cache,
        badge_success(),
    );
}

pub fn print_source_url(package_name: &str, url: &str) {
    println!(
        "{} Source url: {} {}",
        format_package_name(package_name),
        style(url).italic(),
        badge_success(),
    );
}

pub fn print_system(system: &str) {
    println!(
        "{} {} {} {}",
        connector_start(),
        bold("System:"),
        system,
        badge_success(),
    );
}

pub fn print_packages_list(package_name: &str, packages: &[String]) {
    println!(
        "{} Packages: {}",
        format_package_name(package_name),
        style(packages.join(", ")).cyan()
    );
}

pub fn print_package_hash(package_name: &str, package_hash: &str) {
    println!(
        "{} Source hash: {} {}",
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
