use crate::api::Status;
use rusqlite::{Connection, Result};
use std::path::PathBuf;
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

pub fn connect(path: PathBuf) -> Result<Connection> {
    Connection::open(path)
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
