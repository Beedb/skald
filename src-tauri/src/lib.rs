mod db;
mod import;
mod library;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{Manager, State};

#[derive(Serialize)]
struct DbStatus {
    path: String,
    schema_version: i64,
    media_count: i64,
}

#[tauri::command]
fn db_status(app: tauri::AppHandle, db: State<db::Db>) -> Result<DbStatus, String> {
    let conn = db.0.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
    let path = app
        .path()
        .app_data_dir()
        .map(|d| d.join("skald.db").display().to_string())
        .unwrap_or_default();
    Ok(DbStatus {
        path,
        schema_version: db::schema_version(&conn).map_err(|e| e.to_string())?,
        media_count: db::media_count(&conn).map_err(|e| e.to_string())?,
    })
}

#[tauri::command]
async fn import_folder(app: tauri::AppHandle, path: String) -> Result<import::ImportResumo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<db::Db>();
        import::importar(&db.0, std::path::Path::new(&path))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn list_media(db: State<db::Db>, offset: u32, limit: u32) -> Result<Vec<library::MediaRow>, String> {
    let conn = db.0.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
    library::list_media(&conn, offset, limit.min(2000)).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("skald.db"))?;
            app.manage(db::Db(Mutex::new(conn)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![db_status, import_folder, list_media])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o Skáld");
}
