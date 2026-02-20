pub mod models;
pub mod schema;

use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }

    pub fn initialize(&self) -> Result<()> {
        schema::create_tables(&self.conn)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
