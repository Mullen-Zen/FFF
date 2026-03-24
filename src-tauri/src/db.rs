use rusqlite::{Connection, Result};

pub fn init_db() -> Result<()> {
    let conn = Connection::open("files_metadata.db")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL UNIQUE,
            file_type TEXT,
            size INTEGER,
            modified_time INTEGER
        )",
        [],
    )?;

    println!("Database initialized successfully.");

    Ok(())
}