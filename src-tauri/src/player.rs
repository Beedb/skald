//! Player com dois "decks" (dois `playbin`). Normalmente só um toca; o outro entra no
//! crossfade. Entre faixas do mesmo álbum não há crossfade: a próxima é emendada no mesmo
//! deck pelo `about-to-finish` (gapless).
//!
//! Regra de travas: nunca mudar o estado de um `playbin` segurando `Interno`, porque o
//! `about-to-finish` roda na thread do GStreamer e também trava `Interno`.

use std::f64::consts::FRAC_PI_2;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use serde::Serialize;

use crate::eq;
use crate::ganho::{self, Ganho};

/// Inicia o GStreamer. Nos testes do Windows, o `cargo test` põe `target\debug` no PATH,
/// onde o Tauri copiou o GStreamer embutido: a DLL de lá não acha plugins sozinha. Então
/// os testes usam os plugins embutidos, como o app (`gstreamer_embutido` em lib.rs).
pub(crate) fn gst_iniciar() -> Result<(), gst::glib::Error> {
    #[cfg(all(test, windows))]
    {
        static UMA_VEZ: std::sync::Once = std::sync::Once::new();
        UMA_VEZ.call_once(|| {
            let caminhos = std::env::var_os("PATH").unwrap_or_default();
            let Some(dir) = std::env::split_paths(&caminhos)
                .find(|p| p.join("gstreamer-1.0-0.dll").is_file() && p.join("gstreamer-1.0").is_dir())
            else {
                return;
            };
            let registro = std::env::temp_dir().join("skald-teste-gstreamer-registry.bin");
            // SAFETY: só nos testes, antes do primeiro `gst::init`.
            unsafe {
                std::env::set_var("GST_PLUGIN_SYSTEM_PATH_1_0", dir.join("gstreamer-1.0"));
                std::env::remove_var("GST_PLUGIN_PATH_1_0");
                std::env::remove_var("GST_PLUGIN_PATH");
                std::env::set_var("GST_REGISTRY_1_0", registro);
                std::env::set_var("GST_REGISTRY_FORK", "no");
            }
        });
    }
    gst::init()?;
    // O `equalizer-10bands` registra o tipo das bandas na primeira criação, sem trava:
    // duas threads criando o primeiro ao mesmo tempo dão "cannot register existing type
    // 'GstIirEqualizerBand'" e acesso inválido (visto nos testes em paralelo). Cria um
    // aqui, uma vez só, antes de qualquer outro.
    static AQUECER: std::sync::Once = std::sync::Once::new();
    AQUECER.call_once(|| {
        let _ = gst::ElementFactory::make("equalizer-10bands").build();
    });
    Ok(())
}

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
    /// Verdadeiro só no aviso de troca automática (gapless ou crossfade): `id` já é a nova.
    pub transicao: bool,
}

/// Próxima faixa escolhida pelo app.
pub struct Proxima {
    pub id: i64,
    pub path: String,
    /// Mesmo álbum da atual: emenda sem crossfade (gapless).
    pub mesmo_album: bool,
    pub ganho: Ganho,
}

/// Se a próxima já foi pedida ao app para a faixa atual.
enum Escolha {
    NaoPedida,
    Pedida(Option<Proxima>),
}

struct Cruzamento {
    saindo: usize,
    dur_ms: u64,
}

struct Interno {
    ativo: usize,
    atual: Option<i64>,
    /// Faixa emendada no deck ativo pelo `about-to-finish`, esperando começar.
    pendente: Option<(i64, Ganho)>,
    escolha: Escolha,
    cruzando: Option<Cruzamento>,
    /// Volume linear (já convertido da escala percebida).
    volume: f64,
    crossfade_ms: u64,
    normalizacao: ganho::Modo,
    /// ReplayGain da faixa em cada deck.
    ganhos: [Ganho; 2],
    /// Compensa o reforço do preset do equalizador.
    pre_eq: f64,
    /// Redução do temporizador (1 = nenhuma).
    atenuacao: f64,
    /// Velocidade pedida (podcasts; música é sempre 1) e a já aplicada em cada deck.
    velocidade: f64,
    taxa: [f64; 2],
}

impl Interno {
    /// Volume final do deck `i`: o do usuário vezes a normalização da faixa.
    fn alvo(&self, i: usize) -> f64 {
        self.volume * ganho::fator(self.normalizacao, &self.ganhos[i]) * self.pre_eq * self.atenuacao
    }
}

type Pedir = dyn Fn() -> Option<Proxima> + Send + Sync;

pub struct Player {
    decks: [gst::Element; 2],
    /// Equalizador de cada deck (ausente se o plugin não estiver instalado).
    eqs: [Option<gst::Element>; 2],
    int: Arc<Mutex<Interno>>,
    /// Pede à thread de `vigiar` que termine (ao descartar o player).
    encerrar: Arc<AtomicBool>,
}

/// No app o player vive até o fim; nos testes cada um é descartado. Sem isto a thread de
/// `vigiar` e os `playbin` seguiriam vivos (e tocando) até o processo terminar.
impl Drop for Player {
    fn drop(&mut self) {
        self.encerrar.store(true, Ordering::SeqCst);
        for d in &self.decks {
            let _ = d.set_state(gst::State::Null);
        }
    }
}

fn criar_deck() -> Result<(gst::Element, Option<gst::Element>), String> {
    let playbin = gst::ElementFactory::make("playbin")
        .build()
        .map_err(|_| "Elemento playbin do GStreamer indisponível.".to_string())?;
    // Só áudio: capas embutidas como fluxo de vídeo não abrem janela.
    if let Ok(fake) = gst::ElementFactory::make("fakesink").build() {
        playbin.set_property("video-sink", &fake);
    }
    let eq = gst::ElementFactory::make("equalizer-10bands").build().ok();
    // `scaletempo` (plugin audiofx) muda a velocidade sem mudar o tom; a 1× só repassa o áudio.
    let tempo = gst::ElementFactory::make("scaletempo").build().ok();
    let filtro = match (tempo, &eq) {
        (Some(t), Some(e)) => filtro_tempo_eq(&t, e).or(Some(e.clone())),
        (Some(t), None) => Some(t),
        (None, e) => e.clone(),
    };
    if let Some(f) = &filtro {
        playbin.set_property("audio-filter", f);
    }
    Ok((playbin, eq))
}

/// `scaletempo ! audioconvert ! equalizer-10bands` num bin só (o `audio-filter` aceita um elemento).
fn filtro_tempo_eq(tempo: &gst::Element, eq: &gst::Element) -> Option<gst::Element> {
    let conv = gst::ElementFactory::make("audioconvert").build().ok()?;
    let bin = gst::Bin::new();
    bin.add_many([tempo, &conv, eq]).ok()?;
    gst::Element::link_many([tempo, &conv, eq]).ok()?;
    let entrada = gst::GhostPad::with_target(&tempo.static_pad("sink")?).ok()?;
    let saida = gst::GhostPad::with_target(&eq.static_pad("src")?).ok()?;
    bin.add_pad(&entrada).ok()?;
    bin.add_pad(&saida).ok()?;
    Some(bin.upcast())
}

/// Busca em `ms` mantendo a velocidade `taxa` (o `seek_simple` voltaria a 1×).
fn buscar_com_taxa(deck: &gst::Element, ms: u64, taxa: f64) -> Result<(), gst::glib::BoolError> {
    deck.seek(
        taxa,
        gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
        gst::SeekType::Set,
        Some(gst::ClockTime::from_mseconds(ms)),
        gst::SeekType::None,
        gst::ClockTime::NONE,
    )
}

fn uri_de(path: &str) -> Result<String, String> {
    gst::glib::filename_to_uri(path, None).map(|u| u.to_string()).map_err(|e| e.to_string())
}

impl Player {
    /// `avisar` recebe cada mudança de estado e o progresso (a cada 250 ms tocando).
    /// `pedir_proxima` é chamada perto do fim da faixa (uma vez por faixa) e devolve a
    /// seguinte. `None`: a reprodução termina normalmente.
    pub fn new(
        avisar: impl Fn(EstadoPlayer) + Send + 'static,
        pedir_proxima: impl Fn() -> Option<Proxima> + Send + Sync + 'static,
    ) -> Result<Self, String> {
        gst_iniciar().map_err(|e| format!("Falha ao iniciar o GStreamer: {e}"))?;
        let ((d0, e0), (d1, e1)) = (criar_deck()?, criar_deck()?);
        let (decks, eqs) = ([d0, d1], [e0, e1]);
        let int = Arc::new(Mutex::new(Interno {
            ativo: 0,
            atual: None,
            pendente: None,
            escolha: Escolha::NaoPedida,
            cruzando: None,
            volume: 1.0,
            crossfade_ms: 0,
            normalizacao: ganho::Modo::default(),
            ganhos: [Ganho::default(); 2],
            pre_eq: 1.0,
            atenuacao: 1.0,
            velocidade: 1.0,
            taxa: [1.0; 2],
        }));
        let pedir: Arc<Pedir> = Arc::new(pedir_proxima);
        for (i, deck) in decks.iter().enumerate() {
            let int = int.clone();
            let pedir = pedir.clone();
            deck.connect("about-to-finish", false, move |args| {
                let pb = args[0].get::<gst::Element>().ok()?;
                // Só o deck ativo, fora de um crossfade, emenda a próxima.
                let escolha = {
                    let mut g = int.lock().unwrap();
                    if g.ativo != i || g.cruzando.is_some() {
                        return None;
                    }
                    std::mem::replace(&mut g.escolha, Escolha::Pedida(None))
                };
                let proxima = match escolha {
                    Escolha::NaoPedida => pedir(),
                    Escolha::Pedida(p) => p,
                };
                if let Some(p) = proxima {
                    if let Ok(uri) = uri_de(&p.path) {
                        pb.set_property("uri", uri.as_str());
                        let mut g = int.lock().unwrap();
                        if g.ativo == i {
                            g.pendente = Some((p.id, p.ganho));
                        }
                    }
                }
                None
            });
        }
        let encerrar = Arc::new(AtomicBool::new(false));
        vigiar(avisar, decks.clone(), int.clone(), pedir, encerrar.clone());
        Ok(Self { decks, eqs, int, encerrar })
    }

    fn ativo(&self) -> usize {
        self.int.lock().unwrap().ativo
    }

    pub fn tocar(&self, id: i64, path: &str, ganho: Ganho) -> Result<(), String> {
        let uri = uri_de(path)?;
        {
            let mut g = self.int.lock().unwrap();
            g.pendente = None;
            g.cruzando = None;
            g.escolha = Escolha::NaoPedida;
        }
        for d in &self.decks {
            d.set_state(gst::State::Null).map_err(|e| e.to_string())?;
        }
        let (ativo, volume) = {
            let mut g = self.int.lock().unwrap();
            g.pendente = None;
            g.escolha = Escolha::NaoPedida;
            g.atual = Some(id);
            let ativo = g.ativo;
            g.ganhos[ativo] = ganho;
            g.taxa[ativo] = 1.0;
            (ativo, g.alvo(ativo))
        };
        let deck = &self.decks[ativo];
        deck.set_property("volume", volume);
        deck.set_property("uri", uri.as_str());
        deck.set_state(gst::State::Playing)
            .map_err(|_| "Não foi possível tocar este arquivo.".to_string())?;
        Ok(())
    }

    /// Carrega `id` pausado em `posicao_ms` (retomar ao reabrir). Espera o deck ficar pronto
    /// (até 5 s) para poder buscar a posição.
    pub fn carregar_pausado(&self, id: i64, path: &str, ganho: Ganho, posicao_ms: u64) -> Result<(), String> {
        let uri = uri_de(path)?;
        let (ativo, volume) = {
            let mut g = self.int.lock().unwrap();
            g.pendente = None;
            g.cruzando = None;
            g.escolha = Escolha::NaoPedida;
            g.atual = Some(id);
            let ativo = g.ativo;
            g.ganhos[ativo] = ganho;
            g.taxa[ativo] = 1.0;
            (ativo, g.alvo(ativo))
        };
        let deck = &self.decks[ativo];
        let _ = deck.set_state(gst::State::Null);
        deck.set_property("volume", volume);
        deck.set_property("uri", uri.as_str());
        deck.set_state(gst::State::Paused)
            .map_err(|_| "Não foi possível abrir este arquivo.".to_string())?;
        let (pronto, _, _) = deck.state(gst::ClockTime::from_seconds(5));
        if pronto.is_err() {
            let _ = deck.set_state(gst::State::Null);
            self.int.lock().unwrap().atual = None;
            return Err("Não foi possível abrir este arquivo.".into());
        }
        // Já com a velocidade pedida (um `seek_simple` voltaria a 1×).
        let v = {
            let mut g = self.int.lock().unwrap();
            g.taxa[ativo] = g.velocidade;
            g.velocidade
        };
        if posicao_ms > 0 || (v - 1.0).abs() > 1e-3 {
            let _ = buscar_com_taxa(deck, posicao_ms, v);
        }
        Ok(())
    }

    /// Toca `id` a partir de `posicao_ms` (episódio de podcast retomado).
    pub fn tocar_em(&self, id: i64, path: &str, ganho: Ganho, posicao_ms: u64) -> Result<(), String> {
        for d in &self.decks {
            let _ = d.set_state(gst::State::Null);
        }
        self.carregar_pausado(id, path, ganho, posicao_ms)?;
        self.decks[self.ativo()]
            .set_state(gst::State::Playing)
            .map_err(|_| "Não foi possível tocar este arquivo.".to_string())?;
        Ok(())
    }

    /// Velocidade de reprodução (0,5× a 3×), sem mudar o tom. Vale para a faixa atual e as
    /// seguintes até mudar de novo; o app põe 1× para música.
    pub fn definir_velocidade(&self, v: f64) {
        let v = v.clamp(0.5, 3.0);
        let (ativo, aplicar) = {
            let mut g = self.int.lock().unwrap();
            g.velocidade = v;
            let ativo = g.ativo;
            let pronto = matches!(self.decks[ativo].current_state(), gst::State::Playing | gst::State::Paused);
            let aplicar = pronto && g.atual.is_some() && (g.taxa[ativo] - v).abs() > 1e-3;
            if aplicar {
                g.taxa[ativo] = v;
            }
            (ativo, aplicar)
        };
        if aplicar {
            let deck = &self.decks[ativo];
            let _ = buscar_com_taxa(deck, posicao(deck), v);
        }
    }

    /// Estado atual (para a interface ao abrir, antes do primeiro aviso).
    pub fn estado(&self) -> EstadoPlayer {
        let (ativo, atual) = {
            let g = self.int.lock().unwrap();
            (g.ativo, g.atual)
        };
        estado_de(&self.decks[ativo], atual)
    }

    /// Os dois decks sem som, mas no ritmo real (`sync`).
    #[cfg(test)]
    pub(crate) fn silenciar_para_teste(&self) {
        for d in &self.decks {
            let mudo = gst::ElementFactory::make("fakesink").property("sync", true).build().unwrap();
            d.set_property("audio-sink", &mudo);
        }
    }

    /// Alterna entre tocar e pausar (os dois decks, durante um crossfade).
    pub fn alternar_pausa(&self) -> Result<(), String> {
        let (ativo, saindo) = {
            let g = self.int.lock().unwrap();
            if g.atual.is_none() {
                return Ok(());
            }
            (g.ativo, g.cruzando.as_ref().map(|c| c.saindo))
        };
        let alvo = if alvo_de(&self.decks[ativo]) == gst::State::Playing {
            gst::State::Paused
        } else {
            gst::State::Playing
        };
        if let Some(s) = saindo {
            let _ = self.decks[s].set_state(alvo);
        }
        self.decks[ativo].set_state(alvo).map(|_| ()).map_err(|e| e.to_string())
    }

    /// Pausa se estiver tocando (temporizador). Não retoma nada.
    pub fn pausar(&self) -> Result<(), String> {
        let ativo = {
            let g = self.int.lock().unwrap();
            if g.atual.is_none() {
                return Ok(());
            }
            g.ativo
        };
        if alvo_de(&self.decks[ativo]) == gst::State::Playing {
            self.alternar_pausa()?;
        }
        Ok(())
    }

    /// Reduz o volume por um fator de 0 a 1 (atenuação do temporizador).
    pub fn atenuar(&self, fator: f64) {
        let mut g = self.int.lock().unwrap();
        g.atenuacao = fator.clamp(0.0, 1.0);
        self.aplicar_volume(&g);
    }

    /// Volume de 0 a 1 na escala percebida; convertido para linear (cúbico).
    pub fn volume(&self, v: f64) {
        let v = v.clamp(0.0, 1.0);
        let mut g = self.int.lock().unwrap();
        g.volume = v * v * v;
        self.aplicar_volume(&g);
    }

    /// Normalização (ReplayGain); vale na hora.
    pub fn definir_normalizacao(&self, modo: ganho::Modo) {
        let mut g = self.int.lock().unwrap();
        g.normalizacao = modo;
        self.aplicar_volume(&g);
    }

    /// Preset do equalizador; vale na hora, nos dois decks.
    pub fn definir_equalizador(&self, preset: eq::Preset) {
        for e in self.eqs.iter().flatten() {
            for (i, db) in preset.bandas().into_iter().enumerate() {
                e.set_property(&format!("band{i}"), db);
            }
        }
        let mut g = self.int.lock().unwrap();
        g.pre_eq = if self.eqs.iter().any(Option::is_some) { preset.pre_atenuacao() } else { 1.0 };
        self.aplicar_volume(&g);
    }

    /// Durante o crossfade, o próximo passo da rampa aplica o volume novo.
    fn aplicar_volume(&self, g: &Interno) {
        if g.cruzando.is_none() {
            self.decks[g.ativo].set_property("volume", g.alvo(g.ativo));
        }
    }

    /// Duração do crossfade em ms (0 desliga). Vale a partir da próxima troca.
    pub fn definir_crossfade(&self, ms: u64) {
        self.int.lock().unwrap().crossfade_ms = ms;
    }

    pub fn posicao_ms(&self) -> u64 {
        posicao(&self.decks[self.ativo()])
    }

    /// Buscar durante um crossfade encerra o crossfade (a faixa que saía para).
    pub fn buscar(&self, ms: u64) -> Result<(), String> {
        let (ativo, saindo) = {
            let mut g = self.int.lock().unwrap();
            (g.ativo, g.cruzando.take().map(|c| c.saindo))
        };
        if let Some(s) = saindo {
            let _ = self.decks[s].set_state(gst::State::Null);
            let volume = self.int.lock().unwrap().alvo(ativo);
            self.decks[ativo].set_property("volume", volume);
        }
        let taxa = self.int.lock().unwrap().taxa[ativo];
        buscar_com_taxa(&self.decks[ativo], ms, taxa).map_err(|_| "Não foi possível mudar a posição.".to_string())
    }
}

fn posicao(deck: &gst::Element) -> u64 {
    deck.query_position::<gst::ClockTime>().map_or(0, |t| t.mseconds())
}

fn duracao(deck: &gst::Element) -> u64 {
    deck.query_duration::<gst::ClockTime>().map_or(0, |t| t.mseconds())
}

/// Estado para onde o deck está indo (evita "pausado" de relance enquanto ele prepara).
fn alvo_de(deck: &gst::Element) -> gst::State {
    let (_, atual, pendente) = deck.state(gst::ClockTime::ZERO);
    if pendente == gst::State::VoidPending { atual } else { pendente }
}

fn estado_de(deck: &gst::Element, atual: Option<i64>) -> EstadoPlayer {
    let estado = match (atual, alvo_de(deck)) {
        (None, _) => Estado::Parado,
        (_, gst::State::Playing) => Estado::Tocando,
        (_, gst::State::Paused) => Estado::Pausado,
        _ => Estado::Parado,
    };
    EstadoPlayer {
        id: atual,
        estado,
        posicao_ms: posicao(deck),
        duracao_ms: duracao(deck),
        erro: None,
        fim: false,
        transicao: false,
    }
}

/// Lê os barramentos dos dois decks, conduz o crossfade e emite o progresso.
fn vigiar(
    avisar: impl Fn(EstadoPlayer) + Send + 'static,
    decks: [gst::Element; 2],
    int: Arc<Mutex<Interno>>,
    pedir: Arc<Pedir>,
    encerrar: Arc<AtomicBool>,
) {
    let buses = [decks[0].bus().expect("playbin sempre tem barramento"), decks[1].bus().expect("playbin sempre tem barramento")];
    std::thread::spawn(move || {
        let mut ultimo_envio = Instant::now();
        while !encerrar.load(Ordering::SeqCst) {
            let mut envio_imediato = false;
            let mut erro = None;
            let mut fim = false;
            let mut transicao = false;

            for (i, bus) in buses.iter().enumerate() {
                let Some(msg) = bus.timed_pop(gst::ClockTime::from_mseconds(25)) else { continue };
                let ativo = int.lock().unwrap().ativo;
                match msg.view() {
                    gst::MessageView::Eos(..) | gst::MessageView::Error(..) if i != ativo => {
                        // A faixa que saía no crossfade acabou (ou falhou): fim do crossfade.
                        let _ = decks[i].set_state(gst::State::Null);
                        terminar_cruzamento(&decks, &int);
                    }
                    gst::MessageView::Eos(..) => {
                        let _ = decks[i].set_state(gst::State::Null);
                        let mut g = int.lock().unwrap();
                        g.atual = None;
                        g.pendente = None;
                        envio_imediato = true;
                        fim = true;
                    }
                    gst::MessageView::Error(e) => {
                        let _ = decks[i].set_state(gst::State::Null);
                        let mut g = int.lock().unwrap();
                        g.atual = None;
                        g.pendente = None;
                        erro = Some(format!("Erro ao tocar: {}", e.error()));
                        envio_imediato = true;
                    }
                    // A faixa emendada pelo `about-to-finish` começou a tocar (gapless).
                    gst::MessageView::StreamStart(..) if i == ativo => {
                        let mut g = int.lock().unwrap();
                        if let Some((id, ganho)) = g.pendente.take() {
                            g.atual = Some(id);
                            // A faixa emendada traz o próprio ganho.
                            g.ganhos[i] = ganho;
                            let volume = g.alvo(i);
                            decks[i].set_property("volume", volume);
                            g.escolha = Escolha::NaoPedida;
                            envio_imediato = true;
                            transicao = true;
                        }
                    }
                    // Busca concluída (também pausado): a posição nova vai para a tela.
                    gst::MessageView::AsyncDone(..) if i == ativo => {
                        envio_imediato = true;
                        // Deck pronto (faixa nova ou busca): aplica a velocidade pedida.
                        let v = {
                            let mut g = int.lock().unwrap();
                            let v = g.velocidade;
                            let mudar = g.atual.is_some() && (g.taxa[i] - v).abs() > 1e-3;
                            if mudar {
                                g.taxa[i] = v;
                            }
                            mudar.then_some(v)
                        };
                        if let Some(v) = v {
                            let _ = buscar_com_taxa(&decks[i], posicao(&decks[i]), v);
                        }
                    }
                    gst::MessageView::StateChanged(s)
                        if i == ativo && s.src().is_some_and(|src| src == decks[i].upcast_ref::<gst::Object>()) =>
                    {
                        envio_imediato = true;
                    }
                    _ => {}
                }
            }

            if conduzir_cruzamento(&decks, &int, pedir.as_ref()) {
                envio_imediato = true;
                transicao = true;
            }

            let (ativo, atual) = {
                let g = int.lock().unwrap();
                (g.ativo, g.atual)
            };
            let tocando = decks[ativo].current_state() == gst::State::Playing;
            if envio_imediato || (tocando && ultimo_envio.elapsed() >= Duration::from_millis(250)) {
                let mut est = estado_de(&decks[ativo], atual);
                est.erro = erro;
                est.fim = fim;
                est.transicao = transicao;
                avisar(est);
                ultimo_envio = Instant::now();
            }
        }
    });
}

fn terminar_cruzamento(decks: &[gst::Element; 2], int: &Mutex<Interno>) {
    let (ativo, volume, saindo) = {
        let mut g = int.lock().unwrap();
        (g.ativo, g.alvo(g.ativo), g.cruzando.take().map(|c| c.saindo))
    };
    if let Some(s) = saindo {
        let _ = decks[s].set_state(gst::State::Null);
    }
    decks[ativo].set_property("volume", volume);
}

/// Um passo do crossfade: aplica a rampa de volume ou, perto do fim da faixa, pede a
/// próxima e começa o crossfade. Devolve `true` quando outra faixa começou a tocar.
fn conduzir_cruzamento(decks: &[gst::Element; 2], int: &Mutex<Interno>, pedir: &Pedir) -> bool {
    let mut g = int.lock().unwrap();
    let ativo = g.ativo;

    if let Some(c) = &g.cruzando {
        // Progresso pela posição da faixa nova: pausar no meio pausa a rampa.
        let t = (posicao(&decks[ativo]) as f64 / c.dur_ms.max(1) as f64).clamp(0.0, 1.0);
        let saindo = c.saindo;
        let (entra, sai) = (g.alvo(ativo), g.alvo(saindo));
        drop(g);
        if t >= 1.0 {
            terminar_cruzamento(decks, int);
        } else {
            // Potência constante: a soma não "afunda" no meio.
            decks[ativo].set_property("volume", entra * (t * FRAC_PI_2).sin());
            decks[saindo].set_property("volume", sai * (t * FRAC_PI_2).cos());
        }
        return false;
    }

    if g.crossfade_ms == 0 || g.atual.is_none() || !matches!(g.escolha, Escolha::NaoPedida) {
        return false;
    }
    let deck = &decks[ativo];
    if deck.current_state() != gst::State::Playing {
        return false;
    }
    let dur = duracao(deck);
    let cf = g.crossfade_ms.min(dur / 2);
    if dur == 0 || cf == 0 || posicao(deck) + cf < dur {
        return false;
    }
    // Reserva a escolha antes de soltar a trava (o `about-to-finish` não pede de novo).
    g.escolha = Escolha::Pedida(None);
    drop(g);

    let p = match pedir() {
        Some(p) if !p.mesmo_album => p,
        // Mesmo álbum (ou fim da fila): fica para o gapless.
        outra => {
            int.lock().unwrap().escolha = Escolha::Pedida(outra);
            return false;
        }
    };
    let Ok(uri) = uri_de(&p.path) else { return false };
    let novo = 1 - ativo;
    let volume = {
        let mut g = int.lock().unwrap();
        g.ativo = novo;
        g.atual = Some(p.id);
        g.pendente = None;
        g.escolha = Escolha::NaoPedida;
        g.cruzando = Some(Cruzamento { saindo: ativo, dur_ms: cf });
        g.ganhos[novo] = p.ganho;
        g.taxa[novo] = 1.0;
        g.alvo(ativo)
    };
    let _ = decks[novo].set_state(gst::State::Null);
    decks[novo].set_property("volume", 0.0);
    decks[novo].set_property("uri", uri.as_str());
    if decks[novo].set_state(gst::State::Playing).is_err() {
        // Não deu para tocar a nova: desfaz e deixa a atual terminar.
        let mut g = int.lock().unwrap();
        g.ativo = ativo;
        g.atual = None;
        g.cruzando = None;
        g.escolha = Escolha::Pedida(None);
        drop(g);
        decks[ativo].set_property("volume", volume);
        return false;
    }
    true
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;

    fn silenciar(p: &Player) {
        p.silenciar_para_teste();
    }

    /// Toca a.mp3 (3 s) da pasta de teste sem som (fakesink) e espera o fim da faixa.
    /// Rodar com: SKALD_TEST_DIR=/pasta cargo test player -- --nocapture
    #[test]
    fn toca_ate_o_fim() {
        let Ok(dir) = std::env::var("SKALD_TEST_DIR") else { return };
        let (tx, rx) = mpsc::channel();
        let p = Player::new(move |e| { let _ = tx.send(e); }, || None).unwrap();
        silenciar(&p);
        p.volume(0.5);
        p.tocar(7, &format!("{dir}/a.mp3"), Ganho::default()).unwrap();

        let mut viu_tocando = false;
        let mut maior_posicao = 0;
        let fim = loop {
            let e = rx.recv_timeout(Duration::from_secs(10)).expect("sem eventos do player");
            if e.estado == Estado::Tocando {
                viu_tocando = true;
                maior_posicao = maior_posicao.max(e.posicao_ms);
                assert_eq!(e.id, Some(7));
            }
            if viu_tocando && e.estado == Estado::Parado {
                assert!(e.fim, "o aviso de parada deve marcar fim de faixa");
                break e;
            }
        };
        assert!(maior_posicao > 2000);
        assert_eq!(fim.id, None);
        assert!(fim.erro.is_none());
    }

    #[test]
    fn arquivo_invalido_gera_erro() {
        let Ok(dir) = std::env::var("SKALD_TEST_DIR") else { return };
        let (tx, rx) = mpsc::channel();
        let p = Player::new(move |e| { let _ = tx.send(e); }, || None).unwrap();
        let _ = p.tocar(1, &format!("{dir}/sub/quebrado.ogg"), Ganho::default());
        let erro = loop {
            let e = rx.recv_timeout(Duration::from_secs(10)).expect("sem eventos do player");
            if let Some(m) = e.erro { break m; }
        };
        println!("erro: {erro}");
    }

    /// Gera um WAV de tom de 440 Hz com `buffers` blocos de 1024 amostras (44,1 kHz).
    fn gerar_wav(destino: &std::path::Path, buffers: u32) {
        crate::teste_util::wav(destino, buffers * 1024, 0);
    }

    struct Resultado {
        /// Momento da transição, contado do início da primeira faixa.
        transicao: Option<(EstadoPlayer, Duration)>,
        parou_antes: bool,
    }

    /// Toca a.wav e oferece b.wav (id 2) como próxima, uma vez. Devolve o que aconteceu.
    fn tocar_duas(nome: &str, buffers: u32, crossfade_ms: u64, mesmo_album: bool) -> Option<Resultado> {
        // Espaço e acento no caminho, como em "Música/Álbum de Fulano" (URIs no Windows).
        let dir = crate::teste_util::pasta(&format!("skald teste ção {nome}"));
        let (a, b) = (dir.join("faixa á.wav"), dir.join("faixa é.wav"));
        gerar_wav(&a, buffers);
        gerar_wav(&b, buffers);
        let pedida = AtomicBool::new(false);
        let b_str = b.to_string_lossy().into_owned();
        let (tx, rx) = mpsc::channel();
        let p = Player::new(
            move |e| { let _ = tx.send(e); },
            move || {
                (!pedida.swap(true, Ordering::SeqCst)).then(|| Proxima { id: 2, path: b_str.clone(), mesmo_album, ganho: Ganho::default() })
            },
        )
        .unwrap();
        silenciar(&p);
        p.definir_crossfade(crossfade_ms);
        let inicio = Instant::now();
        p.tocar(1, &a.to_string_lossy(), Ganho::default()).unwrap();

        let mut r = Resultado { transicao: None, parou_antes: false };
        loop {
            let e = rx.recv_timeout(Duration::from_secs(15)).expect("sem eventos do player");
            assert!(e.erro.is_none(), "{:?}", e.erro);
            if e.transicao {
                r.transicao = Some((e.clone(), inicio.elapsed()));
            }
            if e.fim {
                assert_eq!(e.id, None);
                break;
            }
            if r.transicao.is_none() && e.estado == Estado::Parado && e.id.is_none() {
                r.parou_antes = true;
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
        Some(r)
    }

    /// Podcast: começa em 1 s de uma faixa de ~4 s a 2× (sem mudar o tom: `scaletempo`);
    /// os ~3 s restantes devem passar em ~1,5 s. Mede quando a posição chega ao fim: no
    /// Windows (GStreamer 1.28.8) o EOS a 2× vem ~1 s depois disso, perto de 2,9 s.
    #[test]
    fn velocidade_e_retomar() {
        let dir = crate::teste_util::pasta("skald-teste-velocidade");
        let a = dir.join("episódio.wav");
        gerar_wav(&a, 172);
        let (tx, rx) = mpsc::channel();
        let p = Player::new(move |e| { let _ = tx.send(e); }, || None).unwrap();
        silenciar(&p);
        let filtro: gst::Element = p.decks[0].property("audio-filter");
        let tem_tempo = filtro.factory().is_some_and(|f| f.name() == "scaletempo")
            || filtro.downcast_ref::<gst::Bin>().is_some_and(|b| b.by_name("scaletempo0").is_some()
                || b.iterate_elements().into_iter().flatten().any(|e| e.factory().is_some_and(|f| f.name() == "scaletempo")));
        assert!(tem_tempo, "scaletempo fora do filtro de áudio");
        p.definir_velocidade(2.0);
        let inicio = Instant::now();
        p.tocar_em(1, &a.to_string_lossy(), Ganho::default(), 1000).unwrap();
        // (ms desde o início, posição em ms): diz, se falhar, onde começou e em que ritmo andou.
        let mut posicoes = Vec::new();
        let mut chegou = None;
        loop {
            let e = rx.recv_timeout(Duration::from_secs(10)).expect("sem eventos do player");
            assert!(e.erro.is_none(), "{:?}", e.erro);
            if e.fim {
                break;
            }
            posicoes.push((inicio.elapsed().as_millis(), e.posicao_ms));
            if e.posicao_ms >= 3900 && chegou.is_none() {
                chegou = Some(inicio.elapsed());
            }
        }
        assert!(posicoes.first().is_some_and(|&(_, p)| (900..1600).contains(&p)), "não começou em 1 s: {posicoes:?}");
        let t = chegou.unwrap_or_else(|| panic!("não chegou ao fim: {posicoes:?}"));
        assert!(t > Duration::from_millis(900) && t < Duration::from_millis(2200), "chegou ao fim em {t:?}; posições {posicoes:?}");
        drop(p);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Mesmo sem crossfade: a segunda entra pelo `about-to-finish`, sem o player parar.
    #[test]
    fn troca_sem_pausa() {
        let Some(r) = tocar_duas("gapless", 65, 0, false) else { return };
        let (t, _) = r.transicao.expect("faltou o aviso de transição");
        assert_eq!(t.id, Some(2));
        assert_eq!(t.estado, Estado::Tocando);
        assert!(!r.parou_antes, "o player parou entre as faixas");
    }

    /// Álbuns diferentes: a próxima começa ~1,5 s antes do fim da primeira (4 s).
    #[test]
    fn crossfade_entre_albuns() {
        let Some(r) = tocar_duas("crossfade", 172, 1500, false) else { return };
        let (t, quando) = r.transicao.expect("faltou o aviso de transição");
        assert_eq!(t.id, Some(2));
        assert!(!r.parou_antes);
        assert!(quando < Duration::from_millis(3300), "crossfade começou tarde: {quando:?}");
        assert!(quando > Duration::from_millis(1800), "crossfade começou cedo: {quando:?}");
    }

    /// Mesmo álbum: nada de crossfade, a troca acontece no fim (gapless).
    #[test]
    fn mesmo_album_sem_crossfade() {
        let Some(r) = tocar_duas("mesmo-album", 172, 1500, true) else { return };
        let (t, quando) = r.transicao.expect("faltou o aviso de transição");
        assert_eq!(t.id, Some(2));
        assert!(!r.parou_antes);
        assert!(quando > Duration::from_millis(3600), "trocou antes do fim: {quando:?}");
    }

    /// Uma música de cada extensão da biblioteca (e cada arquivo de `SKALD_FORMATOS_DIR`),
    /// tocada sem som por ~2 s.
    /// Rodar com: SKALD_DB_ENSAIO=/copia.db cargo test ensaio_formatos -- --nocapture
    #[test]
    fn ensaio_formatos() {
        let mut arquivos: Vec<(String, String)> = Vec::new();
        if let Ok(caminho) = std::env::var("SKALD_DB_ENSAIO") {
            let conn = crate::db::open(std::path::Path::new(&caminho)).unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT lower(replace(path, rtrim(path, replace(path, '.', '')), '')) AS ext, min(path)
                     FROM media WHERE missing = 0 GROUP BY ext",
                )
                .unwrap();
            arquivos.extend(stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap));
        }
        if let Ok(dir) = std::env::var("SKALD_FORMATOS_DIR") {
            for f in std::fs::read_dir(dir).unwrap().flatten() {
                let p = f.path();
                let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                arquivos.push((ext, p.to_string_lossy().into_owned()));
            }
        }
        if arquivos.is_empty() {
            return;
        }
        let mut falhas = Vec::new();
        for (ext, path) in &arquivos {
            let (tx, rx) = mpsc::channel();
            let p = Player::new(move |e| { let _ = tx.send(e); }, || None).unwrap();
            silenciar(&p);
            p.tocar(1, path, Ganho::default()).unwrap();
            let limite = Instant::now() + Duration::from_secs(8);
            let mut resultado = Err("sem progresso".to_string());
            while Instant::now() < limite {
                let Ok(e) = rx.recv_timeout(Duration::from_secs(8)) else { break };
                if let Some(erro) = e.erro {
                    resultado = Err(erro);
                    break;
                }
                if e.estado == Estado::Tocando && e.posicao_ms > 1500 {
                    resultado = Ok(e.duracao_ms);
                    break;
                }
            }
            match resultado {
                Ok(d) => println!("ok  {ext:5} {d:>7} ms  {path}"),
                Err(m) => {
                    println!("ERRO {ext:5} {m}  {path}");
                    falhas.push(ext.clone());
                }
            }
        }
        assert!(falhas.is_empty(), "formatos com erro: {falhas:?}");
    }

    #[test]
    fn equalizador_aplica_bandas_nos_dois_decks() {
        let p = Player::new(|_| {}, || None).unwrap();
        if p.eqs.iter().any(Option::is_none) {
            eprintln!("equalizer-10bands indisponível; teste ignorado");
            return;
        }
        p.definir_equalizador(eq::Preset::Grave);
        for e in p.eqs.iter().flatten() {
            assert_eq!(e.property::<f64>("band0"), 6.0);
            assert_eq!(e.property::<f64>("band9"), 0.0);
        }
        assert!((p.int.lock().unwrap().pre_eq - eq::Preset::Grave.pre_atenuacao()).abs() < 1e-9);
        p.definir_equalizador(eq::Preset::Desligado);
        assert_eq!(p.eqs[1].as_ref().unwrap().property::<f64>("band0"), 0.0);
        assert_eq!(p.int.lock().unwrap().pre_eq, 1.0);
    }
}
