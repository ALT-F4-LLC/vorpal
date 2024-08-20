use anyhow::Result;
use nickel_lang_core::{
    error::report::ErrorFormat,
    eval::cache::lazy::CBNCache,
    program::Program,
    serialize::{to_string, validate, ExportFormat::Json},
};
use sha256::digest;
use std::io::Cursor;
use vorpal_schema::{api::package::PackageSystem, Config};

pub fn load_config(
    config: &String,
    target: PackageSystem,
) -> Result<(Config, String), anyhow::Error> {
    let config_target = match target {
        PackageSystem::Aarch64Linux => "aarch64-linux",
        PackageSystem::Aarch64Macos => "aaarch64-macos",
        PackageSystem::Unknown => anyhow::bail!("unknown target"),
        PackageSystem::X8664Linux => "x86_64-linux",
        PackageSystem::X8664Macos => "x86_64-macos",
    };

    let config_str = format!(
        "let config = import \"{}\" in config \"{}\"",
        config, config_target,
    );

    let src = Cursor::new(config_str);

    let mut program = Program::<CBNCache>::new_from_source(src, "vorpal", std::io::stdout())?;

    if let Ok(nickel_path) = std::env::var("NICKEL_IMPORT_PATH") {
        program.add_import_paths(nickel_path.split(':'));
    }

    let eval = match program.eval_full_for_export() {
        Ok(eval) => eval,
        Err(err) => {
            program.report(err, ErrorFormat::Json);
            std::process::exit(1);
        }
    };

    match validate(Json, &eval) {
        Ok(_) => {}
        Err(err) => {
            eprintln!("{:?}", err);
            std::process::exit(1);
        }
    }

    let data = match to_string(Json, &eval) {
        Ok(data) => data,
        Err(err) => {
            eprintln!("{:?}", err);
            std::process::exit(1);
        }
    };

    Ok((serde_json::from_str(&data)?, digest(data)))
}
