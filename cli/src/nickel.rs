use anyhow::Result;
use nickel_lang_core::eval::cache::lazy::CBNCache;
use nickel_lang_core::program::Program;
use nickel_lang_core::serialize;
use nickel_lang_core::serialize::ExportFormat::Json;
use sha256::digest;
use std::path::PathBuf;
use vorpal_schema::Config;

pub fn load_config(path: PathBuf) -> Result<(Config, String), anyhow::Error> {
    let mut program = Program::<CBNCache>::new_from_file(path, std::io::stderr())?;

    if let Ok(nickel_path) = std::env::var("NICKEL_IMPORT_PATH") {
        program.add_import_paths(nickel_path.split(':'));
    }

    let eval = match program.eval_full_for_export() {
        Ok(eval) => eval,
        Err(err) => {
            eprintln!("{:?}", err);
            std::process::exit(1);
        }
    };

    match serialize::validate(Json, &eval) {
        Ok(_) => {}
        Err(err) => {
            eprintln!("{:?}", err);
            std::process::exit(1);
        }
    }

    let data = match serialize::to_string(Json, &eval) {
        Ok(data) => data,
        Err(err) => {
            eprintln!("{:?}", err);
            std::process::exit(1);
        }
    };

    Ok((serde_json::from_str(&data)?, digest(data)))
}
