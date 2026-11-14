use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use serde::Serialize;

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Estado {
    Parado,
    Tocando,
    Pausado,
}

#[derive(Serialize, Clone, Debug)]
pub struct EstadoPlayer {
    pub id: Option<i64>,
    pub estado: Estado,
    pub posicao_ms: u64,
    pub duracao_ms: u64,
    pub erro: Option<String>,
    /// Verdadeiro só no aviso de fim de faixa (para avançar a fila).
    pub fim: bool,
    /// Verdadeiro só no aviso de troca sem pausa (gapless): `id` já é a nova faixa.
    pub transicao: bool,
}

pub struct Player {
    playbin: gst::Element,
    atual: Arc<Mutex<Option<i64>>>,
    /// Faixa já carregada para tocar em seguida, sem pausa (gapless).
    pendente: Arc<Mutex<Option<i64>>>,
}

impl Player {
    /// `avisar` recebe cada mudança de estado e o progresso (a cada 250 ms tocando).
    /// `pedir_proxima` é chamada pouco antes do fim da faixa e devolve `(id, caminho)` da
    /// seguinte, que entra sem pausa (gapless). `None`: a reprodução termina normalmente.
    pub fn new(
        avisar: impl Fn(EstadoPlayer) + Send + 'static,
        pedir_proxima: impl Fn() -> Option<(i64, String)> + Send + Sync + 'static,
    ) -> Result<Self, String> {
        gst::init().map_err(|e| format!("Falha ao iniciar o GStreamer: {e}"))?;
        let playbin = gst::ElementFactory::make("playbin")
            .build()
            .map_err(|_| "Elemento playbin do GStreamer indisponível.".to_string())?;
        // Só áudio: capas embutidas como fluxo de vídeo não abrem janela.
        if let Ok(fake) = gst::ElementFactory::make("fakesink").build() {
            playbin.set_property("video-sink", &fake);
        }
        let atual = Arc::new(Mutex::new(None));
        let pendente = Arc::new(Mutex::new(None));
        let p = pendente.clone();
        playbin.connect("about-to-finish", false, move |args| {
            let pb = args[0].get::<gst::Element>().ok()?;
            if let Some((id, path)) = pedir_proxima() {
                if let Ok(uri) = gst::glib::filename_to_uri(&path, None) {
                    pb.set_property("uri", uri.as_str());
                    *p.lock().unwrap() = Some(id);
                }
            }
            None
        });
        vigiar(avisar, playbin.clone(), atual.clone(), pendente.clone());
        Ok(Self { playbin, atual, pendente })
    }

    pub fn tocar(&self, id: i64, path: &str) -> Result<(), String> {
        let uri = gst::glib::filename_to_uri(path, None).map_err(|e| e.to_string())?;
        *self.pendente.lock().unwrap() = None;
        self.playbin.set_state(gst::State::Null).map_err(|e| e.to_string())?;
        *self.pendente.lock().unwrap() = None;
        self.playbin.set_property("uri", uri.as_str());
        *self.atual.lock().unwrap() = Some(id);
        self.playbin
            .set_state(gst::State::Playing)
            .map_err(|_| "Não foi possível tocar este arquivo.".to_string())?;
        Ok(())
    }

    /// Alterna entre tocar e pausar. Sem música carregada, não faz nada.
    pub fn alternar_pausa(&self) -> Result<(), String> {
        if self.atual.lock().unwrap().is_none() {
            return Ok(());
        }
        let alvo = if self.playbin.current_state() == gst::State::Playing {
            gst::State::Paused
        } else {
            gst::State::Playing
        };
        self.playbin.set_state(alvo).map(|_| ()).map_err(|e| e.to_string())
    }

    /// Volume de 0 a 1 na escala percebida; convertido para linear (cúbico).
    pub fn volume(&self, v: f64) {
        let v = v.clamp(0.0, 1.0);
        self.playbin.set_property("volume", v * v * v);
    }

    pub fn posicao_ms(&self) -> u64 {
        self.playbin.query_position::<gst::ClockTime>().map_or(0, |t| t.mseconds())
    }

    pub fn buscar(&self, ms: u64) -> Result<(), String> {
        self.playbin
            .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, gst::ClockTime::from_mseconds(ms))
            .map_err(|_| "Não foi possível mudar a posição.".to_string())
    }
}

fn estado_de(playbin: &gst::Element, atual: Option<i64>) -> EstadoPlayer {
    let estado = match (atual, playbin.current_state()) {
        (None, _) => Estado::Parado,
        (_, gst::State::Playing) => Estado::Tocando,
        (_, gst::State::Paused) => Estado::Pausado,
        _ => Estado::Parado,
    };
    EstadoPlayer {
        id: atual,
        estado,
        posicao_ms: playbin.query_position::<gst::ClockTime>().map_or(0, |t| t.mseconds()),
        duracao_ms: playbin.query_duration::<gst::ClockTime>().map_or(0, |t| t.mseconds()),
        erro: None,
        fim: false,
        transicao: false,
    }
}

/// Lê o barramento (fim de faixa, erros, mudanças de estado) e emite o progresso.
fn vigiar(
    avisar: impl Fn(EstadoPlayer) + Send + 'static,
    playbin: gst::Element,
    atual: Arc<Mutex<Option<i64>>>,
    pendente: Arc<Mutex<Option<i64>>>,
) {
    let bus = playbin.bus().expect("playbin sempre tem barramento");
    std::thread::spawn(move || {
        let mut ultimo_envio = Instant::now();
        loop {
            let mut envio_imediato = false;
            let mut erro = None;
            let mut fim = false;
            let mut transicao = false;
            if let Some(msg) = bus.timed_pop(gst::ClockTime::from_mseconds(100)) {
                match msg.view() {
                    gst::MessageView::Eos(..) => {
                        let _ = playbin.set_state(gst::State::Null);
                        *pendente.lock().unwrap() = None;
                        *atual.lock().unwrap() = None;
                        envio_imediato = true;
                        fim = true;
                    }
                    // A faixa carregada no `about-to-finish` começou a tocar.
                    gst::MessageView::StreamStart(..) => {
                        if let Some(id) = pendente.lock().unwrap().take() {
                            *atual.lock().unwrap() = Some(id);
                            envio_imediato = true;
                            transicao = true;
                        }
                    }
                    gst::MessageView::Error(e) => {
                        let _ = playbin.set_state(gst::State::Null);
                        *atual.lock().unwrap() = None;
                        erro = Some(format!("Erro ao tocar: {}", e.error()));
                        envio_imediato = true;
                    }
                    gst::MessageView::StateChanged(s)
                        if s.src().is_some_and(|src| src == playbin.upcast_ref::<gst::Object>()) =>
                    {
                        envio_imediato = true;
                    }
                    _ => {}
                }
            }
            let tocando = playbin.current_state() == gst::State::Playing;
            if envio_imediato || (tocando && ultimo_envio.elapsed() >= Duration::from_millis(250)) {
                let mut est = estado_de(&playbin, *atual.lock().unwrap());
                est.erro = erro;
                est.fim = fim;
                est.transicao = transicao;
                avisar(est);
                ultimo_envio = Instant::now();
            }
        }
    });
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::sync::mpsc;

    /// Toca a.mp3 (3 s) da pasta de teste sem som (fakesink) e espera o fim da faixa.
    /// Rodar com: SKALD_TEST_DIR=/pasta cargo test player -- --nocapture
    #[test]
    fn toca_ate_o_fim() {
        let Ok(dir) = std::env::var("SKALD_TEST_DIR") else { return };
        let (tx, rx) = mpsc::channel();
        let p = Player::new(move |e| { let _ = tx.send(e); }, || None).unwrap();
        let mudo = gst::ElementFactory::make("fakesink").property("sync", true).build().unwrap();
        p.playbin.set_property("audio-sink", &mudo);
        p.volume(0.5);
        p.tocar(7, &format!("{dir}/a.mp3")).unwrap();

        let mut viu_tocando = false;
        let mut maior_posicao = 0;
        let fim = loop {
            let e = rx.recv_timeout(Duration::from_secs(10)).expect("sem eventos do player");
            if e.estado == Estado::Tocando {
                viu_tocando = true;
                maior_posicao = maior_posicao.max(e.posicao_ms);
                assert_eq!(e.id, Some(7));
                assert!(e.duracao_ms > 2900, "duração {}", e.duracao_ms);
            }
            if viu_tocando && e.estado == Estado::Parado {
                assert!(e.fim, "o aviso de parada deve marcar fim de faixa");
                break e;
            }
        };
        println!("posição máxima vista: {maior_posicao} ms");
        assert!(maior_posicao > 2000);
        assert_eq!(fim.id, None);
        assert!(fim.erro.is_none());
    }

    #[test]
    fn arquivo_invalido_gera_erro() {
        let Ok(dir) = std::env::var("SKALD_TEST_DIR") else { return };
        let (tx, rx) = mpsc::channel();
        let p = Player::new(move |e| { let _ = tx.send(e); }, || None).unwrap();
        let _ = p.tocar(1, &format!("{dir}/sub/quebrado.ogg"));
        let erro = loop {
            let e = rx.recv_timeout(Duration::from_secs(10)).expect("sem eventos do player");
            if let Some(m) = e.erro { break m; }
        };
        println!("erro: {erro}");
    }

    /// Gera um WAV de ~1,5 s com o GStreamer (tom de 440 Hz).
    fn gerar_wav(destino: &std::path::Path) -> bool {
        std::process::Command::new("gst-launch-1.0")
            .args(["-q", "audiotestsrc", "num-buffers=65", "samplesperbuffer=1024", "!", "wavenc", "!", "filesink"])
            .arg(format!("location={}", destino.display()))
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Duas faixas seguidas sem pausa: a segunda entra pelo `about-to-finish` e o player
    /// avisa a transição (com o novo id) antes do fim de tudo.
    #[test]
    fn troca_sem_pausa() {
        let dir = std::env::temp_dir().join(format!("skald-teste-gapless-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.wav"), dir.join("b.wav"));
        if !gerar_wav(&a) || !gerar_wav(&b) {
            eprintln!("gst-launch-1.0 indisponível; teste ignorado");
            return;
        }
        let pedida = std::sync::atomic::AtomicBool::new(false);
        let b_str = b.to_string_lossy().into_owned();
        let (tx, rx) = mpsc::channel();
        let p = Player::new(
            move |e| { let _ = tx.send(e); },
            move || (!pedida.swap(true, std::sync::atomic::Ordering::SeqCst)).then(|| (2, b_str.clone())),
        )
        .unwrap();
        let mudo = gst::ElementFactory::make("fakesink").property("sync", true).build().unwrap();
        p.playbin.set_property("audio-sink", &mudo);
        p.tocar(1, &a.to_string_lossy()).unwrap();

        let mut transicao = None;
        let mut parou_antes = false;
        let fim = loop {
            let e = rx.recv_timeout(Duration::from_secs(10)).expect("sem eventos do player");
            assert!(e.erro.is_none(), "{:?}", e.erro);
            if e.transicao {
                transicao = Some(e.clone());
            }
            if e.fim {
                break e;
            }
            if transicao.is_none() && e.estado == Estado::Parado && e.id.is_none() {
                parou_antes = true;
            }
        };
        let t = transicao.expect("faltou o aviso de transição");
        assert_eq!(t.id, Some(2));
        assert_eq!(t.estado, Estado::Tocando);
        assert!(!parou_antes, "o player parou entre as faixas");
        assert_eq!(fim.id, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
