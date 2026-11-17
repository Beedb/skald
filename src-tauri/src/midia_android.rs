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
    let mut classe: Option<GlobalRef> = None;
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
        if let Err(e) = publicar(&mut classe, &meta, estado, pedido.posicao_ms as i64) {
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
    let dir = app.path().app_cache_dir().ok()?.join("midia");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let arquivo = dir.join(format!("capa-{id}.{ext}"));
    std::fs::write(&arquivo, bytes).ok()?;
    Some(arquivo.to_string_lossy().into_owned())
}

/// Chama `com.skald.app.Midia.atualizar(...)`. A classe vem do class loader da atividade:
/// numa thread nativa o `FindClass` só enxerga as classes do sistema.
fn publicar(classe: &mut Option<GlobalRef>, m: &Meta, estado: i32, posicao_ms: i64) -> Result<(), String> {
    let ctx = ndk_context::android_context();
    if ctx.vm().is_null() || ctx.context().is_null() {
        return Err("contexto do Android indisponível".into());
    }
    // SAFETY: ponteiros do JavaVM e da atividade fornecidos pelo tao (ndk_context).
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let r = (|| -> jni::errors::Result<()> {
        if classe.is_none() {
            // SAFETY: referência global da atividade, mantida pelo tao; não é apagada aqui.
            let atividade = unsafe { JObject::from_raw(ctx.context().cast()) };
            let loader = env.call_method(&atividade, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
            let nome = env.new_string("com.skald.app.Midia")?;
            let c = env
                .call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&nome)])?
                .l()?;
            *classe = Some(env.new_global_ref(c)?);
        }
        let c: &JClass = classe.as_ref().unwrap().as_obj().into();
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
    })();
    if r.is_err() && env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    r.map_err(|e| e.to_string())
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
    let pasta = app.path().app_cache_dir().ok().map(crate::miniaturas::pasta);
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

/// Roda `f` com a classe `com.skald.app.Midia` (carregada pelo class loader da atividade).
fn com_midia(f: impl FnOnce(&mut JNIEnv, &JClass) -> jni::errors::Result<()>) -> Result<(), String> {
    let ctx = ndk_context::android_context();
    if ctx.vm().is_null() || ctx.context().is_null() {
        return Err("contexto do Android indisponível".into());
    }
    // SAFETY: ponteiros do JavaVM e da atividade fornecidos pelo tao (ndk_context).
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let r = (|| -> jni::errors::Result<()> {
        // SAFETY: referência global da atividade, mantida pelo tao; não é apagada aqui.
        let atividade = unsafe { JObject::from_raw(ctx.context().cast()) };
        let loader = env.call_method(&atividade, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
        let nome = env.new_string("com.skald.app.Midia")?;
        let c = env
            .call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&nome)])?
            .l()?;
        let c: JClass = c.into();
        f(&mut env, &c)
    })();
    if r.is_err() && env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    r.map_err(|e| e.to_string())
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
        _ => Ok(()),
    };
    if let Err(e) = resultado {
        use tauri::Emitter;
        let _ = app.emit("player-erro", e);
    }
}
