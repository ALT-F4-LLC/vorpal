use std::process::Command;

fn run_command(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn main() {
    // Git commit hash
    let git_hash = run_command("git", &["rev-parse", "--short", "HEAD"]);
    println!("cargo:rustc-env=VORPAL_GIT_HASH={git_hash}");

    // Build timestamp (Unix epoch)
    let build_time = run_command("date", &["+%s"]);
    println!("cargo:rustc-env=VORPAL_BUILD_TIME={build_time}");

    // Rebuild triggers
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/heads");
}
