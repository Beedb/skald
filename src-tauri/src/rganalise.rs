//! Análise de ReplayGain (seção 8.6) para arquivos sem as tags: o GStreamer (`rganalysis`)
//! mede o volume e o resultado fica no banco. Os arquivos de música nunca são alterados.
//! Roda em segundo plano, um arquivo por vez, com pausas, e só com a normalização ligada.

use std::path::Path;
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::ganho::{Ganho, Valor};

/// Tempo máximo de análise por arquivo (arquivos enormes ou travados são pulados).
const LIMITE: Duration = Duration::from_secs(300);

/// Mede ganho (dB, referência 89 dB do ReplayGain) e pico de um arquivo.
pub fn analisar(path: &Path) -> Result<Valor, String> {
    crate::player::gst_iniciar().map_err(|e| e.to_string())?;
    let uri = gst::glib::filename_to_uri(path, None).map_err(|e| e.to_string())?;
    let pipeline = gst::parse::launch(
        "uridecodebin name=origem ! audioconvert ! audioresample ! rganalysis name=rg ! fakesink sync=false",
    )
    .map_err(|e| format!("Análise indisponível (rganalysis): {e}"))?
    .downcast::<gst::Pipeline>()
    .map_err(|_| "Pipeline inválido.".to_string())?;
    pipeline.by_name("origem").ok_or("uridecodebin ausente")?.set_property("uri", uri.as_str());
    let bus = pipeline.bus().ok_or("Sem barramento.")?;
    pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
    let (mut ganho, mut pico) = (None, None);
    let mut resultado = Err("A análise não terminou.".to_string());
    while let Some(msg) = bus.timed_pop(gst::ClockTime::from_seconds(LIMITE.as_secs())) {
        use gst::MessageView;
        match msg.view() {
            MessageView::Tag(t) => {
                let tags = t.tags();
                if let Some(g) = tags.get::<gst::tags::TrackGain>() {
                    ganho = Some(g.get());
                }
                if let Some(p) = tags.get::<gst::tags::TrackPeak>() {
                    pico = Some(p.get());
                }
            }
            MessageView::Eos(_) => {
                resultado = ganho.map(|db| Valor { db, pico }).ok_or("Sem resultado da análise.".into());
                break;
            }
            MessageView::Error(e) => {
                resultado = Err(e.error().to_string());
                break;
            }
            _ => {}
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    resultado
}

/// Ganho guardado no banco para quando o arquivo não tem tags. O de álbum só existe com
/// todas as faixas do álbum analisadas (média de energia ponderada pela duração).
pub fn do_banco(conn: &Connection, id: i64) -> Ganho {
    let faixa = conn
        .query_row(
            "SELECT track_gain, track_peak FROM replaygain_analise WHERE media_id = ?1 AND track_gain IS NOT NULL",
            [id],
            |r| Ok(Valor { db: r.get(0)?, pico: r.get(1)? }),
        )
        .optional()
        .ok()
        .flatten();
    let Some(faixa) = faixa else { return Ganho::default() };
    Ganho { faixa: Some(faixa), album: album(conn, id) }
}

fn album(conn: &Connection, id: i64) -> Option<Valor> {
    let chave: crate::colecao::ChaveAlbum = conn
        .query_row(
            &format!(
                "SELECT album, COALESCE(album_artist, {}) FROM media WHERE id = ?1 AND album IS NOT NULL",
                crate::library::PASTA_SQL
            ),
            [id],
            |r| Ok(crate::colecao::ChaveAlbum { album: r.get(0)?, grupo: r.get(1)? }),
        )
        .ok()?;
    let faixas = crate::colecao::faixas_do_album(conn, &chave).ok()?;
    let mut valores = Vec::with_capacity(faixas.len());
    for f in &faixas {
        let v: (f64, Option<f64>) = conn
            .query_row(
                "SELECT track_gain, track_peak FROM replaygain_analise WHERE media_id = ?1 AND track_gain IS NOT NULL",
                [f.media.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()?; // falta alguma faixa: sem ganho de álbum ainda
        valores.push((v.0, v.1, f.media.duration_ms.unwrap_or(0).max(1) as f64));
    }
    ganho_de_album(&valores)
}

/// Média de energia: cada faixa contribui com 10^(-ganho/10) ponderado pela duração.
fn ganho_de_album(faixas: &[(f64, Option<f64>, f64)]) -> Option<Valor> {
    let total: f64 = faixas.iter().map(|f| f.2).sum();
    if faixas.is_empty() || total <= 0.0 {
        return None;
    }
    let energia: f64 = faixas.iter().map(|(g, _, d)| d * 10f64.powf(-g / 10.0)).sum::<f64>() / total;
    let pico = faixas.iter().map(|f| f.1).collect::<Option<Vec<_>>>().map(|p| p.into_iter().fold(0.0, f64::max));
    Some(Valor { db: -10.0 * energia.log10(), pico })
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Situacao {
    /// Arquivos sem tags já analisados.
    pub analisadas: i64,
    /// Arquivos que ainda vão ser vistos (sem registro ou alterados depois).
    pub pendentes: i64,
}

const PENDENTES: &str = "FROM media m LEFT JOIN replaygain_analise r ON r.media_id = m.id
    WHERE m.category = 'music' AND m.missing = 0 AND (r.media_id IS NULL OR r.file_mtime IS NOT m.file_mtime)";

pub fn situacao(conn: &Connection) -> rusqlite::Result<Situacao> {
    Ok(Situacao {
        analisadas: conn.query_row("SELECT COUNT(*) FROM replaygain_analise WHERE tem_tag = 0 AND track_gain IS NOT NULL", [], |r| r.get(0))?,
        pendentes: conn.query_row(&format!("SELECT COUNT(*) {PENDENTES}"), [], |r| r.get(0))?,
    })
}

fn proximo(conn: &Connection) -> rusqlite::Result<Option<(i64, String, Option<i64>)>> {
    conn.query_row(&format!("SELECT m.id, m.path, m.file_mtime {PENDENTES} ORDER BY m.id LIMIT 1"), [], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })
    .optional()
}

/// Lê as tags e, se não houver ReplayGain, analisa. Grava o resultado (inclusive falha,
/// com ganho nulo, para não tentar o mesmo arquivo de novo até ele mudar).
fn processar(conn_db: &std::sync::Mutex<Connection>, id: i64, path: &str, mtime: Option<i64>) {
    let caminho = Path::new(path);
    let tags = crate::ganho::ler(caminho);
    let (tem_tag, valor) = if tags.faixa.is_some() || tags.album.is_some() {
        (true, None)
    } else {
        match analisar(caminho) {
            Ok(v) => (false, Some(v)),
            Err(e) => {
                eprintln!("ReplayGain de {path}: {e}");
                (false, None)
            }
        }
    };
    if let Ok(conn) = conn_db.lock() {
        let _ = conn.execute(
            "INSERT INTO replaygain_analise (media_id, file_mtime, tem_tag, track_gain, track_peak) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(media_id) DO UPDATE SET file_mtime = excluded.file_mtime, tem_tag = excluded.tem_tag,
                 track_gain = excluded.track_gain, track_peak = excluded.track_peak",
            params![id, mtime, tem_tag, valor.map(|v| v.db), valor.and_then(|v| v.pico)],
        );
    }
}

/// Trabalho em segundo plano: um arquivo por vez; sem pendentes ou com a normalização
/// desligada, espera e confere de novo.
pub fn iniciar(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            let db = app.state::<crate::db::Db>();
            let proximo = {
                let Ok(conn) = db.0.lock() else { return };
                let ligada = crate::settings::carregar(&conn).is_ok_and(|c| c.normalizacao != crate::ganho::Modo::Desligada);
                if ligada { proximo(&conn).ok().flatten() } else { None }
            };
            match proximo {
                Some((id, path, mtime)) => {
                    processar(&db.0, id, &path, mtime);
                    std::thread::sleep(Duration::from_millis(300));
                }
                None => std::thread::sleep(Duration::from_secs(60)),
            }
        }
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    /// SKALD_DB_ENSAIO=/copia.db cargo test ensaio_replaygain -- --nocapture (só lê os arquivos)
    #[test]
    fn ensaio_replaygain() {
        let Ok(caminho) = std::env::var("SKALD_DB_ENSAIO") else { return };
        let conn = crate::db::open(Path::new(&caminho)).unwrap();
        let mut stmt = conn.prepare("SELECT path, duration_ms FROM media WHERE missing = 0 ORDER BY id DESC").unwrap();
        let linhas: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .filter(|(p, _): &(String, i64)| Path::new(p).is_file())
            .take(3)
            .collect();
        for (path, dur) in linhas {
            let tags = crate::ganho::ler(Path::new(&path));
            let t = std::time::Instant::now();
            let v = analisar(Path::new(&path));
            println!("tags={} dur={}s análise={:?} em {:?}", tags.faixa.is_some(), dur / 1000, v.map(|v| (v.db, v.pico)), t.elapsed());
        }
    }

    #[test]
    fn media_de_album() {
        // Duas faixas iguais: o álbum tem o mesmo ganho.
        let a = ganho_de_album(&[(-6.0, Some(0.5), 100.0), (-6.0, Some(0.9), 100.0)]).unwrap();
        assert!((a.db + 6.0).abs() < 1e-9);
        assert_eq!(a.pico, Some(0.9));
        // A faixa mais alta (ganho mais negativo) pesa mais na energia.
        let b = ganho_de_album(&[(-10.0, None, 100.0), (0.0, None, 100.0)]).unwrap();
        assert!(b.db < -5.0 && b.db > -10.0);
        assert_eq!(b.pico, None);
        assert!(ganho_de_album(&[]).is_none());
    }

    #[test]
    fn analisa_wav_e_guarda_no_banco() {
        let dir = std::env::temp_dir().join(format!("skald rg ção-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("t.wav");
        crate::teste_util::wav(&wav, 200 * 1024, 0);
        let v = analisar(&wav).unwrap();
        assert!(v.db.is_finite() && v.pico.is_some_and(|p| p > 0.3 && p <= 1.0), "{v:?}");

        let conn = crate::db::open(Path::new(":memory:")).unwrap();
        conn.execute(
            "INSERT INTO media (id, path, title, album, duration_ms, file_mtime) VALUES (1, ?1, 't', 'A', 1000, 5)",
            [wav.to_string_lossy()],
        )
        .unwrap();
        assert_eq!(situacao(&conn).unwrap(), Situacao { analisadas: 0, pendentes: 1 });
        let db = std::sync::Mutex::new(conn);
        processar(&db, 1, &wav.to_string_lossy(), Some(5));
        let conn = db.lock().unwrap();
        assert_eq!(situacao(&conn).unwrap(), Situacao { analisadas: 1, pendentes: 0 });
        let g = do_banco(&conn, 1);
        assert!((g.faixa.unwrap().db - v.db).abs() < 1e-9);
        assert!((g.album.unwrap().db - v.db).abs() < 1e-9); // álbum de uma faixa só
        // Arquivo alterado (mtime novo): volta a ficar pendente.
        conn.execute("UPDATE media SET file_mtime = 6 WHERE id = 1", []).unwrap();
        assert_eq!(situacao(&conn).unwrap().pendentes, 1);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
