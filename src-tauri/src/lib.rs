mod commands;
mod db;
mod error;
mod indexer;
mod ollama;

use std::sync::Mutex;

use tauri::Manager;

use commands::{DbPath, HttpClient};
use indexer::SharedStatus;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Resolve writable data dir — %APPDATA%\com.fff.proto on Windows
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("could not resolve app data dir");
            std::fs::create_dir_all(&data_dir)?;

            let db_path = data_dir.join("fff.db");

            // Run migrations on startup
            {
                let conn = db::open_connection(&db_path)?;
                db::run_migrations(&conn)?;
            }

            app.manage(DbPath(db_path));
            app.manage(HttpClient(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(60))
                    .build()
                    .expect("failed to build HTTP client"),
            ));
            app.manage(SharedStatus::new(Mutex::new(
                indexer::IndexStatus::default(),
            )));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::browse_directory,
            commands::search_files,
            commands::index_directory,
            commands::get_indexed_dirs,
            commands::delete_indexed_dir,
            commands::open_file,
            commands::reveal_in_file_manager,
            commands::get_index_status,
            commands::clear_index,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
