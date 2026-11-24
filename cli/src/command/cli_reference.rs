//! The data behind the generated CLI reference on docs.vorpal.build.
//!
//! The test below walks the clap tree (`Cli::command()`) and compares it with
//! the committed `website/src/data/cli/main.json`, so the docs fail CI as soon
//! as a command, argument or flag changes without the data being updated.
//! The website build reads only the committed file, so building the docs
//! needs Bun and no Rust.
//!
//! Regenerate the file after changing the CLI:
//!
//! ```sh
//! VORPAL_CLI_REFERENCE_WRITE=1 cargo test -p vorpal-cli cli_reference
//! ```
//!
//! Some defaults are computed on the machine that runs the test (the host
//! system, and the socket path when `VORPAL_SOCKET_PATH` is set). They are
//! written as stable placeholders, so the file is the same on every machine,
//! and the docs say what each placeholder means.

use super::Cli;
use clap::{builder::PossibleValue, Arg, ArgAction, Command, CommandFactory};
use serde_json::{json, Value};
use std::path::PathBuf;
use vorpal_sdk::artifact::{get_default_address, system::get_system_default_str};

/// The default socket address, before `VORPAL_SOCKET_PATH` changes it.
const DEFAULT_SOCKET: &str = "unix:///var/lib/vorpal/vorpal.sock";

/// The placeholder for a default that is the host's own system.
const HOST_SYSTEM: &str = "{host-system}";

/// Replaces a default computed on this machine with a stable placeholder.
fn normalize_default(value: &str) -> String {
    if value == get_system_default_str() {
        return HOST_SYSTEM.to_string();
    }
    if value == get_default_address() {
        return DEFAULT_SOCKET.to_string();
    }
    value.to_string()
}

fn help_text(arg_help: Option<&clap::builder::StyledStr>) -> Value {
    arg_help.map_or(Value::Null, |h| Value::String(h.to_string()))
}

fn action_name(action: &ArgAction) -> &'static str {
    match action {
        ArgAction::SetTrue => "flag",
        ArgAction::SetFalse => "flag-false",
        ArgAction::Append => "append",
        ArgAction::Count => "count",
        _ => "set",
    }
}

fn possible_values(arg: &Arg) -> Value {
    let values: Vec<Value> = arg
        .get_possible_values()
        .iter()
        .filter(|v| !v.is_hide_set())
        .map(PossibleValue::get_name)
        .map(|v| Value::String(v.to_string()))
        .collect();
    if values.is_empty() || matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) {
        Value::Null
    } else {
        Value::Array(values)
    }
}

fn arg_json(command: &Command, arg: &Arg) -> Value {
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|v| normalize_default(&v.to_string_lossy()))
        .collect();
    let value_names: Vec<String> = arg.get_value_names().map_or_else(
        || vec![arg.get_id().as_str().to_uppercase()],
        |names| names.iter().map(ToString::to_string).collect(),
    );
    // clap treats a conflict as two-way, so list it on both flags.
    let mut conflicts: Vec<String> = command
        .get_arguments()
        .filter(|other| {
            command
                .get_arg_conflicts_with(arg)
                .iter()
                .any(|a| a.get_id() == other.get_id())
                || command
                    .get_arg_conflicts_with(other)
                    .iter()
                    .any(|a| a.get_id() == arg.get_id())
        })
        .filter_map(|a| a.get_long().map(|l| format!("--{l}")))
        .collect();
    conflicts.sort();
    conflicts.dedup();
    let takes_value = !matches!(
        arg.get_action(),
        ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count
    );

    json!({
        "id": arg.get_id().as_str(),
        "long": arg.get_long(),
        "short": arg.get_short().map(String::from),
        "value_name": if takes_value { Value::String(value_names.join(" ")) } else { Value::Null },
        "action": action_name(arg.get_action()),
        "required": arg.is_required_set(),
        "global": arg.is_global_set(),
        "default": match defaults.as_slice() {
            [] => Value::Null,
            [one] => Value::String(one.clone()),
            many => json!(many),
        },
        // `--unlock` on `prepare`: the value may be left out.
        "optional_value": takes_value
            && arg.get_num_args().is_some_and(|n| n.min_values() == 0 && n.max_values() > 0),
        "require_equals": arg.is_require_equals_set(),
        "env": arg.get_env().map(|e| e.to_string_lossy().to_string()),
        "possible_values": possible_values(arg),
        "conflicts_with": conflicts,
        "trailing": arg.is_trailing_var_arg_set() || arg.is_last_set(),
        "help": help_text(arg.get_help()),
        "long_help": help_text(arg.get_long_help()),
    })
}

/// Whether clap added this argument (help and version).
fn is_builtin(arg: &Arg) -> bool {
    matches!(
        arg.get_action(),
        ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version
    )
}

fn collect(command: &Command, parent: &[String], root_globals: &[String], out: &mut Vec<Value>) {
    for sub in command.get_subcommands() {
        if sub.is_hide_set() || sub.get_name() == "help" {
            continue;
        }
        let mut path = parent.to_vec();
        path.push(sub.get_name().to_string());

        // Global flags from the root (`--level`) are listed once, at the top.
        let own = |a: &&Arg| {
            let inherited = a.is_global_set() && root_globals.contains(&a.get_id().to_string());
            !(is_builtin(a) || inherited)
        };
        let arguments: Vec<Value> = sub
            .get_positionals()
            .filter(own)
            .map(|a| arg_json(sub, a))
            .collect();
        let options: Vec<Value> = sub
            .get_arguments()
            .filter(|a| !a.is_positional())
            .filter(own)
            .map(|a| arg_json(sub, a))
            .collect();
        let subcommands: Vec<Value> = sub
            .get_subcommands()
            .filter(|s| !s.is_hide_set() && s.get_name() != "help")
            .map(|s| Value::String(format!("{} {}", path.join(" "), s.get_name())))
            .collect();

        out.push(json!({
            "path": path.join(" "),
            "about": help_text(sub.get_about()),
            "long_about": help_text(sub.get_long_about()),
            "arguments": arguments,
            "options": options,
            "subcommands": subcommands,
        }));

        collect(sub, &path, root_globals, out);
    }
}

/// The CLI reference data, as the pretty JSON committed to the website.
pub fn generate() -> Result<String, serde_json::Error> {
    let mut root = Cli::command();
    root.build();

    let globals: Vec<&Arg> = root
        .get_arguments()
        .filter(|a| !is_builtin(a) && a.is_global_set())
        .collect();
    let root_globals: Vec<String> = globals.iter().map(|a| a.get_id().to_string()).collect();
    let global_json: Vec<Value> = globals.iter().map(|a| arg_json(&root, a)).collect();

    let mut commands = Vec::new();
    collect(&root, &[], &root_globals, &mut commands);

    let data = json!({
        "$comment": "Generated from cli/src by `VORPAL_CLI_REFERENCE_WRITE=1 cargo test -p vorpal-cli cli_reference`. Do not edit: CI fails when this file and the CLI differ.",
        // The binary's name ([[bin]] in cli/Cargo.toml), not the package's.
        "name": "vorpal",
        "version": env!("CARGO_PKG_VERSION"),
        "placeholders": {
            HOST_SYSTEM: "this machine's system",
        },
        "global": global_json,
        "commands": commands,
    });

    let mut text = serde_json::to_string_pretty(&data)?;
    text.push('\n');
    Ok(text)
}

fn data_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../website/src/data/cli/main.json")
}

#[cfg(test)]
mod tests {
    use super::{data_path, generate};

    #[test]
    fn cli_reference_data_is_current() -> Result<(), Box<dyn std::error::Error>> {
        let generated = generate()?;
        let path = data_path();

        if std::env::var_os("VORPAL_CLI_REFERENCE_WRITE").is_some() {
            std::fs::write(&path, &generated)?;
            return Ok(());
        }

        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            committed == generated,
            "{} is out of date with the CLI. Regenerate it with \
             `VORPAL_CLI_REFERENCE_WRITE=1 cargo test -p vorpal-cli cli_reference` \
             and commit the result (then add docs for any new flag in \
             website/src/data/cli/overlay.yaml).",
            path.display()
        );
        Ok(())
    }
}
