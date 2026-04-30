use rusqlite::{params, Connection};
use std::path::Path;

use crate::error::Result;

pub fn open_connection(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS files (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            path        TEXT NOT NULL UNIQUE,
            name        TEXT NOT NULL,
            extension   TEXT,
            size        INTEGER,
            modified    INTEGER,
            indexed_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tags (
            id      INTEGER PRIMARY KEY AUTOINCREMENT,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            tag     TEXT NOT NULL,
            source  TEXT NOT NULL CHECK(source IN ('ai', 'auto'))
        );

        CREATE TABLE IF NOT EXISTS indexed_dirs (
            id   INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE
        );

        CREATE INDEX IF NOT EXISTS idx_tags_tag     ON tags(tag);
        CREATE INDEX IF NOT EXISTS idx_files_name   ON files(name);
        CREATE INDEX IF NOT EXISTS idx_tags_file_id ON tags(file_id);
        ",
    )?;
    Ok(())
}

pub fn upsert_file(
    conn: &Connection,
    path: &str,
    name: &str,
    ext: Option<&str>,
    size: Option<i64>,
    modified: Option<i64>,
) -> Result<i64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO files (path, name, extension, size, modified, indexed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(path) DO UPDATE SET
           name=excluded.name, extension=excluded.extension,
           size=excluded.size, modified=excluded.modified,
           indexed_at=excluded.indexed_at",
        params![path, name, ext, size, modified, now],
    )?;

    let id: i64 = conn.query_row(
        "SELECT id FROM files WHERE path = ?1",
        params![path],
        |r| r.get(0),
    )?;
    Ok(id)
}

pub fn insert_tags(conn: &Connection, file_id: i64, tags: &[String], source: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM tags WHERE file_id=?1 AND source=?2",
        params![file_id, source],
    )?;
    let mut stmt = conn.prepare(
        "INSERT OR IGNORE INTO tags (file_id, tag, source) VALUES (?1, ?2, ?3)",
    )?;
    for tag in tags {
        let tag = tag.trim().to_lowercase();
        if !tag.is_empty() {
            stmt.execute(params![file_id, tag, source])?;
        }
    }
    Ok(())
}

#[derive(serde::Serialize, Debug)]
pub struct FileResult {
    pub id:        i64,
    pub path:      String,
    pub name:      String,
    pub extension: Option<String>,
    pub size:      Option<i64>,
    pub tags:      Vec<String>,
}

pub fn search_files(conn: &Connection, query: &str) -> Result<Vec<FileResult>> {
    let pattern = format!("%{}%", query.to_lowercase());
    let mut stmt = conn.prepare(
        "SELECT DISTINCT f.id, f.path, f.name, f.extension, f.size
         FROM files f
         LEFT JOIN tags t ON t.file_id = f.id
         WHERE LOWER(f.name) LIKE ?1
            OR LOWER(f.path) LIKE ?1
            OR LOWER(t.tag)  LIKE ?1
         ORDER BY f.name
         LIMIT 200",
    )?;

    let rows = stmt.query_map(params![pattern], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;

    let mut results = Vec::new();
    for row in rows {
        let (id, path, name, ext, size) = row?;
        let tags = get_tags_for_file(conn, id)?;
        results.push(FileResult {
            id,
            path,
            name,
            extension: ext,
            size,
            tags,
        });
    }
    Ok(results)
}

pub fn get_tags_for_file(conn: &Connection, file_id: i64) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT tag FROM tags WHERE file_id = ?1 ORDER BY source DESC, tag")?;
    let tags: std::result::Result<Vec<String>, _> =
        stmt.query_map(params![file_id], |r| r.get(0))?.collect();
    Ok(tags?)
}

pub fn get_tags_for_path(conn: &Connection, path: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.tag FROM tags t
         JOIN files f ON f.id = t.file_id
         WHERE f.path = ?1
         ORDER BY t.source DESC, t.tag",
    )?;
    let tags: std::result::Result<Vec<String>, _> =
        stmt.query_map(params![path], |r| r.get(0))?.collect();
    Ok(tags?)
}

pub fn delete_file_by_path(conn: &Connection, path: &str) -> Result<()> {
    conn.execute("DELETE FROM files WHERE path = ?1", params![path])?;
    // On macOS /Users is a symlink to /private/Users; FSEvents may report either form.
    if conn.changes() == 0 {
        let alt = if let Some(stripped) = path.strip_prefix("/private") {
            stripped.to_string()
        } else {
            format!("/private{path}")
        };
        conn.execute("DELETE FROM files WHERE path = ?1", params![alt])?;
    }
    Ok(())
}

pub fn get_indexed_dirs(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT path FROM indexed_dirs ORDER BY path")?;
    let rows: std::result::Result<Vec<String>, _> =
        stmt.query_map([], |r| r.get(0))?.collect();
    Ok(rows?)
}

pub fn clear_all(conn: &Connection) -> Result<()> {
    conn.execute_batch("DELETE FROM tags; DELETE FROM files; DELETE FROM indexed_dirs;")?;
    Ok(())
}
