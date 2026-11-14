//! Fails when any SDK's build-target Rust toolchain literal differs from the
//! `channel` in the repository's `rust-toolchain.toml`.
//!
//! Every value is read from source text with an anchored marker: the trimmed
//! line must start with the marker, and the literal runs from the end of the
//! marker to the next `"`. A renamed or moved constant fails the test instead
//! of passing by omission.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> Result<PathBuf, String> {
    // sdk/codegen -> sdk -> repo root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .ok_or_else(|| "no repository root above sdk/codegen".to_string())
}

fn read(relative: &str) -> Result<String, String> {
    let path = repo_root()?.join(relative);
    fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))
}

/// Returns the literal on the line whose trimmed text starts with `marker`.
/// With `after`, the marker line must directly follow a line that starts with
/// `after` (a function body). Fails unless exactly one line matches.
fn literal(relative: &str, after: Option<&str>, marker: &str) -> Result<String, String> {
    let text = read(relative)?;
    let lines: Vec<&str> = text.lines().map(str::trim_start).collect();

    let found: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(i, line)| {
            line.starts_with(marker) && after.is_none_or(|a| *i > 0 && lines[i - 1].starts_with(a))
        })
        .filter_map(|(_, line)| {
            let rest = &line[marker.len()..];
            rest.find('"').map(|end| rest[..end].to_string())
        })
        .collect();

    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(format!(
            "{relative}: no line starts with `{marker}` (constant renamed or moved?)"
        )),
        many => Err(format!(
            "{relative}: {} lines start with `{marker}`: {many:?}",
            many.len()
        )),
    }
}

fn toolchain_channel() -> Result<String, String> {
    literal("rust-toolchain.toml", None, "channel = \"")
}

/// Every SDK copy of the build-target Rust toolchain version.
const PINS: [(&str, Option<&str>, &str); 5] = [
    (
        "sdk/rust/src/artifact/rust_toolchain.rs",
        Some("pub fn version() -> String {"),
        "\"",
    ),
    (
        "sdk/go/pkg/artifact/rust_toolchain.go",
        Some("func RustToolchainVersion() string {"),
        "return \"",
    ),
    (
        "sdk/typescript/src/artifact/rust_toolchain.ts",
        None,
        "export const RUST_TOOLCHAIN_VERSION = \"",
    ),
    (
        "sdk/typescript/src/artifact/language/rust.ts",
        None,
        "const RUST_TOOLCHAIN_VERSION = \"",
    ),
    (
        "sdk/python/src/vorpal_sdk/artifact/rust_toolchain.py",
        None,
        "RUST_TOOLCHAIN_VERSION = \"",
    ),
];

#[test]
fn every_sdk_rust_toolchain_matches_rust_toolchain_toml() -> Result<(), String> {
    let channel = toolchain_channel()?;

    let mut drift = Vec::new();
    for (file, after, marker) in PINS {
        let value = literal(file, after, marker)?;
        if value != channel {
            drift.push(format!("{file}: {value}"));
        }
    }

    if drift.is_empty() {
        return Ok(());
    }
    Err(format!(
        "SDK Rust toolchain literals differ from rust-toolchain.toml channel {channel}:\n  {}\n\
         Run the upgrade-sources skill with `--kind rust` to stage the {channel} sources and bump every SDK.",
        drift.join("\n  ")
    ))
}

#[test]
fn literal_extraction_fails_on_missing_marker() {
    let result = literal("rust-toolchain.toml", None, "THIS_MARKER_DOES_NOT_EXIST");
    assert!(result.is_err(), "a missing constant must fail, never pass");
}

#[test]
fn literal_extraction_is_anchored() -> Result<(), String> {
    // `RUST_TOOLCHAIN_VERSION = "` also occurs inside the TypeScript
    // `export const` line, but not at its start, so exactly one line matches.
    let value = literal(
        "sdk/typescript/src/artifact/rust_toolchain.ts",
        None,
        "export const RUST_TOOLCHAIN_VERSION = \"",
    )?;
    assert_ne!(value, "");

    let unanchored = literal(
        "sdk/typescript/src/artifact/rust_toolchain.ts",
        None,
        "RUST_TOOLCHAIN_VERSION = \"",
    );
    assert!(
        unanchored.is_err(),
        "a marker must match from the start of the line"
    );
    Ok(())
}
