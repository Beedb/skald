//! Integração com o sistema (seção 13): MPRIS no D-Bus (Linux) ou SMTC (Windows), via
//! souvlaki. Teclas de mídia, o painel do GNOME/KDE, o controle de volume do Windows e
//! fones Bluetooth usam essa interface para controlar o Skáld.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig, SeekDirection};
use tauri::{AppHandle, Manager};

use crate::player::{Estado, EstadoPlayer, Player};

#[cfg(windows)]
const INDISPONIVEL: &str = "Controles de mídia do Windows indisponíveis";
#[cfg(not(windows))]
const INDISPONIVEL: &str = "MPRIS indisponível";

/// Passo de busca quando o sistema não diz quanto avançar/voltar.
const PASSO: Duration = Duration::from_secs(10);

/// Último estado publicado, para só avisar o D-Bus quando algo muda de verdade.
struct Publicado {
    id: Option<i64>,
    estado: Estado,
    posicao_ms: u64,
    quando: Instant,
}

pub struct Mpris {
    controles: Mutex<MediaControls>,
    publicado: Mutex<Publicado>,
}

impl Mpris {
    pub fn iniciar(app: AppHandle) -> Result<Self, String> {
        // Windows (SMTC): os controles ficam presos à janela principal.
        #[cfg(windows)]
        let hwnd = Some(
            app.get_webview_window("main")
                .ok_or(format!("{INDISPONIVEL}: janela principal ainda não existe"))?
                .hwnd()
                .map_err(|e| format!("{INDISPONIVEL}: {e}"))?
                .0,
        );
        #[cfg(not(windows))]
        let hwnd = None;
        let config = PlatformConfig { display_name: "Skáld", dbus_name: "skald", hwnd };
        let mut controles = MediaControls::new(config).map_err(|e| format!("{INDISPONIVEL}: {e:?}"))?;
        controles
            .attach(move |evento| comando(&app, evento))
            .map_err(|e| format!("{INDISPONIVEL}: {e:?}"))?;
        Ok(Self {
            controles: Mutex::new(controles),
            publicado: Mutex::new(Publicado { id: None, estado: Estado::Parado, posicao_ms: 0, quando: Instant::now() }),
        })
    }

    fn estado_atual(&self) -> Estado {
        self.publicado.lock().map(|p| p.estado).unwrap_or(Estado::Parado)
    }

    /// Recebe cada aviso do player (inclusive o progresso a cada 250 ms) e publica só as
    /// mudanças: outra faixa, tocando/pausado/parado ou um salto de posição (busca).
    pub fn atualizar(&self, app: &AppHandle, e: &EstadoPlayer) {
        let Ok(mut p) = self.publicado.lock() else { return };
        let previsto = match p.estado {
            Estado::Tocando => p.posicao_ms + p.quando.elapsed().as_millis() as u64,
            _ => p.posicao_ms,
        };
        let trocou_faixa = e.id != p.id;
        if !trocou_faixa && e.estado == p.estado && e.posicao_ms.abs_diff(previsto) < 1500 {
            return;
        }
        *p = Publicado { id: e.id, estado: e.estado, posicao_ms: e.posicao_ms, quando: Instant::now() };
        drop(p);

        let progresso = Some(MediaPosition(Duration::from_millis(e.posicao_ms)));
        let playback = match e.estado {
            Estado::Tocando => MediaPlayback::Playing { progress: progresso },
            Estado::Pausado => MediaPlayback::Paused { progress: progresso },
            Estado::Parado => MediaPlayback::Stopped,
        };
        if let Ok(mut c) = self.controles.lock() {
            let _ = c.set_playback(playback);
        }
        if trocou_faixa {
            if let Some(id) = e.id {
                // Ler a capa abre o arquivo: fora da thread do player.
                let h = app.clone();
                std::thread::spawn(move || publicar_metadados(&h, id));
            }
        }
    }
}

fn publicar_metadados(app: &AppHandle, id: i64) {
    let (musica, detalhes) = {
        let db = app.state::<crate::db::Db>();
        let Ok(conn) = db.0.lock() else { return };
        let Ok(Some((musica, _))) = crate::library::obter(&conn, id) else { return };
        (musica, crate::details::do_banco(&conn, id).ok().flatten())
    };
    let capa = detalhes
        .map(crate::details::completar_do_arquivo)
        .and_then(|d| d.capa)
        .and_then(|url| gravar_capa(app, id, &url));
    // No Windows (SMTC) campo `None` mantém o valor da faixa anterior e a capa não tem
    // como ser apagada: campos vazios viram "" e a falta de capa vira o ícone do Skáld.
    #[cfg(windows)]
    let capa = capa.or_else(|| gravar_arquivo(app, "skald.png", include_bytes!("../icons/128x128.png")));
    let mpris = app.state::<Mpris>();
    // A faixa pode ter mudado enquanto a capa era lida.
    if mpris.publicado.lock().ok().and_then(|p| p.id) != Some(id) {
        return;
    }
    if let Ok(mut c) = mpris.controles.lock() {
        let _ = c.set_metadata(MediaMetadata {
            title: campo(&musica.title),
            artist: campo(&musica.artist),
            album: campo(&musica.album),
            cover_url: capa.as_deref(),
            duration: musica.duration_ms.map(|ms| Duration::from_millis(ms as u64)),
        });
    }
}

#[cfg(windows)]
fn campo(v: &Option<String>) -> Option<&str> {
    Some(v.as_deref().unwrap_or(""))
}

#[cfg(not(windows))]
fn campo(v: &Option<String>) -> Option<&str> {
    v.as_deref()
}

/// Grava a capa (data URL) no cache e devolve `file://...`. Um arquivo por faixa, para o
/// painel do sistema não mostrar a capa antiga; a anterior é apagada (é só cache).
fn gravar_capa(app: &AppHandle, id: i64, data_url: &str) -> Option<String> {
    let (cabecalho, dados) = data_url.strip_prefix("data:")?.split_once(";base64,")?;
    let ext = cabecalho.strip_prefix("image/").unwrap_or("jpg").replace("jpeg", "jpg");
    let bytes = STANDARD.decode(dados).ok()?;
    gravar_arquivo(app, &format!("capa-{id}.{ext}"), &bytes)
}

fn gravar_arquivo(app: &AppHandle, nome: &str, bytes: &[u8]) -> Option<String> {
    let dir = crate::caminhos::cache(app).ok()?.join("mpris");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let arquivo = dir.join(nome);
    std::fs::write(&arquivo, bytes).ok()?;
    Some(format!("file://{}", arquivo.display()))
}

/// Comandos vindos do sistema; usam as mesmas funções dos botões do app.
fn comando(app: &AppHandle, evento: MediaControlEvent) {
    let player = app.state::<Player>();
    let estado = app.state::<Mpris>().estado_atual();
    let resultado = match evento {
        MediaControlEvent::Toggle => player.alternar_pausa(),
        MediaControlEvent::Play if estado == Estado::Pausado => player.alternar_pausa(),
        MediaControlEvent::Pause | MediaControlEvent::Stop if estado == Estado::Tocando => player.alternar_pausa(),
        MediaControlEvent::Next => crate::avancar(app, false),
        MediaControlEvent::Previous => crate::voltar(app),
        MediaControlEvent::SetPosition(MediaPosition(p)) => player.buscar(p.as_millis() as u64),
        MediaControlEvent::Seek(dir) => buscar_relativo(&player, dir, PASSO),
        MediaControlEvent::SeekBy(dir, d) => buscar_relativo(&player, dir, d),
        MediaControlEvent::Raise => {
            if let Some(janela) = app.get_webview_window("main") {
                let _ = janela.unminimize();
                let _ = janela.show();
                let _ = janela.set_focus();
            }
            Ok(())
        }
        MediaControlEvent::Quit => {
            app.exit(0);
            Ok(())
        }
        _ => Ok(()),
    };
    if let Err(e) = resultado {
        use tauri::Emitter;
        let _ = app.emit("player-erro", e);
    }
}

fn buscar_relativo(player: &Player, dir: SeekDirection, passo: Duration) -> Result<(), String> {
    let agora = player.posicao_ms();
    let passo = passo.as_millis() as u64;
    let alvo = match dir {
        SeekDirection::Forward => agora + passo,
        SeekDirection::Backward => agora.saturating_sub(passo),
    };
    player.buscar(alvo)
}
