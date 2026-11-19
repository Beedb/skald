//! Reconhecer a música pelo som (como o Shazam, mas aberto): calcula a impressão digital
//! Chromaprint dos primeiros 120 s (GStreamer decodifica; `rusty-chromaprint` calcula) e
//! consulta o AcoustID, que devolve a gravação do MusicBrainz. Serve para arquivos com nome
//! e tags errados. Só roda a pedido e precisa de internet e da chave do aplicativo
//! (`SKALD_ACOUSTID_KEY`, embutida na compilação); sem ela, fica indisponível.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use gstreamer as gst;
use gst::prelude::*;
use rusty_chromaprint::{Configuration, FingerprintCompressor, Fingerprinter};
use serde_json::Value;

use crate::enriquecer::{self, Candidato};

const ACOUSTID: &str = "https://api.acoustid.org/v2/lookup";
/// O AcoustID recomenda os primeiros 120 s.
const SEGUNDOS: u64 = 120;
const TAXA: u32 = 11025;

pub fn chave() -> Option<&'static str> {
    option_env!("SKALD_ACOUSTID_KEY").filter(|c| !c.is_empty())
}

pub fn disponivel() -> bool {
    chave().is_some()
}

/// Impressão digital compactada (base64 de URL, como o `fpcalc`) e a duração em segundos.
pub fn impressao(path: &Path) -> Result<(String, u64), String> {
    crate::player::gst_iniciar().map_err(|e| e.to_string())?;
    let uri = gst::glib::filename_to_uri(path, None).map_err(|e| e.to_string())?;
    let pipeline = gst::parse::launch(&format!(
        "uridecodebin name=origem ! audioconvert ! audioresample ! \
         audio/x-raw,format=S16LE,channels=1,rate={TAXA},layout=interleaved ! \
         fakesink name=saida sync=false signal-handoffs=true"
    ))
    .map_err(|e| format!("Reconhecimento indisponível: {e}"))?
    .downcast::<gst::Pipeline>()
    .map_err(|_| "Pipeline inválido.".to_string())?;
    pipeline.by_name("origem").ok_or("uridecodebin ausente")?.set_property("uri", uri.as_str());

    let config = Configuration::preset_test2();
    let mut calc = Fingerprinter::new(&config);
    calc.start(TAXA, 1).map_err(|e| format!("{e:?}"))?;
    let limite = (SEGUNDOS * TAXA as u64) as usize;
    // O fakesink entrega os blocos numa thread do GStreamer; o cálculo fica aqui.
    let (tx, rx) = std::sync::mpsc::channel::<Vec<i16>>();
    let tx = Mutex::new(tx);
    pipeline.by_name("saida").ok_or("fakesink ausente")?.connect("handoff", false, move |args| {
        let buf = args.get(1).and_then(|v| v.get::<gst::Buffer>().ok())?;
        let mapa = buf.map_readable().ok()?;
        let amostras = mapa.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        let _ = tx.lock().ok()?.send(amostras);
        None
    });
    let bus = pipeline.bus().ok_or("Sem barramento.")?;
    pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
    let inicio = Instant::now();
    let mut lidas = 0usize;
    let mut erro = None;
    let mut fim = false;
    // Para no fim do arquivo, nos 120 s de áudio ou em 60 s de relógio (arquivo travado).
    while lidas < limite && inicio.elapsed() < Duration::from_secs(60) {
        while let Ok(bloco) = rx.try_recv() {
            let n = (limite - lidas).min(bloco.len());
            calc.consume(&bloco[..n]);
            lidas += n;
        }
        if fim {
            break;
        }
        if let Some(msg) = bus.timed_pop(gst::ClockTime::from_mseconds(50)) {
            match msg.view() {
                // Ainda pode haver blocos no canal: lê na próxima volta e sai.
                gst::MessageView::Eos(_) => fim = true,
                gst::MessageView::Error(e) => {
                    erro = Some(e.error().to_string());
                    break;
                }
                _ => {}
            }
        }
    }
    let duracao = pipeline.query_duration::<gst::ClockTime>().map(|d| d.seconds());
    let _ = pipeline.set_state(gst::State::Null);
    if let Some(e) = erro {
        return Err(format!("Não foi possível ler o áudio: {e}"));
    }
    if lidas < (10 * TAXA) as usize {
        return Err("Áudio curto demais para reconhecer.".into());
    }
    calc.finish();
    let compacta = FingerprintCompressor::from(&config).compress(calc.fingerprint());
    let texto = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compacta);
    Ok((texto, duracao.unwrap_or(lidas as u64 / TAXA as u64)))
}

/// Gravações (id do MusicBrainz e pontuação 0 a 1) da resposta do AcoustID, melhor primeiro.
pub fn gravacoes(json: &Value) -> Vec<(String, f64)> {
    let mut v: Vec<(String, f64)> = Vec::new();
    for r in json["results"].as_array().into_iter().flatten() {
        let score = r["score"].as_f64().unwrap_or(0.0);
        for g in r["recordings"].as_array().into_iter().flatten() {
            if let Some(id) = g["id"].as_str().filter(|id| id.len() == 36) {
                if !v.iter().any(|(x, _)| x == id) {
                    v.push((id.to_string(), score));
                }
            }
        }
    }
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

/// Reconhece pelo som e devolve candidatos no mesmo formato da busca por texto.
/// Só aceita resultado com pontuação ≥ 0,5 no AcoustID.
pub fn reconhecer(path: &Path, duracao_ms: Option<i64>) -> Result<Vec<Candidato>, String> {
    let chave = chave().ok_or("Reconhecimento pelo som indisponível nesta versão.")?;
    let (fp, segundos) = impressao(path)?;
    let duracao = segundos.to_string();
    let corpo = [
        ("client", chave),
        ("meta", "recordingids"),
        ("format", "json"),
        ("duration", duracao.as_str()),
        ("fingerprint", fp.as_str()),
    ];
    let mut sem_rede = 0;
    let mut resposta = loop {
        match enriquecer::agente().post(ACOUSTID).send_form(corpo) {
            Ok(r) => break r,
            Err(e) if enriquecer::transitorio(&e) && sem_rede < 4 => {
                std::thread::sleep(enriquecer::espera_rede(sem_rede));
                sem_rede += 1;
            }
            Err(e) => return Err(enriquecer::erro_rede(e, "AcoustID")),
        }
    };
    let texto = resposta.body_mut().read_to_string().map_err(|e| enriquecer::erro_rede(e, "AcoustID"))?;
    let json: Value = serde_json::from_str(&texto).map_err(|_| "Resposta inesperada do AcoustID.".to_string())?;
    if json["status"].as_str() != Some("ok") {
        let msg = json["error"]["message"].as_str().unwrap_or("erro desconhecido");
        return Err(format!("O AcoustID recusou: {msg}"));
    }
    // As 3 melhores gravações, pelo id no MusicBrainz (mesmo formato da busca por texto).
    let mut lista = Vec::new();
    for (id, score) in gravacoes(&json).into_iter().filter(|(_, s)| *s >= 0.5).take(3) {
        let consulta = format!("rid:{id}");
        let Ok(json) = enriquecer::consultar_mb(
            "https://musicbrainz.org/ws/2/recording",
            &[("query", &consulta), ("fmt", "json"), ("limit", "1")],
        ) else {
            continue;
        };
        let mut c = enriquecer::candidatos(&json, duracao_ms);
        for x in &mut c {
            // A certeza vem do som: pontuação do AcoustID; abaixo de 0,8 a pessoa confere.
            x.pontuacao = x.pontuacao.min((score * 100.0).round() as i64).max(if score >= 0.8 { 90 } else { 0 });
            x.incerto = score < 0.8;
        }
        lista.extend(c);
    }
    lista.sort_by_key(|c| -c.pontuacao);
    Ok(lista)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_resposta_do_acoustid() {
        let json: Value = serde_json::from_str(
            r#"{"status":"ok","results":[
              {"id":"a","score":0.62,"recordings":[{"id":"11111111-1111-1111-1111-111111111111"}]},
              {"id":"b","score":0.97,"recordings":[{"id":"22222222-2222-2222-2222-222222222222"},{"id":"curto"}]}
            ]}"#,
        )
        .unwrap();
        let g = gravacoes(&json);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].0, "22222222-2222-2222-2222-222222222222");
    }

    #[test]
    fn impressao_de_um_wav() {
        let pasta = crate::teste_util::pasta("skald-acustico");
        let wav = pasta.join("tom com acento é.wav");
        crate::teste_util::wav(&wav, 15 * 44_100, 3);
        match impressao(&wav) {
            Ok((fp, s)) => {
                assert!(fp.len() > 20, "{fp}");
                assert!((14..=16).contains(&s), "{s}");
            }
            // Sem GStreamer na máquina de teste: pula.
            Err(e) if e.contains("indisponível") || e.contains("GStreamer") => eprintln!("pulado: {e}"),
            Err(e) => panic!("{e}"),
        }
    }

    /// Reconhecimento real (internet e chave): SKALD_ARQUIVO=/musica.mp3 cargo test ensaio_acustico -- --ignored --nocapture
    #[test]
    #[ignore]
    fn ensaio_acustico() {
        let arquivo = std::env::var("SKALD_ARQUIVO").expect("SKALD_ARQUIVO");
        let c = reconhecer(Path::new(&arquivo), None).unwrap();
        for x in c.iter().take(3) {
            println!("{} | {:?} | {:?} | {:?} | incerto={}", x.pontuacao, x.campos.title, x.campos.artist, x.campos.album, x.incerto);
        }
        assert!(!c.is_empty());
    }
}
