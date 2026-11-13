mod db;
mod import;
mod library;
mod player;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager, State};

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

#[tauri::command]
fn play_media(db: State<db::Db>, player: State<player::Player>, id: i64) -> Result<library::MediaRow, String> {
    let conn = db.0.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
    let (mut musica, path) = library::obter(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or("Música não encontrada na biblioteca.")?;
    let existe = std::path::Path::new(&path).is_file();
    if existe == musica.missing {
        library::marcar_ausente(&conn, id, !existe).map_err(|e| e.to_string())?;
        musica.missing = !existe;
    }
    if !existe {
        return Err(format!("Arquivo não encontrado: {path}"));
    }
    drop(conn);
    player.tocar(id, &path)?;
    Ok(musica)
}

#[tauri::command]
fn toggle_pause(player: State<player::Player>) -> Result<(), String> {
    player.alternar_pausa()
}

#[tauri::command]
fn set_volume(player: State<player::Player>, volume: f64) {
    player.volume(volume);
}

#[tauri::command]
fn seek(player: State<player::Player>, posicao_ms: u64) -> Result<(), String> {
    player.buscar(posicao_ms)
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
            let handle = app.handle().clone();
            app.manage(player::Player::new(move |estado| {
                let _ = handle.emit("player-estado", estado);
            })?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_status,
            import_folder,
            list_media,
            play_media,
            toggle_pause,
            set_volume,
            seek
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o Skáld");
}
