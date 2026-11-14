mod db;
mod details;
mod history;
mod import;
mod library;
mod player;
mod playlists;
mod mpris;
mod queue;
mod watch;
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
        let pasta = std::path::Path::new(&path);
        let resumo = import::importar(&app.state::<db::Db>().0, pasta)?;
        {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            watch::adicionar(&conn, &path).map_err(|e| e.to_string())?;
        }
        app.state::<watch::Vigia>().vigiar(pasta);
        if resumo.adicionados > 0 {
            incluir_novas(&app, Origem::Biblioteca)?;
        }
        Ok(resumo)
    })
    .await
    .map_err(|e| e.to_string())?
}

const EVENTO_BIBLIOTECA: &str = "biblioteca-mudou";

/// Chamado pelo monitoramento de pastas: avisa a interface e põe as novas no contexto da fila.
pub(crate) fn biblioteca_mudou(app: &AppHandle, resumo: &import::ImportResumo) {
    if !resumo.mudou_algo() {
        return;
    }
    if resumo.adicionados > 0 {
        let _ = incluir_novas(app, Origem::Biblioteca);
    }
    let _ = app.emit(EVENTO_BIBLIOTECA, resumo);
}

#[tauri::command]
fn watched_folders(db: State<db::Db>) -> Result<Vec<String>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    watch::listar(&conn).map_err(|e| e.to_string())
}

/// Para de monitorar a pasta; as músicas dela continuam na biblioteca.
#[tauri::command]
fn unwatch_folder(app: AppHandle, path: String) -> Result<(), String> {
    {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        watch::remover(&conn, &path).map_err(|e| e.to_string())?;
    }
    app.state::<watch::Vigia>().parar(std::path::Path::new(&path));
    Ok(())
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

/// De onde veio o contexto da fila, para incluir nele músicas novas (Shuffle Bag).
#[derive(Clone, Copy, PartialEq)]
enum Origem {
    Biblioteca,
    Playlist(i64, playlists::Ordem),
}

struct OrigemState(Mutex<Option<Origem>>);

/// Acrescenta ao contexto da fila as músicas novas de `origem`, se ela for a origem atual.
fn incluir_novas(app: &AppHandle, origem: Origem) -> Result<(), String> {
    if *app.state::<OrigemState>().0.lock().map_err(erro_db)? != Some(origem) {
        return Ok(());
    }
    let ids = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        match origem {
            Origem::Biblioteca => library::ids_em_ordem(&conn, "").map_err(|e| e.to_string())?,
            Origem::Playlist(id, ordem) => playlists::itens(&conn, id, ordem)?.into_iter().map(|i| i.media.id).collect(),
        }
    };
    app.state::<FilaState>().0.lock().map_err(erro_db)?.incluir(&ids);
    let _ = app.emit(EVENTO_FILA, ());
    Ok(())
}

fn definir_origem(app: &AppHandle, origem: Option<Origem>) -> Result<(), String> {
    *app.state::<OrigemState>().0.lock().map_err(erro_db)? = origem;
    Ok(())
}

const EVENTO_FILA: &str = "fila-mudou";

fn erro_db<T>(_: T) -> String {
    "Banco de dados indisponível.".to_string()
}

/// Sessão da faixa em reprodução, para o histórico (seção 11).
struct HistoricoState(Mutex<Option<history::Sessao>>);

const EVENTO_HISTORICO: &str = "historico-mudou";

/// Relógio monotônico em ms (não sofre com mudanças de horário do sistema).
fn agora_ms() -> u64 {
    static INICIO: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    INICIO.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

fn unix_agora() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

fn gravar_sessao(app: &AppHandle, s: &history::Sessao) {
    let db = app.state::<db::Db>();
    if let Ok(conn) = db.0.lock() {
        let _ = history::gravar(&conn, s);
    }
}

/// Fecha a sessão atual (se houver) e grava o tempo ouvido. Falhas aqui nunca impedem a reprodução.
fn encerrar_sessao(app: &AppHandle, concluida: bool) {
    let estado_hist = app.state::<HistoricoState>();
    let Ok(mut hist) = estado_hist.0.lock() else { return };
    if let Some(mut s) = hist.take() {
        s.aviso(false, agora_ms(), 0);
        s.concluida |= concluida;
        gravar_sessao(app, &s);
        let _ = app.emit(EVENTO_HISTORICO, ());
    }
}

fn iniciar_sessao(app: &AppHandle, media_id: i64) {
    let estado_hist = app.state::<HistoricoState>();
    let Ok(mut hist) = estado_hist.0.lock() else { return };
    let registro = {
        let db = app.state::<db::Db>();
        let Ok(conn) = db.0.lock() else { return };
        let duracao: Option<i64> = conn
            .query_row("SELECT duration_ms FROM media WHERE id = ?1", [media_id], |r| r.get(0))
            .unwrap_or(None);
        history::iniciar(&conn, media_id, unix_agora()).ok().map(|r| (r, duracao.unwrap_or(0).max(0) as u64))
    };
    if let Some((registro, duracao)) = registro {
        *hist = Some(history::Sessao::nova(registro, media_id, duracao, agora_ms()));
        let _ = app.emit(EVENTO_HISTORICO, ());
    }
}

/// Atualiza a sessão com cada aviso do player.
fn acompanhar_sessao(app: &AppHandle, estado: &player::EstadoPlayer) {
    if estado.fim || estado.erro.is_some() {
        return encerrar_sessao(app, estado.fim);
    }
    let estado_hist = app.state::<HistoricoState>();
    let Ok(mut hist) = estado_hist.0.lock() else { return };
    let Some(s) = hist.as_mut() else { return };
    // Avisos atrasados da faixa anterior não contam para a atual.
    if estado.id != Some(s.media_id) {
        return;
    }
    if s.aviso(estado.estado == player::Estado::Tocando, agora_ms(), estado.duracao_ms) {
        gravar_sessao(app, s);
    }
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
    encerrar_sessao(app, false);
    app.state::<player::Player>().tocar(id, &path)?;
    iniciar_sessao(app, id);
    Ok(())
}

/// Toca a próxima da fila, pulando arquivos ausentes. Sem próxima, para.
/// `automatica`: a faixa terminou sozinha (aí "repetir faixa" vale).
fn avancar(app: &AppHandle, automatica: bool) -> Result<(), String> {
    let fila = app.state::<FilaState>();
    let mut resultado = Ok(());
    for tentativa in 0..100 {
        let mut f = fila.0.lock().map_err(erro_db)?;
        let proxima = if automatica && tentativa == 0 { f.proxima_automatica() } else { f.proxima() };
        drop(f);
        let Some(id) = proxima else { break };
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
    let busca_vazia = busca.as_deref().unwrap_or("").trim().is_empty();
    definir_origem(&app, busca_vazia.then_some(Origem::Biblioteca))?;
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
    alterar_playlists(&app, |conn| playlists::adicionar(conn, id, &media_ids))?;
    let origem = *app.state::<OrigemState>().0.lock().map_err(erro_db)?;
    if let Some(o @ Origem::Playlist(tocando, _)) = origem {
        if tocando == id {
            incluir_novas(&app, o)?;
        }
    }
    Ok(())
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
    definir_origem(&app, Some(Origem::Playlist(id, ordem)))?;
    let _ = app.emit(EVENTO_FILA, ());
    tocar_id(&app, musica)
}

#[tauri::command]
fn history_list(db: State<db::Db>, offset: u32, limit: u32) -> Result<Vec<history::Entrada>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    history::listar(&conn, offset, limit.min(2000)).map_err(|e| e.to_string())
}

#[tauri::command]
fn history_count(db: State<db::Db>) -> Result<i64, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    history::contar(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn next_track(app: AppHandle) -> Result<(), String> {
    avancar(&app, false)
}

#[tauri::command]
fn set_shuffle(app: AppHandle, fila: State<FilaState>, ativo: bool) -> Result<(), String> {
    fila.0.lock().map_err(erro_db)?.definir_aleatorio(ativo);
    let _ = app.emit(EVENTO_FILA, ());
    Ok(())
}

#[tauri::command]
fn set_repeat(app: AppHandle, fila: State<FilaState>, modo: queue::Repetir) -> Result<(), String> {
    fila.0.lock().map_err(erro_db)?.definir_repetir(modo);
    let _ = app.emit(EVENTO_FILA, ());
    Ok(())
}

#[tauri::command]
fn previous_track(app: AppHandle) -> Result<(), String> {
    voltar(&app)
}

/// Depois de 3 s volta ao início da faixa; antes disso, vai para a anterior.
fn voltar(app: &AppHandle) -> Result<(), String> {
    let fila = app.state::<FilaState>();
    let player = app.state::<player::Player>();
    if player.posicao_ms() > 3000 {
        return player.buscar(0);
    }
    let anterior = fila.0.lock().map_err(erro_db)?.anterior();
    let _ = app.emit(EVENTO_FILA, ());
    match anterior {
        Some(id) => tocar_id(app, id),
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
    aleatorio: bool,
    repetir: queue::Repetir,
}

#[tauri::command]
fn get_queue(db: State<db::Db>, fila: State<FilaState>) -> Result<VisaoFila, String> {
    let (atual, manual, contexto, aleatorio, repetir) = {
        let f = fila.0.lock().map_err(erro_db)?;
        (f.atual(), f.manual().collect::<Vec<_>>(), f.proximas_do_contexto(50), f.aleatorio(), f.repetir())
    };
    let conn = db.0.lock().map_err(erro_db)?;
    let buscar = |ids: &[i64]| library::obter_varios(&conn, ids).map_err(|e| e.to_string());
    Ok(VisaoFila {
        atual: buscar(&atual.into_iter().collect::<Vec<_>>())?.pop(),
        manual: buscar(&manual)?,
        contexto: buscar(&contexto)?,
        aleatorio,
        repetir,
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
            app.manage(HistoricoState(Mutex::new(None)));
            app.manage(OrigemState(Mutex::new(None)));
            app.manage(watch::Vigia::iniciar(app.handle().clone())?);
            watch::Vigia::retomar(app.handle().clone());
            let handle = app.handle().clone();
            app.manage(player::Player::new(move |estado| {
                let fim = estado.fim;
                acompanhar_sessao(&handle, &estado);
                if let Some(m) = handle.try_state::<mpris::Mpris>() {
                    m.atualizar(&handle, &estado);
                }
                let _ = handle.emit("player-estado", estado);
                if fim {
                    // Fora da thread do barramento, para não bloqueá-la.
                    let h = handle.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = avancar(&h, true) {
                            let _ = h.emit("player-erro", e);
                        }
                    });
                }
            })?);
            // Sem D-Bus (sessão sem barramento), o app funciona só sem MPRIS.
            match mpris::Mpris::iniciar(app.handle().clone()) {
                Ok(m) => {
                    app.manage(m);
                }
                Err(e) => eprintln!("{e}"),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_status,
            import_folder,
            watched_folders,
            unwatch_folder,
            list_media,
            count_media,
            play_media,
            next_track,
            set_shuffle,
            set_repeat,
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
            history_list,
            history_count,
            toggle_pause,
            set_volume,
            seek
        ])
        .build(tauri::generate_context!())
        .expect("falha ao iniciar o Skáld")
        .run(|app, evento| {
            // Ao fechar, grava o tempo ouvido da faixa em andamento.
            if let tauri::RunEvent::Exit = evento {
                encerrar_sessao(app, false);
            }
        });
}
