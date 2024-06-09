use crate::api::Status;
use crate::store::paths;
use rusqlite::{Connection, Result};
use std::path::Path;
use tracing::info;
use uuid::Uuid;

pub struct Source {
    pub id: i32,
    pub hash: String,
    pub name: String,
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

pub fn connect<P: AsRef<Path>>(path: P) -> Result<Connection> {
    Connection::open(path)
}

pub fn init() -> Result<(), anyhow::Error> {
    let db_path = paths::get_database();
    let db = connect(&db_path)?;

    db.execute(
        "CREATE TABLE IF NOT EXISTS source (
                id  INTEGER PRIMARY KEY,
                hash TEXT NOT NULL,
                name TEXT NOT NULL
            )",
        [],
    )?;

    info!("database: {:?}", db_path.display());

    if let Err(e) = db.close() {
        return Err(e.1.into());
    }

    Ok(())
}

pub fn insert_source(conn: &Connection, hash: &str, name: &str) -> Result<usize> {
    conn.execute(
        "INSERT INTO source (hash, name) VALUES (?1, ?2)",
        [hash, name],
    )
}

pub fn find_source_by_id(conn: &Connection, id: i32) -> Result<Source> {
    let mut stmt = conn.prepare("SELECT * FROM source WHERE id = ?")?;
    let mut rows = stmt.query([id])?;

    let row = rows
        .next()?
        .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)?;

    Ok(Source {
        id: row.get(0)?,
        hash: row.get(1)?,
        name: row.get(2)?,
    })
}

pub fn find_source(conn: &Connection, hash: &str, name: &str) -> Result<Source> {
    let mut stmt = conn.prepare("SELECT * FROM source WHERE hash = ? AND name = ?")?;
    let mut rows = stmt.query([hash, name])?;

    let row = rows
        .next()?
        .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)?;

    Ok(Source {
        id: row.get(0)?,
        hash: row.get(1)?,
        name: row.get(2)?,
    })
}
