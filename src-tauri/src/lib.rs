mod acustico;
mod apelidos;
mod audiolivros;
mod caminhos;
mod drive;
mod sincdados;
mod sincpasta;
mod artistas;
mod categorias;
mod backup;
#[cfg(test)]
mod teste_util;
mod buscaletras;
mod colecao;
mod db;
mod details;
mod enriquecer;
mod eq;
mod ganho;
mod history;
mod import;
mod inteligentes;
mod letras;
mod library;
mod miniaturas;
mod mixes;
mod novidades;
mod player;
mod playlists;
mod podcasts;
mod rss;
#[cfg(desktop)]
mod mpris;
#[cfg(target_os = "android")]
#[path = "midia_android.rs"]
mod mpris;
#[cfg(all(mobile, not(target_os = "android")))]
#[path = "mpris_stub.rs"]
mod mpris;
mod queue;
mod settings;
mod sistema;
mod temporizador;
mod stats;
mod watch;
mod rganalise;
mod saida;
mod search;

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
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
    let path = crate::caminhos::dados(&app)
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
    tauri::async_runtime::spawn_blocking(move || importar_pasta(&app, &path))
        .await
        .map_err(|e| e.to_string())?
}

/// Registra a pasta como monitorada e importa o que há nela (botão "Importar pasta" e
/// músicas baixadas do Drive).
fn importar_pasta(app: &AppHandle, path: &str) -> Result<import::ImportResumo, String> {
    {
        let pasta = std::path::Path::new(path);
        if !pasta.is_dir() {
            return Err("A pasta selecionada não existe.".into());
        }
        // Registrada antes de importar: a categoria da pasta já vale para as músicas dela.
        {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            watch::adicionar(&conn, path).map_err(|e| e.to_string())?;
        }
        let mut resumo = import::importar(&app.state::<db::Db>().0, pasta)?;
        {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            resumo.ausentes += import::marcar_sumidos(&conn, pasta).map_err(|e| e.to_string())?;
        }
        app.state::<watch::Vigia>().vigiar(pasta);
        resumo.duplicados += import::preencher_hashes(&app.state::<db::Db>().0)?;
        reconciliar_movidos(app, &mut resumo)?;
        if resumo.adicionados > 0 {
            incluir_novas(app, Origem::Biblioteca)?;
        }
        podcasts::sincronizar(&app.state::<db::Db>().0)?;
        audiolivros::sincronizar(&app.state::<db::Db>().0)?;
        Ok(resumo)
    }
}

const EVENTO_PODCASTS: &str = "podcasts-mudou";

#[tauri::command]
fn podcast_shows(db: State<db::Db>) -> Result<Vec<podcasts::Programa>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    podcasts::programas(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn podcast_show(db: State<db::Db>, id: i64) -> Result<Option<podcasts::Programa>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    podcasts::programa(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn podcast_episodes(db: State<db::Db>, id: i64) -> Result<Vec<podcasts::Episodio>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    podcasts::episodios(&conn, id).map_err(|e| e.to_string())
}

/// "Continuar ouvindo" do Início.
#[tauri::command]
fn podcast_in_progress(db: State<db::Db>) -> Result<Vec<podcasts::Episodio>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    podcasts::em_andamento(&conn, 10).map_err(|e| e.to_string())
}

#[tauri::command]
fn podcast_mark(app: AppHandle, id: i64, ouvido: bool) -> Result<(), String> {
    {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        podcasts::marcar(&conn, id, ouvido).map_err(|e| e.to_string())?;
    }
    let _ = app.emit(EVENTO_PODCASTS, ());
    Ok(())
}

/// Toca um episódio. A fila é só ele; ao terminar, "Continuar tocando" segue com o próximo
/// não ouvido do programa.
#[tauri::command]
fn play_episode(app: AppHandle, id: i64) -> Result<(), String> {
    let media = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        conn.query_row("SELECT media_id FROM podcast_episode WHERE id = ?1", [id], |r| r.get::<_, Option<i64>>(0))
            .map_err(|e| e.to_string())?
            .ok_or("Este episódio não tem áudio neste aparelho.")?
    };
    app.state::<FilaState>().0.lock().map_err(erro_db)?.tocar_do_contexto(vec![media], media);
    definir_origem(&app, None)?;
    let _ = app.emit(EVENTO_FILA, ());
    tocar_id(&app, media)
}

/// Faixa atual é um episódio ou um capítulo de audiolivro? (o player mostra −15/+30 s e a
/// velocidade).
#[derive(Serialize)]
struct InfoPodcast {
    episodio: Option<i64>,
    programa: Option<i64>,
    livro: Option<i64>,
    velocidade: f64,
}

#[tauri::command]
fn podcast_info(db: State<db::Db>, media_id: i64) -> Result<Option<InfoPodcast>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    let velocidade = settings::podcast_velocidade(&conn).map_err(|e| e.to_string())?;
    if let Some((episodio, programa, _, _)) = podcasts::de_media(&conn, media_id).map_err(|e| e.to_string())? {
        return Ok(Some(InfoPodcast { episodio: Some(episodio), programa: Some(programa), livro: None, velocidade }));
    }
    Ok(audiolivros::livro_do_arquivo(&conn, media_id)
        .map(|livro| InfoPodcast { episodio: None, programa: None, livro: Some(livro), velocidade }))
}

// ---------- Audiolivros ----------

const EVENTO_AUDIOLIVROS: &str = "audiolivros-mudou";

/// Importa uma pasta como Audiolivros: cada pasta com arquivos dentro vira um livro.
#[tauri::command]
async fn import_audiobook_folder(app: tauri::AppHandle, path: String) -> Result<import::ImportResumo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !std::path::Path::new(&path).is_dir() {
            return Err("A pasta selecionada não existe.".to_string());
        }
        {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            audiolivros::marcar_pasta(&conn, &path).map_err(|e| e.to_string())?;
        }
        let r = importar_pasta(&app, &path)?;
        audiolivros::sincronizar(&app.state::<db::Db>().0)?;
        let _ = app.emit(EVENTO_BIBLIOTECA, &r);
        let _ = app.emit(EVENTO_AUDIOLIVROS, ());
        Ok(r)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn audiobooks_list(db: State<db::Db>) -> Result<Vec<audiolivros::Livro>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    audiolivros::livros(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn audiobook_get(db: State<db::Db>, id: i64) -> Result<Option<audiolivros::Livro>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    audiolivros::livro(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn audiobook_chapters(db: State<db::Db>, id: i64) -> Result<Vec<audiolivros::Capitulo>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    audiolivros::capitulos(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn audiobooks_in_progress(db: State<db::Db>) -> Result<Vec<audiolivros::Livro>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    audiolivros::em_andamento(&conn, 10).map_err(|e| e.to_string())
}

/// Toca o livro: de onde parou ou, com `capitulo`, desse capítulo (do começo, ou de onde
/// parou se for o capítulo atual). Os capítulos seguintes vêm em ordem ao terminar cada um.
#[tauri::command]
fn play_audiobook(app: AppHandle, id: i64, capitulo: Option<i64>) -> Result<(), String> {
    let media = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        match capitulo {
            Some(c) => c,
            None => audiolivros::para_tocar(&conn, id).map_err(|e| e.to_string())?.ok_or("Este livro não tem capítulos.")?,
        }
    };
    app.state::<FilaState>().0.lock().map_err(erro_db)?.tocar_do_contexto(vec![media], media);
    definir_origem(&app, None)?;
    let _ = app.emit(EVENTO_FILA, ());
    tocar_id(&app, media)?;
    let _ = app.emit(EVENTO_AUDIOLIVROS, ());
    Ok(())
}

#[tauri::command]
fn audiobook_mark(app: AppHandle, id: i64, concluido: bool) -> Result<(), String> {
    {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        audiolivros::marcar(&conn, id, concluido).map_err(|e| e.to_string())?;
    }
    let _ = app.emit(EVENTO_AUDIOLIVROS, ());
    Ok(())
}

/// Velocidade dos podcasts (0,8× a 2×); vale na hora se um episódio estiver tocando.
#[tauri::command]
fn set_podcast_speed(app: AppHandle, velocidade: f64) -> Result<f64, String> {
    let (v, atual_e_podcast) = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        let v = settings::definir_podcast_velocidade(&conn, velocidade).map_err(|e| e.to_string())?;
        let atual = app.state::<player::Player>().estado().id;
        (v, atual.is_some_and(|id| falado(&conn, id)))
    };
    if atual_e_podcast {
        app.state::<player::Player>().definir_velocidade(v);
    }
    Ok(v)
}

/// Pular para trás ou para a frente (−15 s / +30 s nos podcasts).
#[tauri::command]
fn seek_relative(player: State<player::Player>, delta_ms: i64) -> Result<(), String> {
    let e = player.estado();
    if e.id.is_none() {
        return Ok(());
    }
    let alvo = (e.posicao_ms as i64 + delta_ms).clamp(0, e.duracao_ms.saturating_sub(500) as i64);
    player.buscar(alvo as u64)
}

// ---------- Assinatura por RSS (etapa 59) ----------

/// Downloads de episódios que a pessoa pediu para cancelar.
struct DownloadsState(Mutex<std::collections::HashSet<i64>>);

/// Reduz a capa do feed a 600 px e guarda em `<dados>/capas-podcast/<id>.jpg`.
fn guardar_capa_programa(app: &AppHandle, id: i64, url: &str) -> Result<(), String> {
    let bytes = rss::baixar_imagem(url)?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let mini = img.thumbnail(600, 600).to_rgb8();
    let pasta = caminhos::dados(app)?.join("capas-podcast");
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    let destino = pasta.join(format!("{id}.jpg"));
    let mut saida = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut saida, 85)
        .encode_image(&mini)
        .map_err(|e| e.to_string())?;
    std::fs::write(&destino, saida.into_inner()).map_err(|e| e.to_string())?;
    let db = app.state::<db::Db>();
    let conn = db.0.lock().map_err(erro_db)?;
    podcasts::definir_capa(&conn, id, &destino.to_string_lossy()).map_err(|e| e.to_string())
}

/// Lê o feed, grava programa e episódios novos e, se faltar, a capa. Devolve (id, novos).
fn atualizar_feed(app: &AppHandle, url: &str) -> Result<(i64, u32), String> {
    let feed = rss::buscar(url)?;
    let (id, novos) = {
        let db = app.state::<db::Db>();
        let mut conn = db.0.lock().map_err(erro_db)?;
        podcasts::assinar(&mut conn, url.trim(), &feed).map_err(|e| e.to_string())?
    };
    let sem_capa = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        podcasts::capa(&conn, id).map_err(|e| e.to_string())?.is_none_or(|c| !std::path::Path::new(&c).is_file())
    };
    if let (true, Some(img)) = (sem_capa, &feed.imagem) {
        // Sem capa o programa funciona igual (usa a do episódio, se houver).
        let _ = guardar_capa_programa(app, id, img);
    }
    Ok((id, novos))
}

/// "Assinar por RSS": devolve o id do programa.
#[tauri::command]
async fn podcast_subscribe(app: AppHandle, url: String) -> Result<i64, String> {
    let h = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || atualizar_feed(&h, &url))
        .await
        .map_err(|e| e.to_string())??;
    let _ = app.emit(EVENTO_PODCASTS, ());
    Ok(r.0)
}

#[derive(Serialize)]
struct ResumoAtualizacao {
    novos: u32,
    falhas: Vec<String>,
}

/// "Atualizar": um programa (`id`) ou todos os assinados. Só baixa a lista, nunca os áudios.
#[tauri::command]
async fn podcast_refresh(app: AppHandle, id: Option<i64>) -> Result<ResumoAtualizacao, String> {
    let h = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<ResumoAtualizacao, String> {
        let lista: Vec<(i64, String)> = {
            let db = h.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            podcasts::assinaturas(&conn).map_err(|e| e.to_string())?
        };
        let mut resumo = ResumoAtualizacao { novos: 0, falhas: Vec::new() };
        for (_, url) in lista.into_iter().filter(|(sid, _)| id.is_none_or(|i| i == *sid)) {
            match atualizar_feed(&h, &url) {
                Ok((_, n)) => resumo.novos += n,
                Err(e) => resumo.falhas.push(format!("{url}: {e}")),
            }
        }
        Ok(resumo)
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = app.emit(EVENTO_PODCASTS, ());
    Ok(r)
}

/// Cancela a assinatura (os episódios baixados e os arquivos ficam).
#[tauri::command]
fn podcast_unsubscribe(app: AppHandle, id: i64) -> Result<(), String> {
    {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        podcasts::cancelar_assinatura(&conn, id).map_err(|e| e.to_string())?;
    }
    let _ = app.emit(EVENTO_PODCASTS, ());
    Ok(())
}

/// Capa baixada do feed, como data URL (ou nada).
#[tauri::command]
fn podcast_cover(db: State<db::Db>, id: i64) -> Result<Option<String>, String> {
    use base64::Engine;
    let caminho = {
        let conn = db.0.lock().map_err(erro_db)?;
        podcasts::capa(&conn, id).map_err(|e| e.to_string())?
    };
    Ok(caminho
        .and_then(|c| std::fs::read(c).ok())
        .map(|b| format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(b))))
}

#[derive(Serialize, Clone)]
struct ProgressoDownload {
    episodio: i64,
    feito: u64,
    total: Option<u64>,
}

/// Baixa o episódio para `<Música>/Skáld/Podcasts/<Programa>/` (pasta monitorada como
/// Podcasts) e liga o arquivo ao episódio. Progresso pelo evento `podcast-download`.
#[tauri::command]
async fn podcast_download(app: AppHandle, id: i64) -> Result<(), String> {
    let h = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let podcasts::ParaBaixar { programa, titulo, publicado_em: data, url, tipo } = {
            let db = h.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            podcasts::para_baixar(&conn, id).map_err(|e| e.to_string())?.ok_or("Este episódio já foi baixado.")?
        };
        let raiz = caminhos::musica(&h)?.join("Skáld").join("Podcasts");
        let pasta = raiz.join(podcasts::nome_seguro(&programa, 80));
        let ext = rss::extensao(&url, tipo.as_deref());
        let nome = podcasts::nome_arquivo(&titulo, data, ext);
        let mut destino = pasta.join(&nome);
        let mut n = 2;
        while destino.exists() {
            destino = pasta.join(nome.replacen(&format!(".{ext}"), &format!(" ({n}).{ext}"), 1));
            n += 1;
        }
        // A pasta raiz vira monitorada como Podcasts antes do arquivo aparecer nela.
        {
            let db = h.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            podcasts::marcar_pasta(&conn, &raiz.to_string_lossy()).map_err(|e| e.to_string())?;
        }
        h.state::<DownloadsState>().0.lock().map_err(erro_db)?.remove(&id);
        let _acordado = TarefaLonga::nova(&format!("Baixando {titulo}"));
        let mut ultimo = std::time::Instant::now();
        let emissor = h.clone();
        let baixado = rss::baixar(&url, &destino, |feito, total| {
            if ultimo.elapsed() >= std::time::Duration::from_millis(300) {
                ultimo = std::time::Instant::now();
                let _ = emissor.emit("podcast-download", &ProgressoDownload { episodio: id, feito, total });
                if let Some(t) = total {
                    TarefaLonga::texto(&format!("Baixando {titulo}: {}%", feito * 100 / t.max(1)));
                }
            }
            !emissor.state::<DownloadsState>().0.lock().is_ok_and(|c| c.contains(&id))
        });
        let _ = h.emit("podcast-download", &ProgressoDownload { episodio: id, feito: 0, total: Some(0) });
        let baixado = baixado?;
        std::fs::create_dir_all(&raiz).ok();
        h.state::<watch::Vigia>().vigiar(&raiz);
        import::importar_arquivos(&h.state::<db::Db>().0, std::slice::from_ref(&baixado))?;
        let db = h.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        let media: i64 = conn
            .query_row("SELECT id FROM media WHERE path = ?1", [baixado.to_string_lossy()], |r| r.get(0))
            .map_err(|_| "O arquivo baixado não pôde ser lido como áudio.".to_string())?;
        podcasts::ligar(&conn, id, media).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = app.emit(EVENTO_PODCASTS, ());
    r
}

#[tauri::command]
fn podcast_download_cancel(downloads: State<DownloadsState>, id: i64) -> Result<(), String> {
    downloads.0.lock().map_err(erro_db)?.insert(id);
    Ok(())
}

/// "Importar pasta como Podcasts" (etapa 54): a pasta e o que já estava importado dentro
/// dela passam à categoria Podcasts; cada subpasta (ou a própria pasta) vira um programa.
#[tauri::command]
async fn import_podcast_folder(app: tauri::AppHandle, path: String) -> Result<import::ImportResumo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !std::path::Path::new(&path).is_dir() {
            return Err("A pasta selecionada não existe.".to_string());
        }
        {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            podcasts::marcar_pasta(&conn, &path).map_err(|e| e.to_string())?;
        }
        let r = importar_pasta(&app, &path)?;
        let _ = app.emit(EVENTO_BIBLIOTECA, &r);
        Ok(r)
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
    if resumo.adicionados > 0 {
        if let Err(e) = podcasts::sincronizar(&app.state::<db::Db>().0) {
            eprintln!("Podcasts: {e}");
        }
        if let Err(e) = audiolivros::sincronizar(&app.state::<db::Db>().0) {
            eprintln!("Audiolivros: {e}");
        }
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
    // Episódios locais e livros sem arquivos saem junto.
    podcasts::sincronizar(&app.state::<db::Db>().0)?;
    audiolivros::sincronizar(&app.state::<db::Db>().0)?;
    let _ = app.emit(EVENTO_FILA, ());
    let _ = app.emit(EVENTO_PLAYLISTS, ());
    let _ = app.emit(EVENTO_HISTORICO, ());
    Ok(n)
}

/// Músicas marcadas como não encontradas (arquivo sumiu do disco).
#[tauri::command]
fn missing_count(db: State<db::Db>) -> Result<i64, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    conn.query_row("SELECT COUNT(*) FROM media WHERE missing = 1 AND category = 'music'", [], |r| r.get(0))
        .map_err(|e| e.to_string())
}

/// "Remover músicas não encontradas": tira da biblioteca só os registros (o arquivo já não existe).
#[tauri::command]
fn remove_missing(app: AppHandle) -> Result<usize, String> {
    let ids = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        let mut st = conn
            .prepare("SELECT id FROM media WHERE missing = 1 AND category = 'music'")
            .map_err(|e| e.to_string())?;
        st.query_map([], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    if ids.is_empty() {
        return Ok(0);
    }
    let n = remove_from_library(app.clone(), ids)?;
    let _ = app.emit(EVENTO_BIBLIOTECA, &import::ImportResumo::default());
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
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
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

/// Retomar ao reabrir: o que fica gravado na configuração `retomar`.
#[derive(Serialize, Deserialize)]
struct Retomar {
    fila: queue::Fila,
    posicao_ms: u64,
    origem: Option<Origem>,
}

#[derive(Default)]
struct RetomarControle {
    /// Último (faixa, estado) gravado e quando: grava ao mudar e a cada 5 s tocando.
    ultimo: Option<(Option<i64>, player::Estado)>,
    gravado_em: Option<std::time::Instant>,
    /// Faixa carregada pausada ao abrir: a sessão do histórico só começa quando tocar.
    sessao_pendente: Option<i64>,
}

struct RetomarState(Mutex<RetomarControle>);

/// Grava fila, posição e origem. Falhas são ignoradas (a reprodução não depende disso).
fn salvar_retomar(app: &AppHandle, posicao_ms: u64) {
    let Ok(fila) = app.state::<FilaState>().0.lock().map(|f| f.clone()) else { return };
    let Ok(origem) = app.state::<OrigemState>().0.lock().map(|o| *o) else { return };
    let Ok(json) = serde_json::to_string(&Retomar { fila, posicao_ms, origem }) else { return };
    if let Ok(conn) = app.state::<db::Db>().0.lock() {
        let _ = settings::definir_retomar(&conn, &json);
    }
}

/// Chamada a cada aviso do player: grava ao trocar de faixa ou de estado e a cada 5 s tocando.
fn acompanhar_retomar(app: &AppHandle, estado: &player::EstadoPlayer) {
    let estado_r = app.state::<RetomarState>();
    let Ok(mut c) = estado_r.0.lock() else { return };
    // Avisos de uma faixa que acabou de terminar: a próxima ainda vai ser escolhida.
    if estado.fim || estado.erro.is_some() {
        return;
    }
    let agora = (estado.id, estado.estado);
    let mudou = c.ultimo != Some(agora);
    let velho = c.gravado_em.is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(5));
    if !(mudou || (estado.estado == player::Estado::Tocando && velho)) {
        return;
    }
    c.ultimo = Some(agora);
    c.gravado_em = Some(std::time::Instant::now());
    drop(c);
    let (h, posicao, id, duracao) = (app.clone(), estado.posicao_ms, estado.id, estado.duracao_ms);
    // Fora da thread do barramento: o banco pode estar ocupado com uma importação.
    std::thread::spawn(move || {
        salvar_retomar(&h, posicao);
        // Podcast: a posição do episódio (retomar por episódio) e o "ouvido" automático.
        if let Some(id) = id {
            let virou = h.state::<db::Db>().0.lock().ok().and_then(|c| {
                podcasts::salvar_posicao(&c, id, posicao as i64, duracao as i64).ok()
            });
            if virou == Some(true) {
                let _ = h.emit(EVENTO_PODCASTS, ());
            }
            // Audiolivro: capítulo e posição onde parou.
            let terminou = h.state::<db::Db>().0.lock().ok().and_then(|c| {
                audiolivros::salvar_posicao(&c, id, posicao as i64, duracao as i64).ok()
            });
            if terminou == Some(true) {
                let _ = h.emit(EVENTO_AUDIOLIVROS, ());
            }
        }
    });
}

/// Ao abrir: volta a fila e deixa a última música pausada no ponto em que parou.
fn retomar_reproducao(app: &AppHandle) -> Result<(), String> {
    let db = app.state::<db::Db>();
    let json = {
        let conn = db.0.lock().map_err(erro_db)?;
        settings::retomar(&conn).map_err(|e| e.to_string())?
    };
    let Some(r) = json.and_then(|j| serde_json::from_str::<Retomar>(&j).ok()) else { return Ok(()) };
    let atual = r.fila.atual();
    {
        let fila = app.state::<FilaState>();
        let mut f = fila.0.lock().map_err(erro_db)?;
        // Aleatório e repetir seguem o que está nas configurações.
        let (aleatorio, repetir) = (f.aleatorio(), f.repetir());
        *f = r.fila;
        if f.aleatorio() != aleatorio {
            f.definir_aleatorio(aleatorio);
        }
        f.definir_repetir(repetir);
    }
    definir_origem(app, r.origem)?;
    let _ = app.emit(EVENTO_FILA, ());
    let Some(id) = atual else { return Ok(()) };
    let path = {
        let conn = db.0.lock().map_err(erro_db)?;
        match library::obter(&conn, id).map_err(|e| e.to_string())? {
            Some((_, path)) if std::path::Path::new(&path).is_file() => path,
            // Arquivo sumiu: a fila volta, e o play tenta de novo (e avisa).
            _ => return Ok(()),
        }
    };
    let ganho = ganho_de(app, id, &path);
    app.state::<player::Player>().definir_velocidade(velocidade_de(app, id));
    app.state::<RetomarState>().0.lock().map_err(erro_db)?.sessao_pendente = Some(id);
    app.state::<player::Player>().carregar_pausado(id, &path, ganho, r.posicao_ms)
}

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
            // Episódio que tocou até o fim: ouvido, e a retomada volta ao começo.
            if concluida && podcasts::concluir(&conn, s.media_id).unwrap_or(false) {
                let _ = app.emit(EVENTO_PODCASTS, ());
            }
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
    if estado.estado == player::Estado::Tocando {
        let pendente = app.state::<RetomarState>().0.lock().ok().and_then(|mut c| {
            c.sessao_pendente.take_if(|p| Some(*p) == estado.id)
        });
        if let Some(id) = pendente {
            iniciar_sessao(app, id);
        }
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
    /// A fila tinha acabado e `depois` já é a continuação ("Continuar tocando").
    continuou: bool,
}

struct GaplessState(Mutex<Option<Pendente>>);

/// Chamada pelo player pouco antes do fim da faixa: escolhe a seguinte numa cópia da fila
/// (respeitando fila manual, aleatório e repetir) e pula arquivos ausentes.
/// Fim da fila (sem repetir): com "Continuar tocando", as músicas que seguem depois de
/// `ultima` (mesmo artista, mesmo gênero, biblioteca inteira).
fn continuacao(app: &AppHandle, ultima: Option<i64>) -> Option<Vec<i64>> {
    let db = app.state::<db::Db>();
    let conn = db.0.lock().ok()?;
    // Audiolivro: o capítulo seguinte, sempre (é a continuação do mesmo livro); no fim, para.
    if let Some(u) = ultima.filter(|&u| audiolivros::e_audiolivro(&conn, u)) {
        return Some(audiolivros::proximos(&conn, u)).filter(|ids| !ids.is_empty());
    }
    if !settings::carregar(&conn).ok()?.continuar_tocando {
        return None;
    }
    let ultima = ultima?;
    // Podcast: o próximo não ouvido do mesmo programa (nunca músicas relacionadas).
    if podcasts::e_podcast(&conn, ultima) {
        return podcasts::proximos(&conn, ultima).ok().filter(|ids| !ids.is_empty());
    }
    colecao::continuacao(&conn, ultima).ok().filter(|ids| !ids.is_empty())
}

fn preparar_proxima(app: &AppHandle) -> Option<player::Proxima> {
    // Temporizador "fim desta música": nada é emendado; a faixa termina e para.
    if app.state::<TemporizadorState>().0.lock().ok()?.fim_da_faixa() {
        return None;
    }
    let antes = app.state::<FilaState>().0.lock().ok()?.clone();
    let mut depois = antes.clone();
    let mut continuou = false;
    for tentativa in 0..100 {
        let id = match if tentativa == 0 { depois.proxima_automatica() } else { depois.proxima() } {
            Some(id) => id,
            None if !continuou => {
                continuou = true;
                let ids = continuacao(app, depois.atual())?;
                depois.continuar_com(ids)?
            }
            None => return None,
        };
        let proxima = {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().ok()?;
            let (musica, path) = library::obter(&conn, id).ok()??;
            let existe = std::path::Path::new(&path).is_file();
            if existe == musica.missing {
                let _ = library::marcar_ausente(&conn, id, !existe);
            }
            // Podcast (o atual ou o próximo): sem crossfade nem emenda; a faixa termina e o
            // próximo começa pelo `tocar_id` (retomada e velocidade de cada episódio).
            if antes.atual().is_some_and(|a| falado(&conn, a)) || falado(&conn, id) {
                return None;
            }
            let mesmo_album = antes.atual().is_some_and(|a| library::mesmo_album(&conn, a, id).unwrap_or(false));
            existe.then_some((id, path, mesmo_album))
        };
        if let Some((id, path, mesmo_album)) = proxima {
            let ganho = ganho_de(app, id, &path);
            let p = player::Proxima { id, path, mesmo_album, ganho };
            *app.state::<GaplessState>().0.lock().ok()? = Some(Pendente { antes, depois, id, continuou });
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
                if p.continuou {
                    // Faixas automáticas: a playlist de onde veio a fila não ganha nada.
                    let _ = definir_origem(app, None);
                }
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
    let mut g = if g.faixa.is_some() || g.album.is_some() {
        g
    } else {
        app.state::<db::Db>().0.lock().map(|c| rganalise::do_banco(&c, id)).unwrap_or_default()
    };
    // Podcast: sem ReplayGain de álbum (cada episódio é um "álbum" à parte).
    if app.state::<db::Db>().0.lock().is_ok_and(|c| podcasts::e_podcast(&c, id)) {
        g.album = None;
    }
    g
}

/// Podcast ou audiolivro: velocidade, −15/+30 s, retomada e nada de crossfade ou emenda.
fn falado(conn: &rusqlite::Connection, id: i64) -> bool {
    podcasts::e_podcast(conn, id) || audiolivros::e_audiolivro(conn, id)
}

/// Velocidade de reprodução: a da fala para podcasts e audiolivros, 1× para música.
fn velocidade_de(app: &AppHandle, id: i64) -> f64 {
    let db = app.state::<db::Db>();
    let Ok(conn) = db.0.lock() else { return 1.0 };
    if falado(&conn, id) {
        settings::podcast_velocidade(&conn).unwrap_or(1.0)
    } else {
        1.0
    }
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
    if let Ok(mut c) = app.state::<RetomarState>().0.lock() {
        c.sessao_pendente = None;
    }
    let ganho = ganho_de(app, id, &path);
    let player = app.state::<player::Player>();
    player.definir_velocidade(velocidade_de(app, id));
    // Episódio começado e não ouvido: continua de onde parou.
    let retomar = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        podcasts::de_media(&conn, id)
            .ok()
            .flatten()
            .and_then(|(_, _, pos, ouvido)| pos.filter(|&p| !ouvido && p > 5_000))
            .or_else(|| audiolivros::retomada(&conn, id))
    };
    match retomar {
        Some(pos) => player.tocar_em(id, &path, ganho, pos as u64)?,
        None => player.tocar(id, &path, ganho)?,
    }
    iniciar_sessao(app, id);
    Ok(())
}

/// Toca a próxima da fila, pulando arquivos ausentes. Sem próxima, para.
/// `automatica`: a faixa terminou sozinha (aí "repetir faixa" vale).
fn avancar(app: &AppHandle, automatica: bool) -> Result<(), String> {
    let fila = app.state::<FilaState>();
    let mut resultado = Ok(());
    let mut continuou = false;
    for tentativa in 0..100 {
        let mut f = fila.0.lock().map_err(erro_db)?;
        let ultima = f.atual();
        let mut proxima = if automatica && tentativa == 0 { f.proxima_automatica() } else { f.proxima() };
        drop(f);
        // Fila acabou (repetir desligado): "Continuar tocando" com relacionadas.
        if proxima.is_none() && !continuou {
            continuou = true;
            if let Some(ids) = continuacao(app, ultima) {
                proxima = fila.0.lock().map_err(erro_db)?.continuar_com(ids);
                // Faixas automáticas: a playlist de onde veio a fila não ganha nada.
                definir_origem(app, None)?;
            }
        }
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
    Ok(crate::caminhos::dados(&app)?.join("capas"))
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
    // No Android o seletor devolve um endereço content:// (sem extensão): o plugin fs lê
    // os dois casos, e o formato vem do conteúdo da imagem.
    use tauri_plugin_fs::FsExt;
    let caminho: tauri_plugin_fs::FilePath = origem.parse().unwrap_or_else(|e| match e {});
    let bytes = app.fs().read(caminho).map_err(|_| "Imagem não encontrada.")?;
    if bytes.len() > details::CAPA_MAX {
        return Err("A imagem é grande demais (máximo de 8 MB).".into());
    }
    let ext = match image::guess_format(&bytes) {
        Ok(image::ImageFormat::Jpeg) => "jpg",
        Ok(image::ImageFormat::Png) => "png",
        Ok(image::ImageFormat::WebP) => "webp",
        _ => return Err("Escolha uma imagem JPG, PNG ou WEBP.".into()),
    };
    let pasta = pasta_capas(&app)?;
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let destino = pasta.join(format!("playlist-{id}-{agora}.{ext}"));
    std::fs::write(&destino, &bytes).map_err(|e| format!("Não foi possível copiar a imagem: {e}"))?;
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
fn playlist_folders(db: State<db::Db>) -> Result<Vec<playlists::Pasta>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    playlists::pastas(&conn)
}

#[tauri::command]
fn playlist_folder_create(app: AppHandle, nome: String) -> Result<i64, String> {
    alterar_playlists(&app, |conn| playlists::criar_pasta(conn, &nome))
}

#[tauri::command]
fn playlist_folder_rename(app: AppHandle, id: i64, nome: String) -> Result<(), String> {
    alterar_playlists(&app, |conn| playlists::renomear_pasta(conn, id, &nome))
}

#[tauri::command]
fn playlist_folder_delete(app: AppHandle, id: i64) -> Result<(), String> {
    alterar_playlists(&app, |conn| playlists::excluir_pasta(conn, id))
}

#[tauri::command]
fn playlist_set_folder(app: AppHandle, id: i64, pasta: Option<i64>) -> Result<(), String> {
    alterar_playlists(&app, |conn| playlists::mover_para_pasta(conn, id, pasta))
}

/// Cria (`id` nulo) ou altera uma playlist inteligente; devolve o id.
#[tauri::command]
fn smart_playlist_save(app: AppHandle, id: Option<i64>, nome: String, regras: inteligentes::Regras) -> Result<i64, String> {
    alterar_playlists(&app, |conn| inteligentes::salvar(conn, id, &nome, &regras))
}

#[tauri::command]
fn smart_playlist_rules(db: State<db::Db>, id: i64) -> Result<Option<inteligentes::Regras>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    inteligentes::regras(&conn, id)
}

/// Quantas músicas as regras escolhem agora (prévia no editor).
#[tauri::command]
fn smart_playlist_preview(db: State<db::Db>, regras: inteligentes::Regras) -> Result<usize, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    inteligentes::escolher(&conn, &regras, 0).map(|v| v.len())
}

/// Resultado de importar um M3U: a playlist criada e as músicas que não estão na biblioteca.
#[derive(Serialize)]
struct M3uImportado {
    id: i64,
    nome: String,
    achadas: usize,
    faltando: Vec<String>,
}

/// Cria uma playlist a partir de um M3U/M3U8 (caminho do disco ou content:// no Android).
/// Só entram músicas que já estão na biblioteca; nada é copiado nem importado.
#[tauri::command]
fn playlist_import_m3u(app: AppHandle, origem: String) -> Result<M3uImportado, String> {
    use tauri_plugin_fs::FsExt;
    let caminho: tauri_plugin_fs::FilePath = origem.parse().unwrap_or_else(|e| match e {});
    let bytes = app.fs().read(caminho).map_err(|_| "Não foi possível ler a playlist.")?;
    let texto = playlists::texto_m3u(&bytes);
    let local = (!e_content(&origem)).then(|| std::path::Path::new(&origem));
    let entradas = playlists::ler_m3u(&texto, local.and_then(|p| p.parent()));
    if entradas.is_empty() {
        return Err("O arquivo não tem nenhuma música.".into());
    }
    // Nome = nome do arquivo (no content:// do Android ele vem codificado no fim do endereço).
    let fim = origem.rsplit(['/', '\\']).next().unwrap_or_default();
    let fim = playlists::decodificar_url(fim);
    let fim = fim.rsplit(['/', ':']).next().unwrap_or_default();
    let nome = fim.rsplit_once('.').map_or(fim, |(n, _)| n).trim();
    let nome = if nome.is_empty() { "Playlist importada" } else { nome }.to_string();
    let mut faltando = Vec::new();
    let mut achadas = 0;
    let mut id = 0;
    alterar_playlists(&app, |conn| {
        let (ids, f) = playlists::casar_m3u(conn, &entradas)?;
        if ids.is_empty() {
            return Err("Nenhuma música dessa playlist está na biblioteca.".into());
        }
        id = playlists::criar(conn, &nome)?;
        playlists::adicionar(conn, id, &ids)?;
        achadas = ids.len();
        faltando = f;
        Ok(())
    })?;
    Ok(M3uImportado { id, nome, achadas, faltando })
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
        let pasta = miniaturas::pasta(crate::caminhos::cache(&app)?);
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

/// Clique numa música da fila: `manual` = "Próximas na fila", senão "A seguir".
#[tauri::command]
fn queue_jump(app: AppHandle, fila: State<FilaState>, manual: bool, posicao: usize) -> Result<(), String> {
    let id = {
        let mut f = fila.0.lock().map_err(erro_db)?;
        if manual { f.pular_manual(posicao) } else { f.pular_contexto(posicao) }
    };
    let _ = app.emit(EVENTO_FILA, ());
    match id {
        Some(id) => tocar_id(&app, id),
        None => Ok(()),
    }
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
fn toggle_pause(app: AppHandle, player: State<player::Player>) -> Result<(), String> {
    // Retomada sem o arquivo carregado (sumiu ao abrir): tenta tocar a atual da fila.
    if player.estado().id.is_none() {
        if let Some(id) = app.state::<FilaState>().0.lock().map_err(erro_db)?.atual() {
            return tocar_id(&app, id);
        }
    }
    player.alternar_pausa()
}

/// Estado do player agora (a interface pede ao abrir; os avisos vêm depois).
#[tauri::command]
fn player_state(player: State<player::Player>) -> player::EstadoPlayer {
    player.estado()
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
fn backup_export(app: AppHandle, destino: String) -> Result<(), String> {
    let db = app.state::<db::Db>();
    if !e_content(&destino) {
        let conn = db.0.lock().map_err(erro_db)?;
        return backup::exportar(&conn, std::path::Path::new(&destino));
    }
    // Android: o seletor devolve content://; exporta num arquivo temporário e copia pelo plugin fs.
    let tmp = backup_temporario(&app)?;
    {
        let conn = db.0.lock().map_err(erro_db)?;
        backup::exportar(&conn, &tmp)?;
    }
    let r = (|| {
        use tauri_plugin_fs::FsExt;
        let mut opcoes = tauri_plugin_fs::OpenOptions::new();
        opcoes.write(true).create(true).truncate(true);
        let caminho: tauri_plugin_fs::FilePath = destino.parse().unwrap_or_else(|e| match e {});
        let mut saida = app.fs().open(caminho, opcoes).map_err(|e| format!("Não foi possível salvar o backup: {e}"))?;
        let mut entrada = std::fs::File::open(&tmp).map_err(|e| e.to_string())?;
        std::io::copy(&mut entrada, &mut saida).map_err(|e| format!("Não foi possível salvar o backup: {e}"))?;
        saida.sync_all().ok();
        Ok(())
    })();
    let _ = std::fs::remove_file(&tmp);
    r
}

/// Endereço do seletor de arquivos do Android (não é um caminho do disco).
fn e_content(caminho: &str) -> bool {
    caminho.starts_with("content://")
}

/// Arquivo temporário (no cache) para passar backups de/para endereços content://.
fn backup_temporario(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let pasta = crate::caminhos::cache(app)?;
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    let tmp = pasta.join("backup-temporario.db");
    let _ = std::fs::remove_file(&tmp);
    Ok(tmp)
}

/// Restaura um backup: a fila é esvaziada (os ids podem ser outros) e a tela recarrega.
/// O banco anterior fica salvo ao lado do atual; o caminho volta para avisar o usuário.
#[tauri::command]
fn backup_restore(app: AppHandle, origem: String) -> Result<String, String> {
    let pasta = crate::caminhos::dados(&app)?;
    let seguranca = backup::copia_de_seguranca(&pasta, unix_agora());
    encerrar_sessao(&app, false);
    {
        let db = app.state::<db::Db>();
        let mut conn = db.0.lock().map_err(erro_db)?;
        if e_content(&origem) {
            // Android: copia o backup escolhido para um arquivo temporário antes de restaurar.
            use tauri_plugin_fs::FsExt;
            let caminho: tauri_plugin_fs::FilePath = origem.parse().unwrap_or_else(|e| match e {});
            let bytes = app.fs().read(caminho).map_err(|e| format!("Não foi possível ler o backup: {e}"))?;
            let tmp = backup_temporario(&app)?;
            std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
            let r = backup::restaurar(&mut conn, &tmp, &seguranca);
            let _ = std::fs::remove_file(&tmp);
            r?;
        } else {
            backup::restaurar(&mut conn, std::path::Path::new(&origem), &seguranca)?;
        }
    }
    *app.state::<FilaState>().0.lock().map_err(erro_db)? = queue::Fila::default();
    definir_origem(&app, None)?;
    let _ = app.emit(EVENTO_FILA, ());
    Ok(seguranca.display().to_string())
}

fn pastas_de_cache(app: &AppHandle) -> Result<Vec<std::path::PathBuf>, String> {
    let cache = crate::caminhos::cache(&app)?;
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

const EVENTO_CATEGORIAS: &str = "categorias-mudou";

#[tauri::command]
fn categories_list(db: State<db::Db>) -> Result<Vec<categorias::Categoria>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    categorias::listar(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn category_name(db: State<db::Db>, id: i64) -> Result<Option<String>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    categorias::nome(&conn, id).map_err(|e| e.to_string())
}

/// Músicas de uma categoria, ou todas (`None`) com as categorias de cada uma.
#[tauri::command]
fn category_tracks(db: State<db::Db>, id: Option<i64>) -> Result<Vec<categorias::MusicaCategoria>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    categorias::musicas(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn media_categories(db: State<db::Db>, id: i64) -> Result<Vec<String>, String> {
    let conn = db.0.lock().map_err(erro_db)?;
    categorias::da_musica(&conn, id).map_err(|e| e.to_string())
}

/// Sugestões de categoria (internet: MusicBrainz, uma consulta por segundo). Não grava nada.
#[tauri::command]
async fn category_suggest(app: AppHandle, id: i64) -> Result<categorias::Sugestao, String> {
    tauri::async_runtime::spawn_blocking(move || categorias::sugerir(&app.state::<db::Db>().0, id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn category_add(app: AppHandle, db: State<db::Db>, id: i64, nomes: Vec<String>, sugestao: bool) -> Result<usize, String> {
    let mut conn = db.0.lock().map_err(erro_db)?;
    let n = categorias::adicionar(&mut conn, id, &nomes, if sugestao { "sugestao" } else { "manual" }).map_err(|e| e.to_string())?;
    let _ = app.emit(EVENTO_CATEGORIAS, ());
    Ok(n)
}

#[tauri::command]
fn category_remove(app: AppHandle, db: State<db::Db>, id: i64, nome: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    categorias::remover(&conn, id, &nome).map_err(|e| e.to_string())?;
    let _ = app.emit(EVENTO_CATEGORIAS, ());
    Ok(())
}

#[tauri::command]
fn category_delete(app: AppHandle, db: State<db::Db>, id: i64) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    categorias::excluir(&conn, id).map_err(|e| e.to_string())?;
    let _ = app.emit(EVENTO_CATEGORIAS, ());
    Ok(())
}

#[tauri::command]
fn category_rename(app: AppHandle, db: State<db::Db>, id: i64, nome: String) -> Result<(), String> {
    let mut conn = db.0.lock().map_err(erro_db)?;
    categorias::renomear(&mut conn, id, &nome)?;
    let _ = app.emit(EVENTO_CATEGORIAS, ());
    Ok(())
}

/// Toca a categoria a partir da música `indice` (ordem da biblioteca).
#[tauri::command]
fn play_category(app: AppHandle, fila: State<FilaState>, id: i64, indice: usize) -> Result<(), String> {
    let ids = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        categorias::ids(&conn, id).map_err(|e| e.to_string())?
    };
    tocar_lista(&app, &fila, ids, indice)
}

/// O reconhecimento pelo som (AcoustID) existe nesta versão?
#[tauri::command]
fn acoustic_available() -> bool {
    acustico::disponivel()
}

/// Reconhece a música pelo som (AcoustID + MusicBrainz). Internet; uns segundos por música.
#[tauri::command]
async fn metadata_recognize(app: AppHandle, id: i64) -> Result<Vec<enriquecer::Candidato>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (caminho, duracao): (String, Option<i64>) = {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            conn.query_row("SELECT path, duration_ms FROM media WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|_| "Música não encontrada.".to_string())?
        };
        acustico::reconhecer(std::path::Path::new(&caminho), duracao)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Renomeia o arquivo para "Artista - Título.ext" com os dados do banco (nunca sobrescreve).
#[tauri::command]
async fn media_rename(app: AppHandle, id: i64) -> Result<String, String> {
    let h = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || enriquecer::renomear_arquivo(&h.state::<db::Db>().0, id))
        .await
        .map_err(|e| e.to_string())??;
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
    tauri::async_runtime::spawn_blocking(move || enriquecer::buscar_flexivel(&titulo, artista.as_deref(), duracao_ms))
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

#[cfg(desktop)]
const ID_BANDEJA: &str = "principal";

#[cfg(desktop)]
fn mostrar_janela(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Mostra ou esconde o ícone da bandeja (criado na primeira vez). Sem a biblioteca de
/// indicadores (libayatana-appindicator3), a criação falha sem derrubar o app.
#[cfg(desktop)]
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

/// No celular não há bandeja: ligar falha e a opção fica desligada.
#[cfg(mobile)]
fn definir_bandeja(_app: &AppHandle, ativo: bool) -> Result<(), String> {
    if ativo { Err("Bandeja indisponível neste sistema.".into()) } else { Ok(()) }
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
fn set_continue(db: State<db::Db>, ativo: bool) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    settings::definir_continuar(&conn, ativo).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_sync_upload(db: State<db::Db>, ativo: bool) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    settings::definir_sync_enviar(&conn, ativo).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_update_check(db: State<db::Db>, ativo: bool) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    settings::definir_verificar_versao(&conn, ativo).map_err(|e| e.to_string())
}

/// Versão nova no GitHub, só com a opção ligada; desligada ou sem internet devolve nada.
/// Conta do Google Drive (sincronização): disponível nesta versão, conectada, e-mail.
#[tauri::command]
fn sync_status(app: AppHandle) -> Result<drive::Estado, String> {
    Ok(drive::estado(&caminhos::dados(&app)?))
}

/// Enquanto existir, o Android mantém o app acordado com uma notificação (SyncService);
/// sem isso a Samsung congela o processo em segundo plano. Nas outras plataformas não faz nada.
/// Várias ao mesmo tempo (download e busca em lote, por exemplo) contam juntas: a notificação
/// só sai quando a última termina.
struct TarefaLonga;

static TAREFAS_LONGAS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl TarefaLonga {
    fn nova(texto: &str) -> Self {
        TAREFAS_LONGAS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self::texto(texto);
        TarefaLonga
    }

    fn texto(_texto: &str) {
        #[cfg(target_os = "android")]
        mpris::segundo_plano(Some(_texto));
    }
}

impl Drop for TarefaLonga {
    fn drop(&mut self) {
        if TAREFAS_LONGAS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1 {
            #[cfg(target_os = "android")]
            mpris::segundo_plano(None);
        }
    }
}

/// Tarefas longas abertas pela tela (buscas em lote do Completar e das Categorias), por id.
#[derive(Default)]
struct TarefasDaTela(Mutex<(u32, std::collections::HashMap<u32, TarefaLonga>)>);

/// A tela começa um trabalho que deve seguir com o app em segundo plano.
#[tauri::command]
fn long_task_start(tarefas: State<TarefasDaTela>, texto: String) -> Result<u32, String> {
    let mut t = tarefas.0.lock().map_err(erro_db)?;
    t.0 += 1;
    let id = t.0;
    t.1.insert(id, TarefaLonga::nova(&texto));
    Ok(id)
}

#[tauri::command]
fn long_task_text(texto: String) {
    TarefaLonga::texto(&texto);
}

#[tauri::command]
fn long_task_end(tarefas: State<TarefasDaTela>, id: u32) -> Result<(), String> {
    tarefas.0.lock().map_err(erro_db)?.1.remove(&id);
    Ok(())
}

/// Abre o navegador para entrar com o Google e espera a autorização (ou "Cancelar").
#[tauri::command]
async fn sync_login(app: AppHandle) -> Result<drive::Estado, String> {
    use tauri_plugin_opener::OpenerExt;
    let dados = caminhos::dados(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _acordado = TarefaLonga::nova("Entrando com o Google…");
        drive::entrar(&dados, |url| app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Resumo de "Sincronizar agora" (músicas; a pasta de destino aparece na tela).
#[derive(Serialize)]
struct SyncResultado {
    #[serde(flatten)]
    resumo: drive::ResumoSync,
    pasta: String,
    /// Playlists, escutas e apelidos que mudaram neste aparelho (nulo se parou antes).
    dados: Option<sincdados::Mudancas>,
}

/// "Sincronizar agora": baixa as músicas que faltam aqui para `<Música>/Skáld/Drive` (vira pasta
/// monitorada) e, se `sync_enviar`, sobe as que faltam no Drive. Progresso pelo evento `sincronizacao-progresso`.
#[tauri::command]
async fn sync_now(app: AppHandle) -> Result<SyncResultado, String> {
    let emissor = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || sincronizar_agora(app))
        .await
        .map_err(|e| e.to_string())?;
    // A tela pode ter saído e voltado: o resultado também vai por evento.
    let _ = emissor.emit(
        "sincronizacao-fim",
        &match &r {
            Ok(v) => serde_json::json!({ "ok": v }),
            Err(e) => serde_json::json!({ "erro": e }),
        },
    );
    r
}

fn sincronizar_agora(app: AppHandle) -> Result<SyncResultado, String> {
    {
        let dados = caminhos::dados(&app)?;
        let destino = caminhos::musica(&app)?
            .join("Skáld")
            .join("Drive");
        // Hash das músicas que ainda não têm (as antigas e as recém-importadas).
        import::preencher_hashes(&app.state::<db::Db>().0)?;
        let (locais, conhecidos, enviar) = {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            let mut st = conn
                .prepare(
                    "SELECT file_hash, path FROM media
                     WHERE missing = 0 AND category = 'music' AND file_hash IS NOT NULL",
                )
                .map_err(|e| e.to_string())?;
            let locais = st
                .query_map([], |r| {
                    Ok(drive::Local { hash: r.get(0)?, caminho: std::path::PathBuf::from(r.get::<_, String>(1)?) })
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            let mut st = conn
                .prepare("SELECT file_hash FROM media WHERE file_hash IS NOT NULL")
                .map_err(|e| e.to_string())?;
            let conhecidos = st
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<std::collections::HashSet<_>, _>>()
                .map_err(|e| e.to_string())?;
            let enviar = settings::sync_enviar(&conn).map_err(|e| e.to_string())?;
            (locais, conhecidos, enviar)
        };
        let _acordado = TarefaLonga::nova("Sincronizando com o Google Drive…");
        let emissor = app.clone();
        let (resumo, baixados) = drive::sincronizar_musicas(&dados, locais, conhecidos, &destino, enviar, move |p| {
            if p.total > 0 {
                let verbo = if p.fase == "subindo" { "Enviando" } else { "Baixando" };
                TarefaLonga::texto(&format!("{verbo} {} de {}: {}", p.feito + 1, p.total, p.nome));
            }
            let _ = emissor.emit("sincronizacao-progresso", &p);
        })?;
        if !baixados.is_empty() {
            let r = importar_pasta(&app, &destino.to_string_lossy())?;
            if r.mudou_algo() {
                let _ = app.emit(EVENTO_BIBLIOTECA, &r);
            }
        }
        // Depois das músicas (as playlists já acham as baixadas): playlists, curtidas,
        // histórico e apelidos.
        let mut mudancas = None;
        if !resumo.cancelado {
            let _ = app.emit(
                "sincronizacao-progresso",
                &drive::Progresso { fase: "dados", feito: 0, total: 0, nome: String::new() },
            );
            TarefaLonga::texto("Sincronizando playlists e histórico…");
            let aparelho = {
                let db = app.state::<db::Db>();
                let conn = db.0.lock().map_err(erro_db)?;
                settings::aparelho_id(&conn).map_err(|e| e.to_string())?
            };
            drive::sincronizar_dados(&dados, &aparelho, |textos| {
                let (meu, m) = trocar_dados(&app, textos)?;
                mudancas = Some(m);
                Ok(meu)
            })?;
            if let Some(m) = &mudancas {
                avisar_dados(&app, m);
            }
        }
        Ok(SyncResultado { resumo, pasta: destino.to_string_lossy().into_owned(), dados: mudancas })
    }
}

/// Junta os arquivos dos outros aparelhos com o banco, aplica e devolve o deste aparelho.
fn trocar_dados(app: &AppHandle, textos: Vec<String>) -> Result<(String, sincdados::Mudancas), String> {
    let db = app.state::<db::Db>();
    let mut conn = db.0.lock().map_err(erro_db)?;
    let mut todos = vec![sincdados::exportar(&conn)?];
    for t in textos {
        // Arquivo de versão mais nova ou estragado: ignora (não apaga nada).
        match serde_json::from_str::<sincdados::Dados>(&t) {
            Ok(d) if d.versao <= sincdados::VERSAO => todos.push(d),
            _ => {}
        }
    }
    let m = sincdados::aplicar(&mut conn, &sincdados::mesclar(&todos))?;
    let meu = serde_json::to_string(&sincdados::exportar(&conn)?).map_err(|e| e.to_string())?;
    Ok((meu, m))
}

fn avisar_dados(app: &AppHandle, m: &sincdados::Mudancas) {
    if m.playlists > 0 {
        let _ = app.emit(EVENTO_PLAYLISTS, ());
    }
    if m.escutas > 0 {
        let _ = app.emit(EVENTO_HISTORICO, ());
    }
}

/// Dados por pasta (Syncthing etc.): nada configurado devolve `None`.
fn sincronizar_pasta(app: &AppHandle) -> Result<Option<sincdados::Mudancas>, String> {
    let (pasta, aparelho) = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        let Some(pasta) = settings::sync_pasta(&conn).map_err(|e| e.to_string())? else { return Ok(None) };
        (pasta, settings::aparelho_id(&conn).map_err(|e| e.to_string())?)
    };
    let mut mudancas = sincdados::Mudancas::default();
    sincpasta::sincronizar(std::path::Path::new(&pasta), &aparelho, |textos| {
        let (meu, m) = trocar_dados(app, textos)?;
        mudancas = m;
        Ok(meu)
    })?;
    avisar_dados(app, &mudancas);
    Ok(Some(mudancas))
}

/// Ao abrir e a cada 5 minutos com a pasta configurada (só arquivos locais, sem internet).
fn vigiar_pasta_dados(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(10));
        loop {
            if let Err(e) = sincronizar_pasta(&app) {
                eprintln!("sincronização por pasta: {e}");
            }
            std::thread::sleep(std::time::Duration::from_secs(300));
        }
    });
}

/// Escolhe (ou desliga, com `None`) a pasta dos dados e já sincroniza.
#[tauri::command]
async fn set_sync_folder(app: AppHandle, pasta: Option<String>) -> Result<Option<sincdados::Mudancas>, String> {
    {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        settings::definir_sync_pasta(&conn, pasta.as_deref()).map_err(|e| e.to_string())?;
    }
    tauri::async_runtime::spawn_blocking(move || sincronizar_pasta(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn sync_folder_now(app: AppHandle) -> Result<Option<sincdados::Mudancas>, String> {
    tauri::async_runtime::spawn_blocking(move || sincronizar_pasta(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn sync_cancel() {
    drive::cancelar();
}

/// Desconecta a conta neste aparelho (nada no Drive é apagado).
#[tauri::command]
async fn sync_logout(app: AppHandle) -> Result<drive::Estado, String> {
    let dados = caminhos::dados(&app)?;
    tauri::async_runtime::spawn_blocking(move || drive::sair(&dados))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn update_check(db: State<'_, db::Db>) -> Result<Option<novidades::NovaVersao>, String> {
    let ligado = {
        let conn = db.0.lock().map_err(erro_db)?;
        settings::verificar_versao(&conn).map_err(|e| e.to_string())?
    };
    if !ligado {
        return Ok(None);
    }
    tauri::async_runtime::spawn_blocking(novidades::verificar)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn set_notifications(db: State<db::Db>, ativo: bool) -> Result<(), String> {
    let conn = db.0.lock().map_err(erro_db)?;
    settings::definir_notificar(&conn, ativo).map_err(|e| e.to_string())
}

#[tauri::command]
fn autostart_get(app: AppHandle) -> Result<bool, String> {
    #[cfg(windows)]
    {
        let _ = app;
        Ok(sistema::registro::ativo())
    }
    #[cfg(mobile)]
    {
        let _ = app;
        Ok(false)
    }
    #[cfg(all(desktop, not(windows)))]
    Ok(sistema::autostart_ativo(&app.path().config_dir().map_err(|e| e.to_string())?))
}

#[tauri::command]
fn set_autostart(app: AppHandle, ativo: bool) -> Result<(), String> {
    #[cfg(mobile)]
    {
        let _ = (app, ativo);
        return Err("Iniciar com o sistema não está disponível neste sistema.".into());
    }
    #[cfg(desktop)]
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        let _ = app;
        sistema::registro::definir(&exe, ativo)
    }
    #[cfg(all(desktop, not(windows)))]
    sistema::definir_autostart(&app.path().config_dir().map_err(|e| e.to_string())?, &exe, ativo)
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
        let capa = crate::caminhos::cache(&app)
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

/// Saídas de áudio disponíveis agora e a escolhida (`None` = padrão do sistema).
#[derive(Serialize)]
struct Saidas {
    lista: Vec<saida::Saida>,
    escolhida: Option<String>,
}

#[tauri::command]
async fn audio_outputs(app: AppHandle) -> Result<Saidas, String> {
    let escolhida = {
        let db = app.state::<db::Db>();
        let conn = db.0.lock().map_err(erro_db)?;
        settings::saida(&conn).map_err(|e| e.to_string())?
    };
    let lista = tauri::async_runtime::spawn_blocking(saida::listar).await.map_err(|e| e.to_string())?;
    Ok(Saidas { lista, escolhida })
}

/// Escolhe a saída de áudio (`None` = padrão do sistema); vale na hora.
#[tauri::command]
async fn set_audio_output(app: AppHandle, chave: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let elementos = match &chave {
            Some(c) => Some(saida::criar(c).ok_or("Essa saída de áudio não está mais disponível.")?),
            None => None,
        };
        {
            let db = app.state::<db::Db>();
            let conn = db.0.lock().map_err(erro_db)?;
            settings::definir_saida(&conn, chave.as_deref()).map_err(|e| e.to_string())?;
        }
        app.state::<player::Player>().definir_saida(elementos);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Escolhe o preset; com `bandas`, grava também o ajuste do personalizado.
#[tauri::command]
fn set_equalizer(
    db: State<db::Db>,
    player: State<player::Player>,
    preset: eq::Preset,
    bandas: Option<eq::Bandas>,
) -> Result<(), String> {
    let personalizadas = {
        let conn = db.0.lock().map_err(erro_db)?;
        settings::definir_equalizador(&conn, preset).map_err(|e| e.to_string())?;
        match bandas {
            Some(b) => settings::definir_bandas(&conn, b).map_err(|e| e.to_string())?,
            None => settings::carregar(&conn).map_err(|e| e.to_string())?.equalizador_bandas,
        }
    };
    player.definir_equalizador(preset.efetivas(&personalizadas));
    Ok(())
}

#[tauri::command]
fn seek(player: State<player::Player>, posicao_ms: u64) -> Result<(), String> {
    player.buscar(posicao_ms)
}

/// Windows instalado: o GStreamer vem junto (DLLs ao lado do .exe, plugins em
/// `gstreamer-1.0/`). Usa só esses plugins, com registro próprio, para não misturar
/// com um GStreamer instalado no sistema. Em `tauri dev` o Tauri também copia os recursos
/// para `target\debug`, então o dev usa o mesmo conjunto do instalador.
#[cfg(windows)]
fn gstreamer_embutido() {
    let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) else { return };
    let plugins = dir.join("gstreamer-1.0");
    if !plugins.is_dir() {
        return;
    }
    let registro = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| dir.clone())
        .join("com.skald.app")
        .join("gstreamer-registry.bin");
    // SAFETY: chamado no início de `run`, antes de qualquer outra thread existir.
    unsafe {
        std::env::set_var("GST_PLUGIN_SYSTEM_PATH_1_0", &plugins);
        std::env::remove_var("GST_PLUGIN_PATH_1_0");
        std::env::remove_var("GST_PLUGIN_PATH");
        std::env::set_var("GST_REGISTRY_1_0", registro);
        // Sem gst-plugin-scanner.exe: a varredura (só na primeira vez) roda no próprio processo.
        std::env::set_var("GST_REGISTRY_FORK", "no");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(windows)]
    gstreamer_embutido();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let dir = caminhos::dados(app.handle())?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("skald.db"))?;
            app.manage(db::Db(Mutex::new(conn)));
            details::definir_pasta_capas(dir.join("capas"));
            letras::definir_pasta(dir.join("letras"));
            app.manage(FilaState(Mutex::new(queue::Fila::default())));
            app.manage(HistoricoState(Mutex::new(None)));
            app.manage(OrigemState(Mutex::new(None)));
            app.manage(DownloadsState(Mutex::new(Default::default())));
            app.manage(TarefasDaTela::default());
            app.manage(RetomarState(Mutex::new(RetomarControle::default())));
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
                acompanhar_retomar(&handle, &estado);
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
                app.state::<player::Player>().definir_equalizador(cfg.equalizador.efetivas(&cfg.equalizador_bandas));
                // Saída escolhida: procurada numa thread; se sumiu, fica a padrão do sistema.
                if let Some(chave) = settings::saida(&conn)? {
                    let h = app.handle().clone();
                    std::thread::spawn(move || {
                        if let Some(e) = saida::criar(&chave) {
                            h.state::<player::Player>().definir_saida(Some(e));
                        }
                    });
                }
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
            // Pode levar até alguns segundos (abrir o arquivo): fora do setup.
            let h = app.handle().clone();
            std::thread::spawn(move || {
                if let Err(e) = retomar_reproducao(&h) {
                    eprintln!("retomar: {e}");
                }
            });
            retomar_temporizador(app.handle());
            vigiar_pasta_dados(app.handle().clone());
            rganalise::iniciar(app.handle().clone());
            // Bandeja ligada: cria o ícone; iniciado com o sistema, abre só nela.
            #[cfg(desktop)]
            let bandeja = {
                let db = app.state::<db::Db>();
                let conn = db.0.lock().map_err(erro_db)?;
                settings::carregar(&conn)?.bandeja
            };
            #[cfg(desktop)]
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
            #[cfg(mobile)]
            let _ = (janela, evento);
            // Com a bandeja ligada, fechar só esconde a janela; a música continua.
            #[cfg(desktop)]
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
            queue_jump,
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
            playlist_import_m3u,
            playlist_folders,
            playlist_folder_create,
            playlist_folder_rename,
            playlist_folder_delete,
            playlist_set_folder,
            smart_playlist_save,
            smart_playlist_rules,
            smart_playlist_preview,
            history_list,
            history_count,
            stats_summary,
            stats_period,
            stats_months,
            retrospective_list,
            retrospective_get,
            retrospective_save_png,
            toggle_pause,
            player_state,
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
            media_rename,
            acoustic_available,
            metadata_recognize,
            categories_list,
            category_name,
            category_tracks,
            media_categories,
            category_suggest,
            category_add,
            category_remove,
            category_delete,
            category_rename,
            play_category,
            metadata_apply,
            set_notifications,
            set_continue,
            set_update_check,
            update_check,
            sync_status,
            set_sync_folder,
            sync_folder_now,
            sync_login,
            sync_cancel,
            sync_now,
            import_podcast_folder,
            import_audiobook_folder,
            audiobooks_list,
            audiobook_get,
            audiobook_chapters,
            audiobooks_in_progress,
            play_audiobook,
            audiobook_mark,
            podcast_shows,
            podcast_show,
            podcast_episodes,
            podcast_in_progress,
            podcast_mark,
            play_episode,
            podcast_info,
            set_podcast_speed,
            seek_relative,
            podcast_subscribe,
            podcast_refresh,
            podcast_unsubscribe,
            podcast_cover,
            podcast_download,
            podcast_download_cancel,
            long_task_start,
            long_task_text,
            long_task_end,
            missing_count,
            remove_missing,
            set_sync_upload,
            sync_logout,
            autostart_get,
            set_autostart,
            set_crossfade,
            set_normalization,
            set_equalizer,
            audio_outputs,
            set_audio_output
        ])
        .build(tauri::generate_context!())
        .expect("falha ao iniciar o Skáld")
        .run(|app, evento| {
            // Ao fechar, grava o tempo ouvido da faixa em andamento.
            if let tauri::RunEvent::Exit = evento {
                salvar_retomar(app, app.state::<player::Player>().posicao_ms());
                encerrar_sessao(app, false);
                // Grava o arquivo deste aparelho com o histórico de agora.
                let _ = sincronizar_pasta(app);
            }
        });
}
