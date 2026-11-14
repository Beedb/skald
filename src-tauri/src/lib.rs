mod apelidos;
mod artistas;
mod backup;
mod buscaletras;
mod colecao;
mod db;
mod details;
mod enriquecer;
mod eq;
mod ganho;
mod history;
mod import;
mod letras;
mod library;
mod miniaturas;
mod mixes;
mod player;
mod playlists;
mod mpris;
mod queue;
mod settings;
mod sistema;
mod temporizador;
mod stats;
mod watch;
mod rganalise;
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
        if !pasta.is_dir() {
            return Err("A pasta selecionada não existe.".into());
        }
        // Registrada antes de importar: a categoria da pasta já vale para as músicas dela.
        {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            watch::adicionar(&conn, &path).map_err(|e| e.to_string())?;
        }
        let mut resumo = import::importar(&app.state::<db::Db>().0, pasta)?;
        {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            resumo.ausentes += import::marcar_sumidos(&conn, pasta).map_err(|e| e.to_string())?;
        }
        app.state::<watch::Vigia>().vigiar(pasta);
        resumo.duplicados += import::preencher_hashes(&app.state::<db::Db>().0)?;
        reconciliar_movidos(&app, &mut resumo)?;
        if resumo.adicionados > 0 {
            incluir_novas(&app, Origem::Biblioteca)?;
        }
        Ok(resumo)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Importa arquivos soltos escolhidos pelo usuário (não viram pasta monitorada). Arquivos
/// excluídos antes da biblioteca voltam, porque a importação foi pedida explicitamente.
#[tauri::command]
async fn import_files(app: tauri::AppHandle, paths: Vec<String>) -> Result<import::ImportResumo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let arquivos: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
        {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            import::readmitir(&conn, &arquivos).map_err(|e| e.to_string())?;
        }
        let mut resumo = import::importar_arquivos(&app.state::<db::Db>().0, &arquivos)?;
        resumo.duplicados += import::preencher_hashes(&app.state::<db::Db>().0)?;
        reconciliar_movidos(&app, &mut resumo)?;
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
pub(crate) fn biblioteca_mudou(app: &AppHandle, mut resumo: import::ImportResumo) {
    // Também preenche, em segundo plano, o hash das músicas anteriores à etapa 20.
    match import::preencher_hashes(&app.state::<db::Db>().0) {
        Ok(n) => resumo.duplicados += n,
        Err(e) => eprintln!("Hash dos arquivos: {e}"),
    }
    if resumo.adicionados + resumo.ausentes > 0 {
        let _ = reconciliar_movidos(app, &mut resumo);
    }
    if !resumo.mudou_algo() {
        return;
    }
    if resumo.adicionados > 0 {
        let _ = incluir_novas(app, Origem::Biblioteca);
    }
    let _ = app.emit(EVENTO_BIBLIOTECA, &resumo);
}

/// Une registros de arquivos renomeados/movidos (seção 5) e atualiza a fila. Os renomeados
/// deixam de contar como "novos" e "não encontrados" no resumo.
fn reconciliar_movidos(app: &AppHandle, resumo: &mut import::ImportResumo) -> Result<(), String> {
    let pares = {
        let db = app.state::<db::Db>();
        let mut conn = db.0.lock().map_err(erro_db)?;
        import::reconciliar_movidos(&mut conn).map_err(|e| e.to_string())?
    };
    if pares.is_empty() {
        return Ok(());
    }
    {
        let estado_fila = app.state::<FilaState>();
        let mut fila = estado_fila.0.lock().map_err(erro_db)?;
        for &(de, para) in &pares {
            fila.trocar_id(de, para);
        }
    }
    let n = pares.len() as u32;
    resumo.movidos += n;
    resumo.adicionados = resumo.adicionados.saturating_sub(n);
    resumo.ausentes = resumo.ausentes.saturating_sub(n);
    let _ = app.emit(EVENTO_FILA, ());
    let _ = app.emit(EVENTO_PLAYLISTS, ());
    let _ = app.emit(EVENTO_HISTORICO, ());
    Ok(())
}

/// Exclui músicas da biblioteca (só o registro; o arquivo fica no disco). Se a atual saiu,
/// o player segue para a próxima da fila.
#[tauri::command]
fn remove_from_library(app: AppHandle, ids: Vec<i64>) -> Result<usize, String> {
    let n = {
        let db = app.state::<db::Db>();
        let mut conn = db.0.lock().map_err(erro_db)?;
        library::excluir(&mut conn, &ids).map_err(|e| e.to_string())?
    };
    // A capa guardada no Skáld era só deste registro.
    for &id in &ids {
        details::remover_capa_salva(id);
        letras::remover_salva(id);
    }
    let atual_saiu = {
        let estado_fila = app.state::<FilaState>();
        let mut fila = estado_fila.0.lock().map_err(erro_db)?;
        let conjunto = ids.iter().copied().collect();
        fila.remover_ids(&conjunto);
        fila.atual().is_some_and(|a| conjunto.contains(&a))
    };
    if atual_saiu {
        avancar(&app, false)?;
    }
    let _ = app.emit(EVENTO_FILA, ());
    let _ = app.emit(EVENTO_PLAYLISTS, ());
    let _ = app.emit(EVENTO_HISTORICO, ());
    Ok(n)
}

/// Arquivos idênticos na biblioteca (só aviso; nada é apagado).
#[tauri::command]
fn duplicates_list(db: State<db::Db>) -> Result<Vec<import::GrupoDuplicados>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    import::duplicados(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn duplicate_ignore(db: State<db::Db>, hash: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    import::ignorar_duplicado(&conn, &hash).map_err(|e| e.to_string())
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
        let db = app.state::<db::Db>();
        if let Ok(conn) = db.0.lock() {
            let _ = history::encerrar(&conn, &s);
        }
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

/// Próxima faixa já entregue ao player para tocar sem pausa (gapless). A fila só avança de
/// verdade quando ela começa a tocar: `antes` é a fila no momento do pedido, `depois` é a
/// cópia já avançada.
struct Pendente {
    antes: queue::Fila,
    depois: queue::Fila,
    id: i64,
}

struct GaplessState(Mutex<Option<Pendente>>);

/// Chamada pelo player pouco antes do fim da faixa: escolhe a seguinte numa cópia da fila
/// (respeitando fila manual, aleatório e repetir) e pula arquivos ausentes.
fn preparar_proxima(app: &AppHandle) -> Option<player::Proxima> {
    // Temporizador "fim desta música": nada é emendado; a faixa termina e para.
    if app.state::<TemporizadorState>().0.lock().ok()?.fim_da_faixa() {
        return None;
    }
    let antes = app.state::<FilaState>().0.lock().ok()?.clone();
    let mut depois = antes.clone();
    for tentativa in 0..100 {
        let id = if tentativa == 0 { depois.proxima_automatica() } else { depois.proxima() }?;
        let proxima = {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().ok()?;
            let (musica, path) = library::obter(&conn, id).ok()??;
            let existe = std::path::Path::new(&path).is_file();
            if existe == musica.missing {
                let _ = library::marcar_ausente(&conn, id, !existe);
            }
            let mesmo_album = antes.atual().is_some_and(|a| library::mesmo_album(&conn, a, id).unwrap_or(false));
            existe.then_some((id, path, mesmo_album))
        };
        if let Some((id, path, mesmo_album)) = proxima {
            let ganho = ganho_de(app, id, &path);
            let p = player::Proxima { id, path, mesmo_album, ganho };
            *app.state::<GaplessState>().0.lock().ok()? = Some(Pendente { antes, depois, id });
            return Some(p);
        }
    }
    None
}

/// A faixa preparada começou a tocar: avança a fila e troca a sessão do histórico.
fn concluir_transicao(app: &AppHandle, id: i64) {
    let pendente = app.state::<GaplessState>().0.lock().ok().and_then(|mut p| p.take());
    let corrigir = {
        let fila = app.state::<FilaState>();
        let Ok(mut f) = fila.0.lock() else { return };
        match pendente {
            Some(p) if p.id == id && *f == p.antes => {
                *f = p.depois;
                None
            }
            // A fila mudou nesses segundos (ex.: música adicionada): vale a fila atual.
            _ => f.proxima_automatica().filter(|&real| real != id),
        }
    };
    encerrar_sessao(app, true);
    iniciar_sessao(app, id);
    let _ = app.emit(EVENTO_FILA, ());
    if let Some(real) = corrigir {
        let h = app.clone();
        std::thread::spawn(move || {
            if let Err(e) = tocar_id(&h, real) {
                let _ = h.emit("player-erro", e);
            }
        });
    }
}

/// ReplayGain das tags; sem elas, o calculado pelo Skáld (análise em segundo plano).
fn ganho_de(app: &AppHandle, id: i64, path: &str) -> ganho::Ganho {
    let g = ganho::ler(std::path::Path::new(path));
    if g.faixa.is_some() || g.album.is_some() {
        return g;
    }
    app.state::<db::Db>().0.lock().map(|c| rganalise::do_banco(&c, id)).unwrap_or_default()
}

/// Situação da análise de volume (Configurações).
#[tauri::command]
fn replaygain_status(db: State<db::Db>) -> Result<rganalise::Situacao, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    rganalise::situacao(&conn).map_err(|e| e.to_string())
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
    let ganho = ganho_de(app, id, &path);
    app.state::<player::Player>().tocar(id, &path, ganho)?;
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
    {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        playlists::marcar_tocada(&conn, id)?;
    }
    let _ = app.emit(EVENTO_PLAYLISTS, ());
    definir_origem(&app, Some(Origem::Playlist(id, ordem)))?;
    let _ = app.emit(EVENTO_FILA, ());
    tocar_id(&app, musica)
}

#[derive(serde::Serialize)]
struct Inicio {
    albuns_recentes: Vec<colecao::Album>,
    tocadas_recentes: Vec<library::MediaRow>,
    mixes: Vec<mixes::Mix>,
}

/// Prévia da biblioteca para a tela inicial (seção 4.2). As playlists recentes vêm de
/// `playlists_list` (`tocada_em`).
#[tauri::command]
fn home_data(db: State<db::Db>) -> Result<Inicio, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    Ok(Inicio {
        albuns_recentes: colecao::albuns_recentes(&conn, 12).map_err(|e| e.to_string())?,
        tocadas_recentes: colecao::tocadas_recentes(&conn, 8).map_err(|e| e.to_string())?,
        mixes: mixes::listar(&conn).map_err(|e| e.to_string())?,
    })
}

/// Resto do álbum e mais músicas dos artistas da faixa (aba "Relacionados" do player).
#[tauri::command]
fn related_tracks(db: State<db::Db>, id: i64) -> Result<colecao::Relacionadas, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    colecao::relacionadas(&conn, id).map_err(|e| e.to_string())
}

/// Toca um mix da tela inicial (geral, gênero ou artista), sorteado na hora.
#[tauri::command]
fn play_mix(app: AppHandle, fila: State<FilaState>, tipo: mixes::Tipo, valor: Option<String>) -> Result<(), String> {
    let ids = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        mixes::ids(&conn, tipo, valor.as_deref()).map_err(|e| e.to_string())?
    };
    tocar_lista(&app, &fila, ids, 0)
}

#[tauri::command]
fn artists_list(db: State<db::Db>) -> Result<Vec<colecao::Artista>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    colecao::artistas(&conn).map_err(|e| e.to_string())
}

/// Álbuns da biblioteca; com `por_artista`, só os que têm faixas de `artista` (nulo: sem artista).
#[tauri::command]
fn albums_list(db: State<db::Db>, por_artista: bool, artista: Option<String>) -> Result<Vec<colecao::Album>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    let filtro = por_artista.then_some(artista.as_deref());
    colecao::albuns(&conn, filtro).map_err(|e| e.to_string())
}

#[tauri::command]
fn album_tracks(db: State<db::Db>, chave: colecao::ChaveAlbum) -> Result<Vec<colecao::Faixa>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    colecao::faixas_do_album(&conn, &chave).map_err(|e| e.to_string())
}

/// Toca o álbum a partir da faixa `indice` (o álbum vira o contexto da fila).
#[tauri::command]
fn play_album(app: AppHandle, fila: State<FilaState>, chave: colecao::ChaveAlbum, indice: usize) -> Result<(), String> {
    let ids: Vec<i64> = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        colecao::faixas_do_album(&conn, &chave).map_err(|e| e.to_string())?.into_iter().map(|f| f.media.id).collect()
    };
    tocar_lista(&app, &fila, ids, indice)
}

/// Toca todas as músicas do artista (nulo: sem artista).
#[tauri::command]
fn play_artist(app: AppHandle, fila: State<FilaState>, artista: Option<String>) -> Result<(), String> {
    let ids = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        colecao::ids_do_artista(&conn, artista.as_deref()).map_err(|e| e.to_string())?
    };
    tocar_lista(&app, &fila, ids, 0)
}

fn tocar_lista(app: &AppHandle, fila: &FilaState, ids: Vec<i64>, indice: usize) -> Result<(), String> {
    let musica = fila.0.lock().map_err(erro_db)?.tocar_do_contexto_em(ids, indice).ok_or("Nada para tocar.")?;
    definir_origem(app, None)?;
    let _ = app.emit(EVENTO_FILA, ());
    tocar_id(app, musica)
}

/// Miniatura da capa (cache em disco), lida fora da thread principal.
#[tauri::command]
async fn cover_thumb(app: AppHandle, id: i64) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let pasta = miniaturas::pasta(app.path().app_cache_dir().map_err(|e| e.to_string())?);
        miniaturas::obter(&app.state::<db::Db>().0, &pasta, id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn history_list(db: State<db::Db>, offset: u32, limit: u32) -> Result<Vec<history::Entrada>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    history::listar(&conn, offset, limit.min(2000)).map_err(|e| e.to_string())
}

#[tauri::command]
fn stats_summary(db: State<db::Db>) -> Result<history::Estatisticas, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    history::estatisticas(&conn).map_err(|e| e.to_string())
}

/// Máquina do Tempo: período "" (sempre), "AAAA" ou "AAAA-MM".
#[tauri::command]
fn stats_period(db: State<db::Db>, periodo: String) -> Result<stats::Painel, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    stats::painel(&conn, &periodo)
}

/// Retrospectivas liberadas (meses e anos encerrados com histórico).
#[tauri::command]
fn retrospective_list(db: State<db::Db>) -> Result<Vec<String>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    let mes = stats::mes_atual(&conn).map_err(|e| e.to_string())?;
    stats::retrospectivas_disponiveis(&conn, &mes).map_err(|e| e.to_string())
}

/// Retrospectiva com capas dos álbuns e "imagens" dos artistas (capa de uma música deles),
/// lidas dos arquivos fora da thread principal.
#[tauri::command]
async fn retrospective_get(app: AppHandle, periodo: String) -> Result<stats::Retrospectiva, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut r = {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            let mes = stats::mes_atual(&conn).map_err(|e| e.to_string())?;
            // Só períodos encerrados (seção 11.5: disponível no 1º dia do período seguinte).
            if periodo.as_str() >= mes.as_str() || (periodo.len() == 4 && periodo.as_str() >= &mes[..4]) {
                return Err("Esta retrospectiva ainda não está disponível.".into());
            }
            stats::retrospectiva(&conn, &periodo)?
        };
        for item in r.albuns.iter_mut().chain(r.artistas.iter_mut()) {
            let Some(id) = item.media_id else { continue };
            let base = {
                let db = app.state::<db::Db>();
                let conn = db.0.lock().map_err(erro_db)?;
                details::do_banco(&conn, id)?
            };
            item.capa = base.map(details::completar_do_arquivo).and_then(|d| d.capa);
        }
        Ok(r)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Grava o PNG (base64) em `<Música>/Skáld/Retrospectivas/<período>/` e devolve o caminho.
#[tauri::command]
fn retrospective_save_png(app: AppHandle, periodo: String, png_base64: String) -> Result<String, String> {
    use base64::Engine;
    let png = base64::engine::general_purpose::STANDARD
        .decode(png_base64.trim_start_matches("data:image/png;base64,"))
        .map_err(|_| "A imagem gerada é inválida.".to_string())?;
    let musica = app.path().audio_dir().map_err(|_| "Pasta de música do sistema não encontrada.".to_string())?;
    let base = musica.join("Skáld").join("Retrospectivas");
    stats::salvar_png(&base, &periodo, &png).map(|p| p.display().to_string())
}

#[tauri::command]
fn stats_months(db: State<db::Db>) -> Result<Vec<String>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    stats::meses(&conn).map_err(|e| e.to_string())
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
fn set_shuffle(app: AppHandle, fila: State<FilaState>, db: State<db::Db>, ativo: bool) -> Result<(), String> {
    fila.0.lock().map_err(erro_db)?.definir_aleatorio(ativo);
    settings::definir_aleatorio(&*db.0.lock().map_err(erro_db)?, ativo).map_err(|e| e.to_string())?;
    let _ = app.emit(EVENTO_FILA, ());
    Ok(())
}

#[tauri::command]
fn set_repeat(app: AppHandle, fila: State<FilaState>, db: State<db::Db>, modo: queue::Repetir) -> Result<(), String> {
    fila.0.lock().map_err(erro_db)?.definir_repetir(modo);
    settings::definir_repetir(&*db.0.lock().map_err(erro_db)?, modo).map_err(|e| e.to_string())?;
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

/// Adiciona várias músicas à fila manual na ordem dada (álbum, playlist).
#[tauri::command]
fn queue_add_many(app: AppHandle, fila: State<FilaState>, ids: Vec<i64>, a_seguir: bool) -> Result<(), String> {
    fila.0.lock().map_err(erro_db)?.adicionar_varios(&ids, a_seguir);
    let _ = app.emit(EVENTO_FILA, ());
    Ok(())
}

/// Adiciona as faixas de um álbum à fila, na ordem do álbum.
#[tauri::command]
fn queue_add_album(app: AppHandle, fila: State<FilaState>, chave: colecao::ChaveAlbum, a_seguir: bool) -> Result<(), String> {
    let ids: Vec<i64> = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        colecao::faixas_do_album(&conn, &chave).map_err(|e| e.to_string())?.into_iter().map(|f| f.media.id).collect()
    };
    queue_add_many(app, fila, ids, a_seguir)
}

/// Adiciona as músicas de uma playlist à fila, na ordem escolhida na tela.
#[tauri::command]
fn queue_add_playlist(
    app: AppHandle,
    fila: State<FilaState>,
    id: i64,
    ordem: playlists::Ordem,
    a_seguir: bool,
) -> Result<(), String> {
    let ids: Vec<i64> = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        playlists::itens(&conn, id, ordem)?.into_iter().map(|i| i.media.id).collect()
    };
    queue_add_many(app, fila, ids, a_seguir)
}

#[tauri::command]
fn queue_move(app: AppHandle, fila: State<FilaState>, de: usize, para: usize) -> Result<(), String> {
    fila.0.lock().map_err(erro_db)?.mover_manual(de, para);
    let _ = app.emit(EVENTO_FILA, ());
    Ok(())
}

/// Limpa o que viria depois da atual. A origem deixa de valer: música nova não volta a encher a fila.
#[tauri::command]
fn queue_clear(app: AppHandle, fila: State<FilaState>) -> Result<(), String> {
    fila.0.lock().map_err(erro_db)?.limpar();
    definir_origem(&app, None)?;
    let _ = app.emit(EVENTO_FILA, ());
    Ok(())
}

#[tauri::command]
fn queue_remove(app: AppHandle, fila: State<FilaState>, posicao: usize) -> Result<(), String> {
    fila.0.lock().map_err(erro_db)?.remover_manual(posicao);
    let _ = app.emit(EVENTO_FILA, ());
    Ok(())
}

struct TemporizadorState(Mutex<temporizador::Temporizador>);

const EVENTO_TEMPORIZADOR: &str = "temporizador-mudou";

/// No fim natural de uma faixa: se o temporizador é "fim desta música", desliga-o e diz para não avançar.
fn encerrar_por_temporizador(app: &AppHandle) -> bool {
    let estado = app.state::<TemporizadorState>();
    let Ok(mut t) = estado.0.lock() else { return false };
    if !t.fim_da_faixa() {
        return false;
    }
    t.definir(None);
    drop(t);
    salvar_temporizador(app, None);
    let _ = app.emit(EVENTO_TEMPORIZADOR, ());
    true
}

/// Guarda o temporizador no banco para continuar depois de fechar e abrir o app.
fn salvar_temporizador(app: &AppHandle, modo: Option<temporizador::Modo>) {
    use temporizador::Modo;
    let salvo = modo.map(|m| match m {
        Modo::FimDaFaixa => settings::TemporizadorSalvo::FimDaFaixa,
        Modo::Ate(fim) => {
            let falta = fim.saturating_duration_since(std::time::Instant::now()).as_millis() as i64;
            settings::TemporizadorSalvo::Ate(unix_agora_ms() + falta)
        }
    });
    if let Ok(conn) = app.state::<db::Db>().0.lock() {
        let _ = settings::definir_temporizador(&conn, salvo);
    }
}

fn unix_agora_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Liga (ou desliga) o temporizador: estado, contagem, volume, banco e aviso à tela.
fn aplicar_temporizador(app: &AppHandle, modo: Option<temporizador::Modo>) -> Result<(), String> {
    let geracao = app.state::<TemporizadorState>().0.lock().map_err(erro_db)?.definir(modo);
    app.state::<player::Player>().atenuar(1.0);
    if let Some(temporizador::Modo::Ate(fim)) = modo {
        let h = app.clone();
        std::thread::spawn(move || contar_temporizador(h, geracao, fim));
    }
    salvar_temporizador(app, modo);
    let _ = app.emit(EVENTO_TEMPORIZADOR, ());
    Ok(())
}

/// Ao abrir: retoma o temporizador salvo, se ainda não acabou.
fn retomar_temporizador(app: &AppHandle) {
    use std::time::{Duration, Instant};
    let salvo = app.state::<db::Db>().0.lock().ok().and_then(|c| settings::temporizador(&c).ok().flatten());
    let modo = match salvo {
        Some(settings::TemporizadorSalvo::FimDaFaixa) => Some(temporizador::Modo::FimDaFaixa),
        Some(settings::TemporizadorSalvo::Ate(ms)) if ms > unix_agora_ms() => {
            Some(temporizador::Modo::Ate(Instant::now() + Duration::from_millis((ms - unix_agora_ms()) as u64)))
        }
        _ => None,
    };
    let _ = aplicar_temporizador(app, modo);
}

/// Liga o temporizador por `minutos`, até o fim da música atual, ou desliga (nenhum dos dois).
#[tauri::command]
fn sleep_timer_set(app: AppHandle, minutos: Option<u32>, fim_da_faixa: bool) -> Result<(), String> {
    use temporizador::Modo;
    use std::time::{Duration, Instant};
    let modo = match (minutos, fim_da_faixa) {
        (_, true) => Some(Modo::FimDaFaixa),
        (Some(m), false) if (1..=24 * 60).contains(&m) => Some(Modo::Ate(Instant::now() + Duration::from_secs(m as u64 * 60))),
        (Some(_), false) => return Err("Tempo inválido para o temporizador.".into()),
        (None, false) => None,
    };
    // A emenda já preparada para a próxima só respeita o modo novo na troca seguinte.
    aplicar_temporizador(&app, modo)
}

/// Espera o fim do temporizador, baixando o volume nos últimos segundos; então pausa.
fn contar_temporizador(app: AppHandle, geracao: u64, fim: std::time::Instant) {
    use std::time::{Duration, Instant};
    loop {
        let vale = app.state::<TemporizadorState>().0.lock().is_ok_and(|t| t.geracao() == geracao);
        if !vale {
            return; // trocado ou desligado: quem trocou já restaurou o volume
        }
        let restante = fim.saturating_duration_since(Instant::now());
        let player = app.state::<player::Player>();
        player.atenuar(temporizador::atenuacao(restante));
        if restante.is_zero() {
            let _ = player.pausar();
            player.atenuar(1.0);
            let acabou = app.state::<TemporizadorState>().0.lock().is_ok_and(|mut t| {
                t.geracao() == geracao && {
                    t.definir(None);
                    true
                }
            });
            if acabou {
                salvar_temporizador(&app, None);
            }
            let _ = app.emit(EVENTO_TEMPORIZADOR, ());
            return;
        }
        let passo = if restante <= temporizador::ATENUAR { Duration::from_millis(100) } else { Duration::from_millis(500) };
        std::thread::sleep(passo.min(restante));
    }
}

#[tauri::command]
fn sleep_timer_get(estado: State<TemporizadorState>) -> Result<temporizador::Visao, String> {
    Ok(estado.0.lock().map_err(erro_db)?.visao(std::time::Instant::now()))
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

/// Letra da música (`.lrc` ao lado ou tag), lida fora da thread principal.
#[tauri::command]
async fn get_lyrics(app: AppHandle, id: i64) -> Result<Option<letras::Letra>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            library::obter(&conn, id).map_err(|e| e.to_string())?.map(|(_, p)| p)
        };
        Ok(path.and_then(|p| letras::carregar(id, std::path::Path::new(&p))))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Procura a letra no LRCLIB (rede; só quando o usuário pede). Nada é gravado aqui.
#[tauri::command]
async fn lyrics_lookup(app: AppHandle, id: i64) -> Result<Option<buscaletras::Encontrada>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (m, _) = {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            library::obter(&conn, id).map_err(|e| e.to_string())?.ok_or("Música não encontrada.")?
        };
        let (Some(titulo), Some(artista)) = (m.title.as_deref(), m.artist.as_deref()) else {
            return Err("Para buscar a letra, a música precisa ter título e artista.".into());
        };
        buscaletras::buscar(titulo, artista, m.album.as_deref(), m.duration_ms)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Músicas presentes, com título e artista, sem letra em lugar nenhum (lê as tags; fora da thread principal).
#[tauri::command]
async fn lyrics_missing(app: AppHandle) -> Result<Vec<library::MediaRow>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let linhas: Vec<(library::MediaRow, String)> = {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, artist, album, duration_ms, missing, path FROM media
                     WHERE category = 'music' AND missing = 0 AND title IS NOT NULL AND artist IS NOT NULL
                     ORDER BY artist COLLATE SEMACENTO, album COLLATE SEMACENTO, track_number, id",
                )
                .map_err(|e| e.to_string())?;
            let it = stmt
                .query_map([], |r| Ok((library::linha(r)?, r.get::<_, String>(6)?)))
                .map_err(|e| e.to_string())?;
            it.collect::<Result<_, _>>().map_err(|e| e.to_string())?
        };
        Ok(linhas.into_iter().filter(|(m, p)| !letras::tem_letra(m.id, std::path::Path::new(p))).map(|(m, _)| m).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Guarda a letra só no Skáld ou, se `no_arquivo`, cria o `.lrc` ao lado da música (sem substituir).
#[tauri::command]
fn lyrics_save(db: State<db::Db>, id: i64, texto: String, no_arquivo: bool) -> Result<(), String> {
    if texto.trim().is_empty() {
        return Err("Letra vazia.".into());
    }
    if no_arquivo {
        let path = {
            let conn = db.0.lock().map_err(erro_db)?;
            library::obter(&conn, id).map_err(|e| e.to_string())?.ok_or("Música não encontrada.")?.1
        };
        letras::salvar_ao_lado(std::path::Path::new(&path), &texto)?;
        // O .lrc ao lado passa a valer; a cópia guardada no Skáld não é mais necessária.
        letras::remover_salva(id);
        Ok(())
    } else {
        letras::salvar_no_skald(id, &texto)
    }
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
fn get_settings(db: State<db::Db>) -> Result<settings::Configuracoes, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    settings::carregar(&conn).map_err(|e| e.to_string())
}

/// Duração do crossfade em ms (0 desliga); vale a partir da próxima troca de faixa.
#[tauri::command]
fn set_appearance(db: State<db::Db>, tema: String, modo: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    settings::definir_aparencia(&conn, &tema, &modo)
}

#[tauri::command]
fn aliases_list(db: State<db::Db>) -> Result<Vec<apelidos::Apelido>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    apelidos::listar(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn alias_add(db: State<db::Db>, alias: String, canonical: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    apelidos::adicionar(&conn, &alias, &canonical)
}

#[tauri::command]
fn alias_remove(db: State<db::Db>, alias: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    apelidos::remover(&conn, &alias).map_err(|e| e.to_string())
}

#[tauri::command]
fn alias_suggestions(db: State<db::Db>) -> Result<Vec<Vec<String>>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    apelidos::sugestoes(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn backup_export(db: State<db::Db>, destino: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    backup::exportar(&conn, std::path::Path::new(&destino))
}

/// Restaura um backup: a fila é esvaziada (os ids podem ser outros) e a tela recarrega.
/// O banco anterior fica salvo ao lado do atual; o caminho volta para avisar o usuário.
#[tauri::command]
fn backup_restore(app: AppHandle, origem: String) -> Result<String, String> {
    let pasta = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let seguranca = backup::copia_de_seguranca(&pasta, unix_agora());
    encerrar_sessao(&app, false);
    {
        let db = app.state::<db::Db>();
        let mut conn = db.0.lock().map_err(erro_db)?;
        backup::restaurar(&mut conn, std::path::Path::new(&origem), &seguranca)?;
    }
    *app.state::<FilaState>().0.lock().map_err(erro_db)? = queue::Fila::default();
    definir_origem(&app, None)?;
    let _ = app.emit(EVENTO_FILA, ());
    Ok(seguranca.display().to_string())
}

fn pastas_de_cache(app: &AppHandle) -> Result<Vec<std::path::PathBuf>, String> {
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    Ok(vec![miniaturas::pasta(cache.clone()), cache.join("mpris")])
}

#[tauri::command]
async fn cache_size(app: AppHandle) -> Result<u64, String> {
    let pastas = pastas_de_cache(&app)?;
    tauri::async_runtime::spawn_blocking(move || backup::tamanho(&pastas)).await.map_err(|e| e.to_string())
}

/// Apaga as capas em cache (refeitas quando preciso). Devolve os bytes liberados.
#[tauri::command]
async fn cache_clear(app: AppHandle) -> Result<u64, String> {
    let pastas = pastas_de_cache(&app)?;
    tauri::async_runtime::spawn_blocking(move || backup::limpar(&pastas)).await.map_err(|e| e.to_string())?
}

/// Ids da lista (com a busca) entre duas posições, para selecionar com Shift.
#[tauri::command]
fn media_ids_range(db: State<db::Db>, busca: Option<String>, de: usize, ate: usize) -> Result<Vec<i64>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    let ids = library::ids_em_ordem(&conn, busca.as_deref().unwrap_or("")).map_err(|e| e.to_string())?;
    let (a, b) = (de.min(ate), de.max(ate).saturating_add(1).min(ids.len()));
    Ok(ids.get(a..b).map(<[i64]>::to_vec).unwrap_or_default())
}

#[tauri::command]
fn incomplete_list(db: State<db::Db>) -> Result<Vec<enriquecer::Incompleta>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    enriquecer::incompletas(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn batch_values(db: State<db::Db>, ids: Vec<i64>) -> Result<enriquecer::Lote, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    enriquecer::lote_valores(&conn, &ids)
}

/// Edição em lote de artista, álbum, gênero e ano (só os campos enviados).
#[tauri::command]
async fn batch_edit(
    app: AppHandle,
    ids: Vec<i64>,
    campos: enriquecer::Campos,
    no_arquivo: bool,
) -> Result<enriquecer::ResultadoLote, String> {
    let h = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || {
        enriquecer::lote_aplicar(&h.state::<db::Db>().0, &ids, &campos, no_arquivo)
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = app.emit(EVENTO_FILA, ());
    Ok(r)
}

/// Valores atuais da música para a janela de informações.
#[tauri::command]
fn metadata_get(db: State<db::Db>, id: i64) -> Result<enriquecer::Atual, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    enriquecer::atual(&conn, id)
}

/// Sugestões do MusicBrainz (rede, fora da thread principal).
#[tauri::command]
async fn metadata_lookup(
    titulo: String,
    artista: Option<String>,
    duracao_ms: Option<i64>,
) -> Result<Vec<enriquecer::Candidato>, String> {
    tauri::async_runtime::spawn_blocking(move || enriquecer::buscar(&titulo, artista.as_deref(), duracao_ms))
        .await
        .map_err(|e| e.to_string())?
}

/// Grava os campos escolhidos (banco, e arquivo se pedido, com a capa opcional).
#[tauri::command]
async fn metadata_apply(
    app: AppHandle,
    id: i64,
    campos: enriquecer::Campos,
    no_arquivo: bool,
    release_capa: Option<String>,
) -> Result<(), String> {
    let h = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        enriquecer::aplicar(&h.state::<db::Db>().0, id, &campos, no_arquivo, release_capa.as_deref())
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = app.emit(EVENTO_FILA, ());
    Ok(())
}

/// Erro do MPRIS (`None`: ativo). Mostrado em Configurações.
struct MprisStatus(Mutex<Option<String>>);

/// Tenta registrar o MPRIS; se falhar, tenta de novo a cada 15 s por até 10 minutos.
fn iniciar_mpris(app: AppHandle) {
    const TENTATIVAS: u32 = 40;
    std::thread::spawn(move || {
        for tentativa in 1..=TENTATIVAS {
            let status = app.state::<MprisStatus>();
            match mpris::Mpris::iniciar(app.clone()) {
                Ok(m) => {
                    app.manage(m);
                    if let Ok(mut s) = status.0.lock() {
                        *s = None;
                    }
                    let _ = app.emit("mpris-mudou", ());
                    return;
                }
                Err(e) => {
                    eprintln!("{e} (tentativa {tentativa})");
                    if let Ok(mut s) = status.0.lock() {
                        *s = Some(if tentativa < TENTATIVAS {
                            format!("{e}. Tentando de novo…")
                        } else {
                            format!("{e}. Reabra o Skáld depois que a sessão estiver pronta.")
                        });
                    }
                    let _ = app.emit("mpris-mudou", ());
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(15));
        }
    });
}

/// `None` quando o MPRIS está ativo; senão o motivo.
#[tauri::command]
fn mpris_status(estado: State<MprisStatus>) -> Result<Option<String>, String> {
    Ok(estado.0.lock().map_err(erro_db)?.clone())
}

const ID_BANDEJA: &str = "principal";

fn mostrar_janela(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Mostra ou esconde o ícone da bandeja (criado na primeira vez). Sem a biblioteca de
/// indicadores (libayatana-appindicator3), a criação falha sem derrubar o app.
fn definir_bandeja(app: &AppHandle, ativo: bool) -> Result<(), String> {
    if let Some(t) = app.tray_by_id(ID_BANDEJA) {
        return t.set_visible(ativo).map_err(|e| e.to_string());
    }
    if !ativo {
        return Ok(());
    }
    let criar = || -> tauri::Result<()> {
        use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
        use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
        let mostrar = MenuItem::with_id(app, "mostrar", "Mostrar o Skáld", true, None::<&str>)?;
        let pausa = MenuItem::with_id(app, "pausa", "Tocar/Pausar", true, None::<&str>)?;
        let proxima = MenuItem::with_id(app, "proxima", "Próxima", true, None::<&str>)?;
        let anterior = MenuItem::with_id(app, "anterior", "Anterior", true, None::<&str>)?;
        let separador = PredefinedMenuItem::separator(app)?;
        let sair = MenuItem::with_id(app, "sair", "Sair", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&mostrar, &pausa, &proxima, &anterior, &separador, &sair])?;
        let mut b = TrayIconBuilder::with_id(ID_BANDEJA)
            .tooltip("Skáld")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, ev| {
                let r = match ev.id().as_ref() {
                    "mostrar" => Ok(mostrar_janela(app)),
                    "pausa" => app.state::<player::Player>().alternar_pausa(),
                    "proxima" => avancar(app, false),
                    "anterior" => voltar(app),
                    "sair" => Ok(app.exit(0)),
                    _ => Ok(()),
                };
                if let Err(e) = r {
                    let _ = app.emit("player-erro", e);
                }
            })
            .on_tray_icon_event(|tray, ev| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                    mostrar_janela(tray.app_handle());
                }
            });
        if let Some(icone) = app.default_window_icon() {
            b = b.icon(icone.clone());
        }
        b.build(app)?;
        Ok(())
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(criar)) {
        Ok(r) => r.map_err(|e| format!("Não foi possível criar o ícone da bandeja: {e}")),
        Err(_) => Err("Bandeja indisponível: instale o pacote libayatana-appindicator3-1.".into()),
    }
}

/// "Fechar envia para a bandeja". Se a bandeja não puder ser criada, a opção volta a desligada.
#[tauri::command]
fn set_tray(app: AppHandle, ativo: bool) -> Result<(), String> {
    let r = definir_bandeja(&app, ativo);
    let gravar = ativo && r.is_ok();
    let db = app.state::<db::Db>();
    let conn = db.0.lock().map_err(erro_db)?;
    settings::definir_bandeja(&conn, gravar).map_err(|e| e.to_string())?;
    r
}

#[tauri::command]
fn set_notifications(db: State<db::Db>, ativo: bool) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    settings::definir_notificar(&conn, ativo).map_err(|e| e.to_string())
}

#[tauri::command]
fn autostart_get(app: AppHandle) -> Result<bool, String> {
    Ok(sistema::autostart_ativo(&app.path().config_dir().map_err(|e| e.to_string())?))
}

#[tauri::command]
fn set_autostart(app: AppHandle, ativo: bool) -> Result<(), String> {
    let dir = app.path().config_dir().map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    sistema::definir_autostart(&dir, &exe, ativo)
}

/// Última música notificada, para avisar só quando ela muda.
struct NotificadaState(Mutex<Option<i64>>);

/// Notifica a troca de música se a opção estiver ligada e a janela não estiver em foco.
fn notificar_troca(app: &AppHandle, estado: &player::EstadoPlayer) {
    let Some(id) = estado.id else { return };
    if estado.estado != player::Estado::Tocando {
        return;
    }
    {
        let estado_notif = app.state::<NotificadaState>();
        let Ok(mut ultima) = estado_notif.0.lock() else { return };
        if *ultima == Some(id) {
            return;
        }
        *ultima = Some(id);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let em_foco = app
            .get_webview_window("main")
            .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false));
        if em_foco {
            return;
        }
        let musica = {
            let db = app.state::<db::Db>();
            let Ok(conn) = db.0.lock() else { return };
            if !settings::carregar(&conn).is_ok_and(|c| c.notificar) {
                return;
            }
            library::obter(&conn, id).ok().flatten().map(|(m, _)| m)
        };
        let Some(m) = musica else { return };
        let capa = app
            .path()
            .app_cache_dir()
            .ok()
            .and_then(|c| miniaturas::arquivo(&app.state::<db::Db>().0, &miniaturas::pasta(c), id).ok().flatten());
        sistema::notificar_musica(
            m.title.as_deref().unwrap_or("Sem título"),
            m.artist.as_deref().unwrap_or("Artista desconhecido"),
            capa.as_deref(),
        );
    });
}

#[tauri::command]
fn set_dynamic_colors(db: State<db::Db>, ativo: bool) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    settings::definir_cores_dinamicas(&conn, ativo).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_crossfade(db: State<db::Db>, player: State<player::Player>, ms: u64) -> Result<u64, String> {
    let ms = {
        let conn = db.0.lock().map_err(erro_db)?;
        settings::definir_crossfade(&conn, ms).map_err(|e| e.to_string())?
    };
    player.definir_crossfade(ms);
    Ok(ms)
}

#[tauri::command]
fn set_normalization(db: State<db::Db>, player: State<player::Player>, modo: ganho::Modo) -> Result<(), String> {
    {
        let conn = db.0.lock().map_err(erro_db)?;
        settings::definir_normalizacao(&conn, modo).map_err(|e| e.to_string())?;
    }
    player.definir_normalizacao(modo);
    Ok(())
}

#[tauri::command]
fn set_equalizer(db: State<db::Db>, player: State<player::Player>, preset: eq::Preset) -> Result<(), String> {
    {
        let conn = db.0.lock().map_err(erro_db)?;
        settings::definir_equalizador(&conn, preset).map_err(|e| e.to_string())?;
    }
    player.definir_equalizador(preset);
    Ok(())
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
            details::definir_pasta_capas(dir.join("capas"));
            letras::definir_pasta(dir.join("letras"));
            app.manage(FilaState(Mutex::new(queue::Fila::default())));
            app.manage(HistoricoState(Mutex::new(None)));
            app.manage(OrigemState(Mutex::new(None)));
            app.manage(GaplessState(Mutex::new(None)));
            app.manage(NotificadaState(Mutex::new(None)));
            app.manage(TemporizadorState(Mutex::new(temporizador::Temporizador::default())));
            app.manage(watch::Vigia::iniciar(app.handle().clone())?);
            watch::Vigia::retomar(app.handle().clone());
            let handle = app.handle().clone();
            let h_proxima = app.handle().clone();
            app.manage(player::Player::new(move |estado| {
                let fim = estado.fim;
                if estado.transicao {
                    if let Some(id) = estado.id {
                        concluir_transicao(&handle, id);
                    }
                }
                acompanhar_sessao(&handle, &estado);
                notificar_troca(&handle, &estado);
                if let Some(m) = handle.try_state::<mpris::Mpris>() {
                    m.atualizar(&handle, &estado);
                }
                let _ = handle.emit("player-estado", estado);
                if fim && encerrar_por_temporizador(&handle) {
                    // A música atual terminou e o temporizador pediu para parar aqui.
                } else if fim {
                    // Fora da thread do barramento, para não bloqueá-la.
                    let h = handle.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = avancar(&h, true) {
                            let _ = h.emit("player-erro", e);
                        }
                    });
                }
            }, move || preparar_proxima(&h_proxima))?);
            {
                let db = app.state::<db::Db>();
                let conn = db.0.lock().map_err(erro_db)?;
                let cfg = settings::carregar(&conn)?;
                app.state::<player::Player>().definir_crossfade(cfg.crossfade_ms);
                app.state::<player::Player>().definir_normalizacao(cfg.normalizacao);
                app.state::<player::Player>().definir_equalizador(cfg.equalizador);
                // Aleatório e repetir voltam como estavam.
                let (aleatorio, repetir) = settings::modos_fila(&conn)?;
                let fila = app.state::<FilaState>();
                let mut f = fila.0.lock().map_err(erro_db)?;
                f.definir_aleatorio(aleatorio);
                f.definir_repetir(repetir);
            }
            // Sem D-Bus o app segue sem MPRIS e tenta de novo (o barramento pode subir depois).
            app.manage(MprisStatus(Mutex::new(Some("Conectando…".into()))));
            iniciar_mpris(app.handle().clone());
            retomar_temporizador(app.handle());
            rganalise::iniciar(app.handle().clone());
            // Bandeja ligada: cria o ícone; iniciado com o sistema, abre só nela.
            let bandeja = {
                let db = app.state::<db::Db>();
                let conn = db.0.lock().map_err(erro_db)?;
                settings::carregar(&conn)?.bandeja
            };
            if bandeja {
                match definir_bandeja(app.handle(), true) {
                    Ok(()) if std::env::args().any(|a| a == sistema::ARG_MINIMIZADO) => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.hide();
                        }
                    }
                    Ok(()) => {}
                    Err(e) => eprintln!("{e}"),
                }
            }
            Ok(())
        })
        .on_window_event(|janela, evento| {
            // Com a bandeja ligada, fechar só esconde a janela; a música continua.
            if let tauri::WindowEvent::CloseRequested { api, .. } = evento {
                let app = janela.app_handle();
                let bandeja = app.tray_by_id(ID_BANDEJA).is_some()
                    && app.state::<db::Db>().0.lock().ok().and_then(|c| settings::carregar(&c).ok()).is_some_and(|c| c.bandeja);
                if bandeja {
                    api.prevent_close();
                    let _ = janela.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            db_status,
            import_folder,
            import_files,
            duplicates_list,
            remove_from_library,
            duplicate_ignore,
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
            queue_add_many,
            queue_add_album,
            queue_add_playlist,
            queue_move,
            queue_clear,
            sleep_timer_set,
            sleep_timer_get,
            get_queue,
            get_track_details,
            get_lyrics,
            lyrics_lookup,
            lyrics_save,
            lyrics_missing,
            playlists_list,
            playlist_create,
            playlist_rename,
            playlist_delete,
            playlist_add,
            playlist_items,
            playlist_remove_item,
            playlist_move_item,
            play_playlist,
            artists_list,
            home_data,
            albums_list,
            album_tracks,
            play_album,
            play_mix,
            related_tracks,
            play_artist,
            cover_thumb,
            toggle_like,
            liked_ids,
            playlist_set_cover,
            playlist_clear_cover,
            playlist_cover,
            playlist_export_m3u,
            history_list,
            history_count,
            stats_summary,
            stats_period,
            stats_months,
            retrospective_list,
            retrospective_get,
            retrospective_save_png,
            toggle_pause,
            set_volume,
            seek,
            get_settings,
            set_appearance,
            set_dynamic_colors,
            set_tray,
            mpris_status,
            replaygain_status,
            backup_export,
            aliases_list,
            alias_add,
            alias_remove,
            alias_suggestions,
            backup_restore,
            cache_size,
            cache_clear,
            metadata_get,
            media_ids_range,
            batch_values,
            incomplete_list,
            batch_edit,
            metadata_lookup,
            metadata_apply,
            set_notifications,
            autostart_get,
            set_autostart,
            set_crossfade,
            set_normalization,
            set_equalizer
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
