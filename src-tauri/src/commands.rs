use std::path::PathBuf;

use tauri::{AppHandle, State};

use crate::{
    db::{self, FileResult},
    error::AppError,
    indexer::{IndexStatus, SharedStatus},
};

// ---- Managed-state wrappers ----

pub struct DbPath(pub PathBuf);
pub struct HttpClient(pub reqwest::Client);

// ---- browse_directory ----

#[derive(serde::Serialize, Debug)]
pub struct DirEntry {
    pub name:     String,
    pub path:     String,
    pub is_dir:   bool,
    pub size:     Option<u64>,
    pub modified: Option<u64>,
}

#[tauri::command]
pub async fn browse_directory(path: String) -> Result<Vec<DirEntry>, AppError> {
    let base = PathBuf::from(&path);
    let mut entries = Vec::new();

    for entry in std::fs::read_dir(&base)
        .map_err(AppError::Io)?
        .flatten()
    {
        let p = entry.path();
        let meta = entry.metadata().ok();
        entries.push(DirEntry {
            name:     entry.file_name().to_string_lossy().to_string(),
            path:     p.to_string_lossy().to_string(),
            is_dir:   p.is_dir(),
            size:     meta.as_ref().map(|m| m.len()),
            modified: meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
        });
    }

    // Directories first, then alphabetical
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

// ---- search_files ----

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

// ---- index_directory ----

#[tauri::command]
pub async fn index_directory(
    app:         AppHandle,
    path:        String,
    db_path:     State<'_, DbPath>,
    http_client: State<'_, HttpClient>,
    status:      State<'_, SharedStatus>,
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

    let db_path_clone  = db_path.0.clone();
    let client_clone   = http_client.0.clone();
    let status_clone   = status.inner().clone();

    tauri::async_runtime::spawn(async move {
        crate::indexer::run_indexing(app, db_path_clone, folder, status_clone, client_clone).await;
    });

    Ok(())
}

// ---- get_indexed_dirs ----

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

// ---- open_file ----

#[tauri::command]
pub async fn open_file(path: String) -> Result<(), AppError> {
    tauri_plugin_opener::open_path(path, None::<String>)
        .map_err(|e| AppError::Other(e.to_string()))
}

// ---- reveal_in_file_manager ----

#[tauri::command]
pub async fn reveal_in_file_manager(path: String) -> Result<(), AppError> {
    tauri_plugin_opener::reveal_item_in_dir(path)
        .map_err(|e| AppError::Other(e.to_string()))
}

// ---- clear_index ----

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

// ---- get_index_status ----

#[tauri::command]
pub async fn get_index_status(
    status: State<'_, SharedStatus>,
) -> Result<IndexStatus, AppError> {
    Ok(status.inner().lock().unwrap().clone())
}


#[tauri::command]
pub async fn delete_indexed_dir(
    path: String,
    db_path: State<'_, DbPath>,
) -> Result<(), AppError> {
    let db_file = db_path.0.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db::open_connection(&db_file)?;
        db::delete_indexed_dir(&conn, &path)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
