use crate::api::Status;
use rusqlite::{Connection, Result};
use std::path::PathBuf;
use uuid::Uuid;

pub struct Source {
    pub id: i64,
    pub uri: String,
}

pub struct Build {
    pub build_phase: String,
    pub error: Option<String>, // populated on failed
    pub id: Uuid,
    pub ignore_paths: Vec<String>,
    pub install_phase: String,
    pub package_id: Option<Uuid>, // populated on completed
    pub source_id: Uuid,
    pub status: Status,
}

pub fn connect(path: PathBuf) -> Result<Connection> {
    Connection::open(path)
}

pub fn insert_source(conn: &Connection, uri: &PathBuf) -> Result<()> {
    conn.execute(
        "INSERT INTO source (uri) VALUES (?)",
        [uri.display().to_string()],
    )?;
    Ok(())
}

pub fn find_source(conn: &Connection, uri: &PathBuf) -> Result<Source> {
    let mut stmt = conn.prepare("SELECT * FROM source WHERE uri = ?")?;
    let mut rows = stmt.query([uri.display().to_string()])?;

    let row = rows
        .next()?
        .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)?;

    Ok(Source {
        id: row.get(0)?,
        uri: row.get(1)?,
    })
}
