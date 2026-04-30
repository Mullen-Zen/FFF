use std::path::PathBuf;

use tauri::{AppHandle, State};

use crate::{
    db::{self, FileResult},
    error::AppError,
    indexer::{IndexStatus, SharedStatus, WatcherHandle},
};

// managed state wrappers

pub struct DbPath(pub PathBuf);
pub struct HttpClient(pub reqwest::Client);

// browse dir

#[derive(serde::Serialize, Debug)]
pub struct DirEntry {
    pub name:     String,
    pub path:     String,
    pub is_dir:   bool,
    pub size:     Option<u64>,
    pub modified: Option<u64>,
    pub tags:     Vec<String>,
}

#[tauri::command]
pub async fn browse_directory(
    path:    String,
    db_path: State<'_, DbPath>,
) -> Result<Vec<DirEntry>, AppError> {
    let base = PathBuf::from(&path);
    let db_path_clone = db_path.0.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db::open_connection(&db_path_clone)?;
        let mut entries = Vec::new();

        for entry in std::fs::read_dir(&base)
            .map_err(AppError::Io)?
            .flatten()
        {
            let p = entry.path();
            let meta = entry.metadata().ok();
            let is_dir = p.is_dir();
            let path_str = p.to_string_lossy().to_string();
            let tags = if !is_dir {
                db::get_tags_for_path(&conn, &path_str).unwrap_or_default()
            } else {
                vec![]
            };

            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: path_str,
                is_dir,
                size: meta.as_ref().map(|m| m.len()),
                modified: meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs()),
                tags,
            });
        }

        // Directories first, then alphabetical
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        Ok(entries)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

// search

#[tauri::command]
pub async fn search_files(
    query:    String,
    db_path:  State<'_, DbPath>,
) -> Result<Vec<FileResult>, AppError> {
    let path = db_path.0.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db::open_connection(&path)?;
        db::search_files(&conn, &query)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

// index new dir

#[tauri::command]
pub async fn index_directory(
    app:         AppHandle,
    path:        String,
    db_path:     State<'_, DbPath>,
    http_client: State<'_, HttpClient>,
    status:      State<'_, SharedStatus>,
    watcher:     State<'_, WatcherHandle>,
) -> Result<(), AppError> {
    {
        let s = status.inner().lock().unwrap();
        if s.is_running {
            return Err(AppError::Other("Indexing already in progress".into()));
        }
    }

    let folder = PathBuf::from(&path);
    if !folder.is_dir() {
        return Err(AppError::Other(format!("Not a directory: {}", path)));
    }

    // Register with the watcher so new files are picked up after initial indexing.
    watcher.add(&folder);

    let db_path_clone = db_path.0.clone();
    let client_clone  = http_client.0.clone();
    let status_clone  = status.inner().clone();

    tauri::async_runtime::spawn(async move {
        crate::indexer::run_indexing(app, db_path_clone, folder, status_clone, client_clone).await;
    });

    Ok(())
}

#[tauri::command]
pub async fn get_indexed_dirs(
    db_path: State<'_, DbPath>,
) -> Result<Vec<String>, AppError> {
    let path = db_path.0.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db::open_connection(&path)?;
        db::get_indexed_dirs(&conn)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
pub async fn open_file(path: String) -> Result<(), AppError> {
    tauri_plugin_opener::open_path(path, None::<String>)
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn reveal_in_file_manager(path: String) -> Result<(), AppError> {
    tauri_plugin_opener::reveal_item_in_dir(path)
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn clear_index(
    db_path: State<'_, DbPath>,
) -> Result<(), AppError> {
    let path = db_path.0.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db::open_connection(&path)?;
        db::clear_all(&conn)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
pub async fn get_index_status(
    status: State<'_, SharedStatus>,
) -> Result<IndexStatus, AppError> {
    Ok(status.inner().lock().unwrap().clone())
}
