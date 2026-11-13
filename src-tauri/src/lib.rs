mod db;
mod details;
mod import;
mod library;
mod player;
mod playlists;
mod queue;
mod search;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

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
fn list_media(
    db: State<db::Db>,
    offset: u32,
    limit: u32,
    busca: Option<String>,
) -> Result<Vec<library::MediaRow>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    library::list_media(&conn, offset, limit.min(2000), busca.as_deref().unwrap_or("")).map_err(|e| e.to_string())
}

/// Total de músicas que a busca encontra (sem busca: a biblioteca toda).
#[tauri::command]
fn count_media(db: State<db::Db>, busca: Option<String>) -> Result<i64, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    library::contar(&conn, busca.as_deref().unwrap_or("")).map_err(|e| e.to_string())
}

struct FilaState(Mutex<queue::Fila>);

const EVENTO_FILA: &str = "fila-mudou";

fn erro_db<T>(_: T) -> String {
    "Banco de dados indisponível.".to_string()
}

/// Toca `id` no player. Arquivo ausente é marcado como não encontrado (registro mantido).
fn tocar_id(app: &AppHandle, id: i64) -> Result<(), String> {
    let path = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        let (musica, path) = library::obter(&conn, id)
            .map_err(|e| e.to_string())?
            .ok_or("Música não encontrada na biblioteca.")?;
        let existe = std::path::Path::new(&path).is_file();
        if existe == musica.missing {
            library::marcar_ausente(&conn, id, !existe).map_err(|e| e.to_string())?;
        }
        if !existe {
            return Err(format!("Arquivo não encontrado: {path}"));
        }
        path
    };
    app.state::<player::Player>().tocar(id, &path)
}

/// Toca a próxima da fila, pulando arquivos ausentes. Sem próxima, para.
fn avancar(app: &AppHandle) -> Result<(), String> {
    let fila = app.state::<FilaState>();
    let mut resultado = Ok(());
    for _ in 0..100 {
        let Some(id) = fila.0.lock().map_err(erro_db)?.proxima() else { break };
        resultado = tocar_id(app, id);
        if resultado.is_ok() {
            break;
        }
    }
    let _ = app.emit(EVENTO_FILA, ());
    resultado
}

/// Toca `id` e usa como contexto da fila a lista que estava na tela (a busca, se houver).
#[tauri::command]
fn play_media(app: AppHandle, fila: State<FilaState>, id: i64, busca: Option<String>) -> Result<(), String> {
    let ids = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        library::ids_em_ordem(&conn, busca.as_deref().unwrap_or("")).map_err(|e| e.to_string())?
    };
    fila.0.lock().map_err(erro_db)?.tocar_do_contexto(ids, id);
    let _ = app.emit(EVENTO_FILA, ());
    tocar_id(&app, id)
}

const EVENTO_PLAYLISTS: &str = "playlists-mudou";

/// Roda `f` com o banco travado e avisa a interface que as playlists mudaram.
fn alterar_playlists<T>(app: &AppHandle, f: impl FnOnce(&mut rusqlite::Connection) -> Result<T, String>) -> Result<T, String> {
    let resultado = {
        let db = app.state::<db::Db>();
        let mut conn = db.0.lock().map_err(erro_db)?;
        f(&mut conn)?
    };
    let _ = app.emit(EVENTO_PLAYLISTS, ());
    Ok(resultado)
}

#[tauri::command]
fn playlists_list(db: State<db::Db>) -> Result<Vec<playlists::Resumo>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    playlists::listar(&conn)
}

/// Cria a playlist, opcionalmente já com músicas. Devolve o id.
#[tauri::command]
fn playlist_create(app: AppHandle, name: String, media_ids: Option<Vec<i64>>) -> Result<i64, String> {
    alterar_playlists(&app, |conn| {
        let id = playlists::criar(conn, &name)?;
        if let Some(ids) = media_ids {
            playlists::adicionar(conn, id, &ids)?;
        }
        Ok(id)
    })
}

#[tauri::command]
fn playlist_rename(app: AppHandle, id: i64, name: String) -> Result<(), String> {
    alterar_playlists(&app, |conn| playlists::renomear(conn, id, &name))
}

#[tauri::command]
fn playlist_delete(app: AppHandle, id: i64) -> Result<(), String> {
    let capa = alterar_playlists(&app, |conn| {
        let capa = playlists::capa(conn, id)?;
        playlists::excluir(conn, id)?;
        Ok(capa)
    })?;
    apagar_capa_copiada(&app, capa.as_deref());
    Ok(())
}

/// Pasta onde o Skáld guarda cópias das capas escolhidas para playlists.
fn pasta_capas(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("capas"))
}

/// Apaga uma capa copiada pelo próprio Skáld. Nunca toca em arquivos fora da pasta de capas.
fn apagar_capa_copiada(app: &AppHandle, caminho: Option<&str>) {
    let (Some(caminho), Ok(pasta)) = (caminho, pasta_capas(app)) else { return };
    let caminho = std::path::Path::new(caminho);
    if caminho.parent() == Some(pasta.as_path()) {
        let _ = std::fs::remove_file(caminho);
    }
}

#[tauri::command]
fn toggle_like(app: AppHandle, media_id: i64) -> Result<bool, String> {
    alterar_playlists(&app, |conn| playlists::alternar_curtida(conn, media_id))
}

#[tauri::command]
fn liked_ids(db: State<db::Db>) -> Result<Vec<i64>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    playlists::ids_curtidos(&conn)
}

/// Copia a imagem escolhida para a pasta de capas do Skáld (o original não é alterado).
#[tauri::command]
fn playlist_set_cover(app: AppHandle, id: i64, origem: String) -> Result<(), String> {
    let origem = std::path::Path::new(&origem);
    let ext = details::mime_imagem(origem)
        .and(origem.extension())
        .ok_or("Escolha uma imagem JPG, PNG ou WEBP.")?
        .to_string_lossy()
        .to_ascii_lowercase();
    let tamanho = std::fs::metadata(origem).map_err(|_| "Imagem não encontrada.")?.len();
    if tamanho as usize > details::CAPA_MAX {
        return Err("A imagem é grande demais (máximo de 8 MB).".into());
    }
    let pasta = pasta_capas(&app)?;
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let destino = pasta.join(format!("playlist-{id}-{agora}.{ext}"));
    std::fs::copy(origem, &destino).map_err(|e| format!("Não foi possível copiar a imagem: {e}"))?;
    let destino_txt = destino.to_string_lossy().into_owned();
    match alterar_playlists(&app, |conn| playlists::definir_capa(conn, id, Some(&destino_txt))) {
        Ok(anterior) => {
            apagar_capa_copiada(&app, anterior.as_deref());
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&destino);
            Err(e)
        }
    }
}

/// Volta à capa automática (a da primeira música).
#[tauri::command]
fn playlist_clear_cover(app: AppHandle, id: i64) -> Result<(), String> {
    let anterior = alterar_playlists(&app, |conn| playlists::definir_capa(conn, id, None))?;
    apagar_capa_copiada(&app, anterior.as_deref());
    Ok(())
}

/// Capa escolhida como data URL (`None` se a playlist usa a capa automática).
#[tauri::command]
fn playlist_cover(db: State<db::Db>, id: i64) -> Result<Option<String>, String> {
    let caminho = {
        let conn = db.0.lock().map_err(erro_db)?;
        playlists::capa(&conn, id)?
    };
    Ok(caminho.and_then(|c| details::imagem_data_url(std::path::Path::new(&c))))
}

/// Exporta em M3U (ordem personalizada, caminhos absolutos). Devolve quantas músicas foram gravadas.
#[tauri::command]
fn playlist_export_m3u(db: State<db::Db>, id: i64, destino: String) -> Result<usize, String> {
    let (texto, n) = {
        let conn = db.0.lock().map_err(erro_db)?;
        playlists::m3u(&conn, id)?
    };
    std::fs::write(&destino, texto).map_err(|e| format!("Não foi possível salvar o arquivo: {e}"))?;
    Ok(n)
}

#[tauri::command]
fn playlist_add(app: AppHandle, id: i64, media_ids: Vec<i64>) -> Result<(), String> {
    alterar_playlists(&app, |conn| playlists::adicionar(conn, id, &media_ids))
}

#[tauri::command]
fn playlist_items(db: State<db::Db>, id: i64, ordem: playlists::Ordem) -> Result<Vec<playlists::Item>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    playlists::itens(&conn, id, ordem)
}

#[tauri::command]
fn playlist_remove_item(app: AppHandle, item_id: i64) -> Result<(), String> {
    alterar_playlists(&app, |conn| playlists::remover_item(conn, item_id))
}

#[tauri::command]
fn playlist_move_item(app: AppHandle, item_id: i64, posicao: usize) -> Result<(), String> {
    alterar_playlists(&app, |conn| playlists::mover_item(conn, item_id, posicao))
}

/// Toca a música na posição `indice` da playlist (na ordem mostrada); a playlist vira o contexto da fila.
#[tauri::command]
fn play_playlist(app: AppHandle, fila: State<FilaState>, id: i64, ordem: playlists::Ordem, indice: usize) -> Result<(), String> {
    let ids: Vec<i64> = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        playlists::itens(&conn, id, ordem)?.into_iter().map(|i| i.media.id).collect()
    };
    let musica = fila
        .0
        .lock()
        .map_err(erro_db)?
        .tocar_do_contexto_em(ids, indice)
        .ok_or("A playlist está vazia.")?;
    let _ = app.emit(EVENTO_FILA, ());
    tocar_id(&app, musica)
}

#[tauri::command]
fn next_track(app: AppHandle) -> Result<(), String> {
    avancar(&app)
}

/// Depois de 3 s volta ao início da faixa; antes disso, vai para a anterior.
#[tauri::command]
fn previous_track(app: AppHandle, fila: State<FilaState>, player: State<player::Player>) -> Result<(), String> {
    if player.posicao_ms() > 3000 {
        return player.buscar(0);
    }
    let anterior = fila.0.lock().map_err(erro_db)?.anterior();
    let _ = app.emit(EVENTO_FILA, ());
    match anterior {
        Some(id) => tocar_id(&app, id),
        None => player.buscar(0),
    }
}

#[tauri::command]
fn queue_add(app: AppHandle, fila: State<FilaState>, id: i64, a_seguir: bool) -> Result<(), String> {
    let mut f = fila.0.lock().map_err(erro_db)?;
    if a_seguir { f.tocar_a_seguir(id) } else { f.adicionar(id) }
    let _ = app.emit(EVENTO_FILA, ());
    Ok(())
}

#[tauri::command]
fn queue_remove(app: AppHandle, fila: State<FilaState>, posicao: usize) -> Result<(), String> {
    fila.0.lock().map_err(erro_db)?.remover_manual(posicao);
    let _ = app.emit(EVENTO_FILA, ());
    Ok(())
}

#[derive(Serialize)]
struct VisaoFila {
    atual: Option<library::MediaRow>,
    manual: Vec<library::MediaRow>,
    contexto: Vec<library::MediaRow>,
}

#[tauri::command]
fn get_queue(db: State<db::Db>, fila: State<FilaState>) -> Result<VisaoFila, String> {
    let (atual, manual, contexto) = {
        let f = fila.0.lock().map_err(erro_db)?;
        (f.atual(), f.manual().collect::<Vec<_>>(), f.proximas_do_contexto(50).to_vec())
    };
    let conn = db.0.lock().map_err(erro_db)?;
    let buscar = |ids: &[i64]| library::obter_varios(&conn, ids).map_err(|e| e.to_string());
    Ok(VisaoFila {
        atual: buscar(&atual.into_iter().collect::<Vec<_>>())?.pop(),
        manual: buscar(&manual)?,
        contexto: buscar(&contexto)?,
    })
}

/// Capa e informações adicionais da música (lidas do arquivo, fora da thread principal).
#[tauri::command]
async fn get_track_details(app: AppHandle, id: i64) -> Result<Option<details::Detalhes>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let base = {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            details::do_banco(&conn, id)?
        };
        Ok(base.map(details::completar_do_arquivo))
    })
    .await
    .map_err(|e| e.to_string())?
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
            app.manage(FilaState(Mutex::new(queue::Fila::default())));
            let handle = app.handle().clone();
            app.manage(player::Player::new(move |estado| {
                let fim = estado.fim;
                let _ = handle.emit("player-estado", estado);
                if fim {
                    // Fora da thread do barramento, para não bloqueá-la.
                    let h = handle.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = avancar(&h) {
                            let _ = h.emit("player-erro", e);
                        }
                    });
                }
            })?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_status,
            import_folder,
            list_media,
            count_media,
            play_media,
            next_track,
            previous_track,
            queue_add,
            queue_remove,
            get_queue,
            get_track_details,
            playlists_list,
            playlist_create,
            playlist_rename,
            playlist_delete,
            playlist_add,
            playlist_items,
            playlist_remove_item,
            playlist_move_item,
            play_playlist,
            toggle_like,
            liked_ids,
            playlist_set_cover,
            playlist_clear_cover,
            playlist_cover,
            playlist_export_m3u,
            toggle_pause,
            set_volume,
            seek
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o Skáld");
}
