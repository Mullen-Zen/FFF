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

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = setup();
        run_migrations(&conn).unwrap();
    }

    #[test]
    fn upsert_file_returns_positive_id() {
        let conn = setup();
        let id = upsert_file(&conn, "/tmp/foo.txt", "foo.txt", Some("txt"), Some(100), None).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn upsert_file_is_idempotent() {
        let conn = setup();
        let id1 = upsert_file(&conn, "/tmp/foo.txt", "foo.txt", Some("txt"), Some(100), None).unwrap();
        let id2 = upsert_file(&conn, "/tmp/foo.txt", "foo.txt", Some("txt"), Some(200), None).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn insert_tags_trims_and_lowercases() {
        let conn = setup();
        let id = upsert_file(&conn, "/tmp/cat.jpg", "cat.jpg", Some("jpg"), None, None).unwrap();
        insert_tags(&conn, id, &["  Cat  ".to_string(), "Animal".to_string()], "ai").unwrap();
        let tags = get_tags_for_file(&conn, id).unwrap();
        assert!(tags.contains(&"cat".to_string()));
        assert!(tags.contains(&"animal".to_string()));
    }

    #[test]
    fn insert_tags_skips_empty_strings() {
        let conn = setup();
        let id = upsert_file(&conn, "/tmp/x.txt", "x.txt", None, None, None).unwrap();
        insert_tags(&conn, id, &["".to_string(), "  ".to_string()], "auto").unwrap();
        assert!(get_tags_for_file(&conn, id).unwrap().is_empty());
    }

    #[test]
    fn search_finds_by_name() {
        let conn = setup();
        upsert_file(&conn, "/tmp/report.pdf", "report.pdf", Some("pdf"), None, None).unwrap();
        let results = search_files(&conn, "report").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "report.pdf");
    }

    #[test]
    fn search_finds_by_tag() {
        let conn = setup();
        let id = upsert_file(&conn, "/tmp/cat.jpg", "cat.jpg", Some("jpg"), None, None).unwrap();
        insert_tags(&conn, id, &["feline".to_string()], "ai").unwrap();
        let results = search_files(&conn, "feline").unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].tags.contains(&"feline".to_string()));
    }

    #[test]
    fn search_returns_empty_on_no_match() {
        let conn = setup();
        upsert_file(&conn, "/tmp/report.pdf", "report.pdf", Some("pdf"), None, None).unwrap();
        assert!(search_files(&conn, "zzznomatch").unwrap().is_empty());
    }

    #[test]
    fn delete_removes_file() {
        let conn = setup();
        upsert_file(&conn, "/tmp/gone.txt", "gone.txt", Some("txt"), None, None).unwrap();
        delete_file_by_path(&conn, "/tmp/gone.txt").unwrap();
        assert!(search_files(&conn, "gone").unwrap().is_empty());
    }

    #[test]
    fn delete_falls_back_to_private_prefix() {
        let conn = setup();
        // File stored without the /private prefix (canonical path)
        upsert_file(&conn, "/Users/test/file.txt", "file.txt", Some("txt"), None, None).unwrap();
        // FSEvents on macOS may report the /private-prefixed form
        delete_file_by_path(&conn, "/private/Users/test/file.txt").unwrap();
        assert!(search_files(&conn, "file").unwrap().is_empty());
    }

    #[test]
    fn clear_all_removes_files_and_tags() {
        let conn = setup();
        let id = upsert_file(&conn, "/tmp/file.txt", "file.txt", Some("txt"), None, None).unwrap();
        insert_tags(&conn, id, &["tag".to_string()], "auto").unwrap();
        clear_all(&conn).unwrap();
        assert!(search_files(&conn, "file").unwrap().is_empty());
    }
}
