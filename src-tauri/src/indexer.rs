use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

use crate::{db, ollama};

#[derive(Default, Clone, serde::Serialize)]
pub struct IndexStatus {
    pub total:        usize,
    pub indexed:      usize,
    pub current_file: String,
    pub is_running:   bool,
}

pub type SharedStatus = Arc<Mutex<IndexStatus>>;

/// Returns text content preview for supported text file types.
fn read_preview(path: &PathBuf) -> Option<String> {
    use std::io::Read;
    const TEXT_EXTS: &[&str] = &[
        "txt", "md", "rs", "js", "ts", "py", "html", "css", "json", "toml",
        "yaml", "yml", "xml", "csv", "log", "sh", "bat", "cfg", "ini", "conf",
        "c", "cpp", "h", "java", "rb", "go", "kt", "swift",
    ];
    let ext = path.extension()?.to_str()?.to_lowercase();
    if !TEXT_EXTS.contains(&ext.as_str()) {
        return None;
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; 2000];
    let n = f.read(&mut buf).ok()?;
    String::from_utf8(buf[..n].to_vec()).ok()
}

pub async fn run_indexing(
    app: AppHandle,
    db_path: PathBuf,
    folder: PathBuf,
    status: SharedStatus,
    http_client: reqwest::Client,
) {
    // Phase 1: collect all file paths
    let entries: Vec<PathBuf> = WalkDir::new(&folder)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();

    let total = entries.len();
    {
        let mut s = status.lock().unwrap();
        s.total = total;
        s.indexed = 0;
        s.is_running = true;
        s.current_file = format!("Found {} files, starting indexing…", total);
    }
    let _ = app.emit("index-status", status.lock().unwrap().clone());

    // Phase 2: open a single DB connection for the duration of this task
    let conn = match db::open_connection(&db_path) {
        Ok(c) => c,
        Err(e) => {
            let mut s = status.lock().unwrap();
            s.is_running = false;
            s.current_file = format!("DB error: {}", e);
            let _ = app.emit("index-status", s.clone());
            return;
        }
    };

    // Register this folder in indexed_dirs
    let folder_str = folder.to_string_lossy().to_string();
    let _ = conn.execute(
        "INSERT OR IGNORE INTO indexed_dirs(path) VALUES(?1)",
        rusqlite::params![folder_str],
    );

    // Phase 3: walk and index each file
    for (i, path) in entries.iter().enumerate() {
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        let meta = std::fs::metadata(path).ok();
        let size = meta.as_ref().map(|m| m.len() as i64);
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        {
            let mut s = status.lock().unwrap();
            s.indexed = i;
            s.current_file = name.clone();
        }
        let _ = app.emit("index-status", status.lock().unwrap().clone());

        let path_str = path.to_string_lossy().to_string();
        let file_id = match db::upsert_file(&conn, &path_str, &name, ext.as_deref(), size, modified)
        {
            Ok(id) => id,
            Err(_) => continue,
        };

        // Auto-tag by extension
        if let Some(ref e) = ext {
            let _ = db::insert_tags(&conn, file_id, &[e.clone()], "auto");
        }

        // AI tag via Ollama (silently skip on failure)
        let preview = read_preview(path);
        if let Ok(tags) = ollama::get_tags(
            &http_client,
            &name,
            ext.as_deref().unwrap_or(""),
            preview.as_deref(),
        )
        .await
        {
            if !tags.is_empty() {
                let _ = db::insert_tags(&conn, file_id, &tags, "ai");
            }
        }
    }

    // Done
    {
        let mut s = status.lock().unwrap();
        s.indexed = total;
        s.is_running = false;
        s.current_file = "Indexing complete".to_string();
    }
    let _ = app.emit("index-status", status.lock().unwrap().clone());
}
