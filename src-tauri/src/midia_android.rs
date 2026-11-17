//! Android: notificação de mídia e MediaSession (`MidiaService.kt`), no lugar do MPRIS.
//! Mesma API pública de `mpris.rs`. O Rust avisa o Kotlin por JNI (`Midia.atualizar`) e os
//! botões da notificação, da tela de bloqueio e dos fones voltam por `Midia.comando`.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::sys::jlong;
use jni::{JNIEnv, JavaVM};
use tauri::{AppHandle, Manager};

use crate::player::{Estado, EstadoPlayer, Player};

static APP: OnceLock<AppHandle> = OnceLock::new();

/// Último estado publicado, para só avisar o Android quando algo muda de verdade.
struct Publicado {
    id: Option<i64>,
    estado: Estado,
    posicao_ms: u64,
    quando: Instant,
}

/// Pedido ao worker: estado novo (e se a faixa mudou, para reler metadados e capa).
struct Pedido {
    id: Option<i64>,
    estado: Estado,
    posicao_ms: u64,
}

pub struct Mpris {
    publicado: Mutex<Publicado>,
    envio: Mutex<Sender<Pedido>>,
}

impl Mpris {
    pub fn iniciar(app: AppHandle) -> Result<Self, String> {
        let _ = APP.set(app.clone());
        // Widget: playlists recentes agora e a cada mudança (criar, tocar, capa, itens).
        {
            use tauri::Listener;
            let a = app.clone();
            std::thread::spawn(move || publicar_playlists(&a));
            let a = app.clone();
            app.listen(crate::EVENTO_PLAYLISTS, move |_| {
                let a = a.clone();
                std::thread::spawn(move || publicar_playlists(&a));
            });
        }
        let (envio, recebe) = mpsc::channel();
        std::thread::spawn(move || trabalhar(app, recebe));
        Ok(Self {
            publicado: Mutex::new(Publicado { id: None, estado: Estado::Parado, posicao_ms: 0, quando: Instant::now() }),
            envio: Mutex::new(envio),
        })
    }

    fn estado_atual(&self) -> Estado {
        self.publicado.lock().map(|p| p.estado).unwrap_or(Estado::Parado)
    }

    /// Recebe cada aviso do player e repassa só as mudanças: outra faixa, tocando/pausado/
    /// parado ou um salto de posição (busca).
    pub fn atualizar(&self, _app: &AppHandle, e: &EstadoPlayer) {
        let Ok(mut p) = self.publicado.lock() else { return };
        let previsto = match p.estado {
            Estado::Tocando => p.posicao_ms + p.quando.elapsed().as_millis() as u64,
            _ => p.posicao_ms,
        };
        if e.id == p.id && e.estado == p.estado && e.posicao_ms.abs_diff(previsto) < 1500 {
            return;
        }
        *p = Publicado { id: e.id, estado: e.estado, posicao_ms: e.posicao_ms, quando: Instant::now() };
        drop(p);
        if let Ok(envio) = self.envio.lock() {
            let _ = envio.send(Pedido { id: e.id, estado: e.estado, posicao_ms: e.posicao_ms });
        }
    }
}

/// Metadados da faixa atual, guardados entre um aviso e outro.
#[derive(Default)]
struct Meta {
    id: Option<i64>,
    titulo: String,
    artista: String,
    album: String,
    capa: Option<String>,
    duracao_ms: i64,
}

/// Uma thread só, para os avisos chegarem ao Android na ordem certa. Ler a capa abre o
/// arquivo, por isso fica fora da thread do player.
fn trabalhar(app: AppHandle, recebe: Receiver<Pedido>) {
    let mut meta = Meta::default();
    while let Ok(mut pedido) = recebe.recv() {
        // Vários avisos acumulados: vale o mais recente.
        while let Ok(p) = recebe.try_recv() {
            pedido = p;
        }
        if pedido.id != meta.id {
            meta = carregar(&app, pedido.id);
        }
        let estado = match pedido.estado {
            Estado::Parado => 0,
            Estado::Tocando => 1,
            Estado::Pausado => 2,
        };
        let estado = if meta.id.is_none() { 0 } else { estado };
        if let Err(e) = publicar(&meta, estado, pedido.posicao_ms as i64) {
            eprintln!("Notificação de mídia: {e}");
        }
    }
}

fn carregar(app: &AppHandle, id: Option<i64>) -> Meta {
    let Some(id) = id else { return Meta::default() };
    let (musica, detalhes) = {
        let db = app.state::<crate::db::Db>();
        let Ok(conn) = db.0.lock() else { return Meta::default() };
        let Ok(Some((musica, _))) = crate::library::obter(&conn, id) else { return Meta::default() };
        (musica, crate::details::do_banco(&conn, id).ok().flatten())
    };
    let capa = detalhes
        .map(crate::details::completar_do_arquivo)
        .and_then(|d| d.capa)
        .and_then(|url| gravar_capa(app, id, &url));
    Meta {
        id: Some(id),
        titulo: musica.title.unwrap_or_else(|| "Sem título".into()),
        artista: musica.artist.unwrap_or_else(|| "Artista desconhecido".into()),
        album: musica.album.unwrap_or_default(),
        capa,
        duracao_ms: musica.duration_ms.unwrap_or(0),
    }
}

/// Grava a capa (data URL) no cache e devolve o caminho. Um arquivo por faixa; o anterior
/// é apagado (é só cache).
fn gravar_capa(app: &AppHandle, id: i64, data_url: &str) -> Option<String> {
    let (cabecalho, dados) = data_url.strip_prefix("data:")?.split_once(";base64,")?;
    let ext = cabecalho.strip_prefix("image/").unwrap_or("jpg").replace("jpeg", "jpg");
    let bytes = STANDARD.decode(dados).ok()?;
    let dir = crate::caminhos::cache(app).ok()?.join("midia");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let arquivo = dir.join(format!("capa-{id}.{ext}"));
    std::fs::write(&arquivo, bytes).ok()?;
    Some(arquivo.to_string_lossy().into_owned())
}

/// Chama `com.skald.app.Midia.atualizar(...)`.
fn publicar(m: &Meta, estado: i32, posicao_ms: i64) -> Result<(), String> {
    com_midia(|env, c| {
        let titulo = env.new_string(&m.titulo)?;
        let artista = env.new_string(&m.artista)?;
        let album = env.new_string(&m.album)?;
        let capa: JObject = match &m.capa {
            Some(p) => env.new_string(p)?.into(),
            None => JObject::null(),
        };
        env.call_static_method(
            c,
            "atualizar",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;JJI)V",
            &[
                JValue::Object(&titulo),
                JValue::Object(&artista),
                JValue::Object(&album),
                JValue::Object(&capa),
                JValue::Long(m.duracao_ms),
                JValue::Long(posicao_ms),
                JValue::Int(estado),
            ],
        )?;
        Ok(())
    })
}

/// Quantas playlists o widget mostra no máximo (5×2).
const PLAYLISTS_WIDGET: usize = 5;

/// Playlists para o widget: não vazias, tocadas mais recentemente primeiro (as nunca
/// tocadas seguem a ordem da lateral), com a capa escolhida ou a miniatura da primeira música.
fn playlists_widget(app: &AppHandle) -> Vec<(i64, Option<String>)> {
    let db = app.state::<crate::db::Db>();
    let escolhidas: Vec<(i64, Option<String>, Option<i64>)> = {
        let Ok(conn) = db.0.lock() else { return Vec::new() };
        let Ok(mut lista) = crate::playlists::listar(&conn) else { return Vec::new() };
        lista.retain(|p| p.total > 0);
        lista.sort_by_key(|p| std::cmp::Reverse(p.tocada_em));
        lista
            .into_iter()
            .take(PLAYLISTS_WIDGET)
            .map(|p| {
                let escolhida = if p.capa_escolhida { crate::playlists::capa(&conn, p.id).ok().flatten() } else { None };
                (p.id, escolhida, p.primeira_media_id)
            })
            .collect()
    };
    let pasta = crate::caminhos::cache(app).ok().map(crate::miniaturas::pasta);
    escolhidas
        .into_iter()
        .map(|(id, escolhida, primeira)| {
            let capa = escolhida.or_else(|| {
                let (pasta, primeira) = (pasta.as_ref()?, primeira?);
                crate::miniaturas::arquivo(&db.0, pasta, primeira).ok().flatten().map(|c| c.to_string_lossy().into_owned())
            });
            (id, capa)
        })
        .collect()
}

/// Manda ao widget uma linha por playlist: `id<TAB>caminho da capa` (caminho vazio = sem capa).
fn publicar_playlists(app: &AppHandle) {
    let texto: String = playlists_widget(app)
        .into_iter()
        .map(|(id, capa)| format!("{id}\t{}\n", capa.unwrap_or_default()))
        .collect();
    let r = com_midia(|env, c| {
        let t = env.new_string(&texto)?;
        env.call_static_method(c, "playlists", "(Ljava/lang/String;)V", &[JValue::Object(&t)])?;
        Ok(())
    });
    if let Err(e) = r {
        eprintln!("Widget (playlists): {e}");
    }
}

/// Máquina Java e classe `com.skald.app.Midia`, guardadas por `Midia.iniciarNativo` (o Kotlin
/// chama antes de o Rust iniciar, com ou sem atividade). Numa thread nativa o `FindClass` só
/// enxerga as classes do sistema; por isso a classe vem pronta.
static JVM: OnceLock<JavaVM> = OnceLock::new();
static CLASSE_MIDIA: OnceLock<GlobalRef> = OnceLock::new();

/// Roda `f` com a classe `com.skald.app.Midia`, nesta thread anexada à máquina Java.
fn com_midia(f: impl FnOnce(&mut JNIEnv, &JClass) -> jni::errors::Result<()>) -> Result<(), String> {
    let (Some(vm), Some(classe)) = (JVM.get(), CLASSE_MIDIA.get()) else {
        return Err("Midia.iniciarNativo ainda não foi chamado".into());
    };
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let c: &JClass = classe.as_obj().into();
    let r = f(&mut env, c);
    if r.is_err() && env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    r.map_err(|e| e.to_string())
}

/// Notificação de tarefa longa (login no Google, sincronização): `Some(texto)` liga o
/// `SyncService` (o Android não congela o app), `None` desliga.
pub fn segundo_plano(texto: Option<&str>) {
    let r = com_midia(|env, c| {
        let t: JObject = match texto {
            Some(t) => env.new_string(t)?.into(),
            None => JObject::null(),
        };
        env.call_static_method(c, "segundoPlano", "(Ljava/lang/String;)V", &[JValue::Object(&t)])?;
        Ok(())
    });
    if let Err(e) = r {
        eprintln!("SyncService: {e}");
    }
}

/// `Midia.iniciarNativo(dados, cache, musica)`: chamado pelo Kotlin antes de o Rust iniciar.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_skald_app_Midia_iniciarNativo<'l>(
    mut env: JNIEnv<'l>,
    classe: JClass<'l>,
    dados: JString<'l>,
    cache: JString<'l>,
    musica: JString<'l>,
) {
    if let Ok(vm) = env.get_java_vm() {
        let _ = JVM.set(vm);
    }
    if CLASSE_MIDIA.get().is_none() {
        if let Ok(g) = env.new_global_ref(&classe) {
            let _ = CLASSE_MIDIA.set(g);
        }
    }
    let texto = |env: &mut JNIEnv<'l>, s: &JString<'l>| -> Option<String> { env.get_string(s).ok().map(Into::into) };
    if let (Some(d), Some(c), Some(m)) = (texto(&mut env, &dados), texto(&mut env, &cache), texto(&mut env, &musica)) {
        crate::caminhos::definir_android(d.into(), c.into(), m.into());
    }
}

// ---------- Android Auto: navegação pela biblioteca (MediaBrowserService) ----------

/// Uma linha por item: `id<TAB>título<TAB>subtítulo<TAB>capa<TAB>tipo` (tipo `p` = pasta,
/// `t` = tocável). Os ids tocáveis voltam por `Midia.comando(id, 0)`.
fn item(id: &str, titulo: &str, subtitulo: &str, capa: Option<String>, pasta: bool) -> String {
    let limpo = |t: &str| t.replace(['\t', '\n'], " ");
    format!(
        "{}\t{}\t{}\t{}\t{}\n",
        limpo(id),
        limpo(titulo),
        limpo(subtitulo),
        capa.unwrap_or_default(),
        if pasta { "p" } else { "t" }
    )
}

fn navegar(app: &AppHandle, pai: &str) -> String {
    let db = app.state::<crate::db::Db>();
    let pasta = crate::caminhos::cache(app).ok().map(crate::miniaturas::pasta);
    let capa = |id: i64| -> Option<String> {
        let c = crate::miniaturas::arquivo(&db.0, pasta.as_ref()?, id).ok().flatten()?;
        Some(c.to_string_lossy().into_owned())
    };
    let ler = || db.0.lock().ok();
    match pai {
        "raiz" => [
            ("playlists", "Playlists"),
            ("albuns", "Álbuns"),
            ("artistas", "Artistas"),
            ("recentes", "Tocadas recentemente"),
        ]
        .iter()
        .map(|(id, nome)| item(id, nome, "", None, true))
        .collect(),
        "playlists" => {
            let Some(lista) = ler().and_then(|c| crate::playlists::listar(&c).ok()) else { return String::new() };
            lista
                .into_iter()
                .filter(|p| p.total > 0)
                .map(|p| {
                    let escolhida = p.capa_escolhida.then(|| ler().and_then(|c| crate::playlists::capa(&c, p.id).ok().flatten())).flatten();
                    let c = escolhida.or_else(|| p.primeira_media_id.and_then(capa));
                    let sub = if p.total == 1 { "1 música".to_string() } else { format!("{} músicas", p.total) };
                    item(&format!("playlist:{}", p.id), &p.name, &sub, c, false)
                })
                .collect()
        }
        "albuns" => {
            let Some(lista) = ler().and_then(|c| crate::colecao::albuns(&c, None).ok()) else { return String::new() };
            lista
                .into_iter()
                .filter_map(|a| {
                    let chave = serde_json::to_string(&a.chave).ok()?;
                    let titulo = a.chave.album.clone().unwrap_or_else(|| "Sem álbum".into());
                    let sub = a.artista.clone().unwrap_or_else(|| "Vários artistas".into());
                    Some(item(&format!("album:{chave}"), &titulo, &sub, capa(a.capa_id), false))
                })
                .collect()
        }
        "artistas" => {
            let Some(lista) = ler().and_then(|c| crate::colecao::artistas(&c).ok()) else { return String::new() };
            lista
                .into_iter()
                .filter_map(|a| {
                    let id = serde_json::to_string(&a.nome).ok()?;
                    let nome = a.nome.clone().unwrap_or_else(|| "Desconhecido".into());
                    let sub = if a.musicas == 1 { "1 música".to_string() } else { format!("{} músicas", a.musicas) };
                    Some(item(&format!("artista:{id}"), &nome, &sub, capa(a.capa_id), false))
                })
                .collect()
        }
        "recentes" => {
            let Some(lista) = ler().and_then(|c| crate::colecao::tocadas_recentes(&c, RECENTES).ok()) else { return String::new() };
            lista
                .into_iter()
                .enumerate()
                .map(|(i, m)| {
                    let titulo = m.title.unwrap_or_else(|| "Sem título".into());
                    item(&format!("recentes:{i}"), &titulo, m.artist.as_deref().unwrap_or(""), capa(m.id), false)
                })
                .collect()
        }
        _ => String::new(),
    }
}

/// Quantas músicas a pasta "Tocadas recentemente" mostra (e vira a fila ao tocar uma).
const RECENTES: usize = 30;

/// `Midia.navegar(pai)`: filhos de um nó da árvore do Android Auto (chamado fora da thread principal).
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_skald_app_Midia_navegar<'l>(
    mut env: JNIEnv<'l>,
    _classe: JClass<'l>,
    pai: JString<'l>,
) -> jni::sys::jstring {
    let pai: String = env.get_string(&pai).map(Into::into).unwrap_or_default();
    let texto = APP.get().map(|app| navegar(app, &pai)).unwrap_or_default();
    env.new_string(texto).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Ids tocáveis do Android Auto (`playlist:`, `album:`, `artista:`, `recentes:`).
fn tocar_do_carro(app: &AppHandle, id: &str) -> Option<Result<(), String>> {
    let (tipo, valor) = id.split_once(':')?;
    let fila = app.state::<crate::FilaState>();
    Some(match tipo {
        "playlist" => crate::play_playlist(app.clone(), fila, valor.parse().ok()?, crate::playlists::Ordem::Personalizada, 0),
        "album" => crate::play_album(app.clone(), fila, serde_json::from_str(valor).ok()?, 0),
        "artista" => crate::play_artist(app.clone(), fila, serde_json::from_str(valor).ok()?),
        "recentes" => {
            let indice: usize = valor.parse().ok()?;
            let ids: Vec<i64> = {
                let db = app.state::<crate::db::Db>();
                let conn = db.0.lock().ok()?;
                crate::colecao::tocadas_recentes(&conn, RECENTES).ok()?.into_iter().map(|m| m.id).collect()
            };
            crate::tocar_lista(app, &fila, ids, indice)
        }
        _ => return None,
    })
}

/// `Midia.comando(acao, valor)`: botões da notificação, tela de bloqueio e fones.
/// Usa as mesmas funções dos botões do app, fora da thread do Android.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_skald_app_Midia_comando<'l>(
    mut env: JNIEnv<'l>,
    _classe: JClass<'l>,
    acao: JString<'l>,
    valor: jlong,
) {
    let acao: String = match env.get_string(&acao) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let Some(app) = APP.get().cloned() else { return };
    std::thread::spawn(move || comando(&app, &acao, valor.max(0) as u64));
}

fn comando(app: &AppHandle, acao: &str, valor: u64) {
    let player = app.state::<Player>();
    let estado = app.state::<Mpris>().estado_atual();
    let resultado = match acao {
        "alternar" => player.alternar_pausa(),
        "tocar" if estado == Estado::Pausado => player.alternar_pausa(),
        "pausar" if estado == Estado::Tocando => player.alternar_pausa(),
        "proxima" => crate::avancar(app, false),
        "anterior" => crate::voltar(app),
        "buscar" => player.buscar(valor),
        "playlist" => crate::play_playlist(
            app.clone(),
            app.state::<crate::FilaState>(),
            valor as i64,
            crate::playlists::Ordem::Personalizada,
            0,
        ),
        outra => tocar_do_carro(app, outra).unwrap_or(Ok(())),
    };
    if let Err(e) = resultado {
        use tauri::Emitter;
        let _ = app.emit("player-erro", e);
    }
}
