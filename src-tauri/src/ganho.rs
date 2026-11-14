//! Normalização de volume (seção 8.6): ReplayGain lido das tags, por faixa ou por álbum.
//! Sem as tags, o volume fica como está.

use std::path::Path;

use lofty::prelude::*;
use lofty::tag::ItemKey;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Modo {
    Desligada,
    #[default]
    Faixa,
    Album,
}

impl Modo {
    pub fn de_texto(s: &str) -> Option<Self> {
        match s {
            "desligada" => Some(Self::Desligada),
            "faixa" => Some(Self::Faixa),
            "album" => Some(Self::Album),
            _ => None,
        }
    }

    pub fn texto(self) -> &'static str {
        match self {
            Self::Desligada => "desligada",
            Self::Faixa => "faixa",
            Self::Album => "album",
        }
    }
}

/// Ganho em dB e pico (amplitude linear, 1.0 = máximo) de uma faixa ou álbum.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Valor {
    pub db: f64,
    pub pico: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ganho {
    pub faixa: Option<Valor>,
    pub album: Option<Valor>,
}

/// Fator a multiplicar no volume. O pico limita o ganho para não clipar; sem pico
/// conhecido, o ganho nunca aumenta o volume.
pub fn fator(modo: Modo, g: &Ganho) -> f64 {
    let valor = match modo {
        Modo::Desligada => None,
        Modo::Faixa => g.faixa.or(g.album),
        Modo::Album => g.album.or(g.faixa),
    };
    let Some(v) = valor else { return 1.0 };
    let f = 10f64.powf(v.db / 20.0);
    if f <= 1.0 {
        return f; // reduzir nunca clipa
    }
    match v.pico {
        Some(p) if p > 0.0 => f.min((1.0 / p).max(1.0)),
        _ => 1.0,
    }
}

/// "-7.89 dB", "+1.2", "0.98" → número.
fn numero(s: &str) -> Option<f64> {
    let s = s.trim();
    let fim = s.find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | ','))).unwrap_or(s.len());
    s[..fim].replace(',', ".").parse().ok()
}

/// Lê as tags de ReplayGain (ou R128, do Opus) do arquivo.
pub fn ler(path: &Path) -> Ganho {
    let Ok(arquivo) = lofty::read_from_path(path) else { return Ganho::default() };
    let mut g = Ganho::default();
    for tag in arquivo.tags() {
        let texto = |k: ItemKey| tag.get_string(k).and_then(numero);
        // R128 é inteiro Q7.8 relativo a -23 LUFS; o ReplayGain usa -18 LUFS (+5 dB).
        let r128 = |k: ItemKey| tag.get_string(k).and_then(numero).map(|v| v / 256.0 + 5.0);
        if g.faixa.is_none() {
            let db = texto(ItemKey::ReplayGainTrackGain).or_else(|| r128(ItemKey::R128TrackGain));
            g.faixa = db.map(|db| Valor { db, pico: texto(ItemKey::ReplayGainTrackPeak) });
        }
        if g.album.is_none() {
            let db = texto(ItemKey::ReplayGainAlbumGain).or_else(|| r128(ItemKey::R128AlbumGain));
            g.album = db.map(|db| Valor { db, pico: texto(ItemKey::ReplayGainAlbumPeak) });
        }
    }
    g
}

#[cfg(test)]
mod testes {
    use super::*;

    fn v(db: f64, pico: Option<f64>) -> Option<Valor> {
        Some(Valor { db, pico })
    }

    #[test]
    fn calcula_fator() {
        let g = Ganho { faixa: v(-6.0, Some(1.0)), album: v(-3.0, Some(0.9)) };
        assert_eq!(fator(Modo::Desligada, &g), 1.0);
        assert!((fator(Modo::Faixa, &g) - 0.501).abs() < 0.001);
        assert!((fator(Modo::Album, &g) - 0.708).abs() < 0.001);
        // Álbum sem tag usa a da faixa, e vice-versa.
        let so_faixa = Ganho { faixa: v(-6.0, None), album: None };
        assert_eq!(fator(Modo::Album, &so_faixa), fator(Modo::Faixa, &so_faixa));
        // Aumentar: limitado pelo pico; sem pico, não aumenta.
        assert!((fator(Modo::Faixa, &Ganho { faixa: v(6.0, Some(0.8)), album: None }) - 1.25).abs() < 1e-9);
        assert_eq!(fator(Modo::Faixa, &Ganho { faixa: v(6.0, None), album: None }), 1.0);
        assert_eq!(fator(Modo::Faixa, &Ganho::default()), 1.0);
    }

    #[test]
    fn le_numeros_das_tags() {
        assert_eq!(numero("-7.89 dB"), Some(-7.89));
        assert_eq!(numero("+1,5 dB"), Some(1.5));
        assert_eq!(numero("0.988525"), Some(0.988525));
        assert_eq!(numero("-1234"), Some(-1234.0));
        assert_eq!(numero("dB"), None);
    }

    /// FLAC com tags gerado pelo GStreamer; lido de volta pelo `lofty`.
    #[test]
    fn le_tags_de_um_flac() {
        let dir = std::env::temp_dir().join(format!("skald-teste-rg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let arquivo = dir.join("rg.flac");
        let ok = std::process::Command::new("gst-launch-1.0")
            .args(["-q", "audiotestsrc", "num-buffers=10", "!", "taginject"])
            .arg("tags=replaygain-track-gain=(double)-6.5,replaygain-track-peak=(double)0.9,replaygain-album-gain=(double)-4.0")
            .args(["!", "flacenc", "!", "filesink"])
            .arg(format!("location={}", arquivo.display()))
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            eprintln!("gst-launch-1.0 indisponível; teste ignorado");
            return;
        }
        let g = ler(&arquivo);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(g.faixa.map(|v| v.db), Some(-6.5));
        assert_eq!(g.faixa.and_then(|v| v.pico), Some(0.9));
        assert_eq!(g.album.map(|v| v.db), Some(-4.0));
    }
}
