use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{mpsc::RecvTimeoutError, Arc, Mutex},
    time::{Duration, Instant},
};

use notify::{
    event::{ModifyKind, RenameMode},
    EventKind, RecursiveMode, Watcher,
};
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

/// Keeps the notify watcher alive and exposes `add` for new directories.
pub struct WatcherHandle(pub Mutex<notify::RecommendedWatcher>);

impl WatcherHandle {
    pub fn add(&self, path: &Path) {
        if let Ok(mut w) = self.0.lock() {
            let _ = w.watch(path, RecursiveMode::Recursive);
        }
    }
}

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];

fn read_preview(path: &Path) -> Option<String> {
    use std::io::Read;
    const TEXT_EXTS: &[&str] = &[
        "txt", "md", "rs", "js", "ts", "py", "html", "css", "json", "toml",
        "yaml", "yml", "xml", "csv", "log", "sh", "bat", "cfg", "ini", "conf",
        "c", "cpp", "h", "java", "rb", "go", "kt", "swift",
    ];
    let ext = path.extension()?.to_str()?.to_lowercase();
    if !TEXT_EXTS.contains(&ext.as_str()) { return None; }
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; 2000];
    let n = f.read(&mut buf).ok()?;
    String::from_utf8(buf[..n].to_vec()).ok()
}

/// Upsert a single file into the DB and apply auto + AI tags.
async fn index_file(db_path: PathBuf, client: reqwest::Client, path: PathBuf) {
    let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned)
    else { return };

    let ext      = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase());
    let ext_str  = ext.as_deref().unwrap_or("");
    let meta     = std::fs::metadata(&path).ok();
    let size     = meta.as_ref().map(|m| m.len() as i64);
    let modified = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    // AI tags first (async) so we open the DB just once below.
    let ai_tags: Option<Vec<String>> = if IMAGE_EXTS.contains(&ext_str) {
        match std::fs::read(&path) {
            Ok(bytes) => ollama::get_image_tags(&client, &name, &bytes).await.ok(),
            Err(_)    => None,
        }
    } else {
        let preview = read_preview(&path);
        ollama::get_tags(&client, &name, ext_str, preview.as_deref()).await.ok()
    };

    let path_str = path.to_string_lossy().to_string();
    let _ = tokio::task::spawn_blocking(move || -> crate::error::Result<()> {
        let conn = db::open_connection(&db_path)?;
        let id   = db::upsert_file(&conn, &path_str, &name, ext.as_deref(), size, modified)?;
        if let Some(ref e) = ext {
            let _ = db::insert_tags(&conn, id, &[e.clone()], "auto");
        }
        if let Some(tags) = ai_tags.filter(|t| !t.is_empty()) {
            let _ = db::insert_tags(&conn, id, &tags, "ai");
        }
        Ok(())
    })
    .await;
}

fn remove_path(db_path: &PathBuf, pending: &mut HashMap<PathBuf, Instant>, path: PathBuf) {
    pending.remove(&path);
    let db       = db_path.clone();
    let path_str = path.to_string_lossy().to_string();
    tauri::async_runtime::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || -> crate::error::Result<()> {
            let conn = db::open_connection(&db)?;
            db::delete_file_by_path(&conn, &path_str)
        })
        .await;
    });
}

fn dispatch_event(
    event:   notify::Event,
    db_path: &PathBuf,
    pending: &mut HashMap<PathBuf, Instant>,
) {
    match event.kind {
        // Content created or changed
        EventKind::Create(_) | EventKind::Modify(ModifyKind::Data(_)) => {
            for path in event.paths.into_iter().filter(|p| p.is_file()) {
                pending.insert(path, Instant::now());
            }
        }
        // File deleted
        EventKind::Remove(_) => {
            for path in event.paths {
                remove_path(db_path, pending, path);
            }
        }
        // Rename / move
        EventKind::Modify(ModifyKind::Name(mode)) => {
            match mode {
                // Old location: delete from DB.
                RenameMode::From => {
                    for path in event.paths {
                        remove_path(db_path, pending, path);
                    }
                }
                // New location: queue for indexing.
                RenameMode::To => {
                    for path in event.paths.into_iter().filter(|p| p.is_file()) {
                        pending.insert(path, Instant::now());
                    }
                }
                // Both paths in one event 
                // paths[0] = old, paths[1] = new.
                RenameMode::Both => {
                    if let Some(old) = event.paths.first().cloned() {
                        remove_path(db_path, pending, old);
                    }
                    if let Some(new) = event.paths.into_iter().nth(1).filter(|p| p.is_file()) {
                        pending.insert(new, Instant::now());
                    }
                }
                // Direction unknown
                RenameMode::Any => {
                    for path in event.paths {
                        if path.is_file() {
                            pending.insert(path, Instant::now());
                        } else {
                            remove_path(db_path, pending, path);
                        }
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// Watch all indexed directories for new/modified files and re-index them automatically.
pub fn start_watcher(db_path: PathBuf, client: reqwest::Client) -> WatcherHandle {
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();

    let mut watcher = notify::recommended_watcher(move |res| { let _ = tx.send(res); })
        .expect("file watcher");

    if let Ok(conn) = db::open_connection(&db_path) {
        for dir in db::get_indexed_dirs(&conn).unwrap_or_default() {
            let _ = watcher.watch(Path::new(&dir), RecursiveMode::Recursive);
        }
    }

    std::thread::spawn(move || {
        let debounce = Duration::from_secs(2);
        let mut pending: HashMap<PathBuf, Instant> = HashMap::new();

        loop {
            // Block up to 500ms for the next event, exit cleanly on channel close.
            match rx.recv_timeout(Duration::from_millis(500)) {
                Err(RecvTimeoutError::Disconnected) => break,
                Ok(Ok(event)) => dispatch_event(event, &db_path, &mut pending),
                _ => {}
            }
            // Drain any remaining events queued since the blocking recv.
            while let Ok(Ok(event)) = rx.try_recv() {
                dispatch_event(event, &db_path, &mut pending);
            }
            // Fire any path that has been stable (no new events) for >= 2s.
            let now = Instant::now();
            pending.retain(|path, last_seen| {
                if now.duration_since(*last_seen) < debounce { return true; }
                let (db, c, p) = (db_path.clone(), client.clone(), path.clone());
                tauri::async_runtime::spawn(async move { index_file(db, c, p).await; });
                false
            });
        }
    });

    WatcherHandle(Mutex::new(watcher))
}

pub async fn run_indexing(
    app:         AppHandle,
    db_path:     PathBuf,
    folder:      PathBuf,
    status:      SharedStatus,
    http_client: reqwest::Client,
) {
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
        s.total        = total;
        s.indexed      = 0;
        s.is_running   = true;
        s.current_file = format!("Found {} files…", total);
    }
    let _ = app.emit("index-status", status.lock().unwrap().clone());

    let db = db_path.clone();
    let folder_str = folder.to_string_lossy().to_string();
    let _ = tokio::task::spawn_blocking(move || -> crate::error::Result<()> {
        let conn = db::open_connection(&db)?;
        conn.execute(
            "INSERT OR IGNORE INTO indexed_dirs(path) VALUES(?1)",
            rusqlite::params![folder_str],
        )?;
        Ok(())
    })
    .await;

    for (i, path) in entries.iter().enumerate() {
        {
            let mut s = status.lock().unwrap();
            s.indexed      = i;
            s.current_file = path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        let _ = app.emit("index-status", status.lock().unwrap().clone());
        index_file(db_path.clone(), http_client.clone(), path.clone()).await;
    }

    {
        let mut s = status.lock().unwrap();
        s.indexed      = total;
        s.is_running   = false;
        s.current_file = "Indexing complete".to_string();
    }
    let _ = app.emit("index-status", status.lock().unwrap().clone());
}
