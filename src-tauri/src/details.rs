use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use lofty::picture::{MimeType, PictureType};
use lofty::prelude::*;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

/// Nomes de imagem de capa procurados na pasta da música (sem diferenciar maiúsculas).
const CAPAS_NA_PASTA: &[&str] = &["cover", "folder", "front", "album", "albumart"];
const EXTENSOES_IMAGEM: &[(&str, &str)] = &[("jpg", "image/jpeg"), ("jpeg", "image/jpeg"), ("png", "image/png"), ("webp", "image/webp")];
/// Capas maiores que isso são ignoradas para não pesar a interface.
const CAPA_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Serialize, Default, Debug)]
pub struct Detalhes {
    pub id: i64,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i64>,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
    pub path: String,
    pub file_size: Option<i64>,
    pub formato: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub canais: Option<u8>,
    /// Data URL (`data:image/...;base64,...`), se houver capa.
    pub capa: Option<String>,
}

/// Só os campos do banco; completar depois com `completar_do_arquivo`, fora da trava do banco.
pub fn do_banco(conn: &Connection, id: i64) -> Result<Option<Detalhes>, String> {
    conn
        .query_row(
            "SELECT album_artist, genre, year, track_number, disc_number, path, file_size FROM media WHERE id = ?1",
            [id],
            |r| {
                Ok(Detalhes {
                    id,
                    album_artist: r.get(0)?,
                    genre: r.get(1)?,
                    year: r.get(2)?,
                    track_number: r.get(3)?,
                    disc_number: r.get(4)?,
                    path: r.get(5)?,
                    file_size: r.get(6)?,
                    ..Default::default()
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
}

/// Lê do arquivo o que não está no banco. Arquivo ausente ou ilegível: devolve só o do banco.
pub fn completar_do_arquivo(mut d: Detalhes) -> Detalhes {
    let path = Path::new(&d.path);
    d.formato = path.extension().map(|e| e.to_string_lossy().to_uppercase());
    if let Ok(arquivo) = lofty::read_from_path(path) {
        let p = arquivo.properties();
        d.bitrate_kbps = p.audio_bitrate();
        d.sample_rate = p.sample_rate();
        d.bit_depth = p.bit_depth();
        d.canais = p.channels();
        d.capa = capa_embutida(&arquivo);
    }
    if d.capa.is_none() {
        d.capa = capa_da_pasta(path);
    }
    d
}

/// Bytes da capa (embutida ou da pasta) e o tipo MIME, sem converter para data URL.
pub fn capa_bruta(path: &Path) -> Option<(&'static str, Vec<u8>)> {
    if let Ok(arquivo) = lofty::read_from_path(path) {
        let imagens: Vec<_> = arquivo.tags().iter().flat_map(|t| t.pictures()).collect();
        let pic = imagens
            .iter()
            .find(|p| p.pic_type() == PictureType::CoverFront)
            .or_else(|| imagens.first());
        if let Some(pic) = pic {
            return Some((mime_de(pic.mime_type()), pic.data().to_vec()));
        }
    }
    let (caminho, mime) = arquivo_capa_da_pasta(path)?;
    Some((mime, std::fs::read(caminho).ok()?))
}

fn mime_de(m: Option<&MimeType>) -> &'static str {
    match m {
        Some(MimeType::Png) => "image/png",
        Some(MimeType::Gif) => "image/gif",
        Some(MimeType::Bmp) => "image/bmp",
        _ => "image/jpeg",
    }
}

fn data_url(mime: &str, bytes: &[u8]) -> Option<String> {
    (bytes.len() <= CAPA_MAX_BYTES).then(|| format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

/// Prefere a capa frontal; senão, a primeira imagem embutida.
fn capa_embutida(arquivo: &lofty::file::TaggedFile) -> Option<String> {
    let imagens: Vec<_> = arquivo.tags().iter().flat_map(|t| t.pictures()).collect();
    let pic = imagens
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| imagens.first())?;
    data_url(mime_de(pic.mime_type()), pic.data())
}

fn capa_da_pasta(musica: &Path) -> Option<String> {
    let (caminho, mime) = arquivo_capa_da_pasta(musica)?;
    data_url(mime, &std::fs::read(caminho).ok()?)
}

/// Imagem de capa na pasta da música (cover.jpg, folder.png...), com o tipo MIME.
fn arquivo_capa_da_pasta(musica: &Path) -> Option<(std::path::PathBuf, &'static str)> {
    let pasta = musica.parent()?;
    let mut entradas: Vec<_> = std::fs::read_dir(pasta).ok()?.filter_map(Result::ok).map(|e| e.path()).collect();
    entradas.sort();
    for nome in CAPAS_NA_PASTA {
        for caminho in &entradas {
            let (Some(stem), Some(ext)) = (caminho.file_stem(), caminho.extension()) else { continue };
            if !stem.to_string_lossy().eq_ignore_ascii_case(nome) {
                continue;
            }
            let ext = ext.to_string_lossy().to_ascii_lowercase();
            let Some((_, mime)) = EXTENSOES_IMAGEM.iter().find(|(e, _)| *e == ext) else { continue };
            return Some((caminho.clone(), mime));
        }
    }
    None
}

/// Imagem (jpg, png, webp) como data URL, se tiver extensão aceita e não for grande demais.
pub fn imagem_data_url(caminho: &Path) -> Option<String> {
    let mime = mime_imagem(caminho)?;
    data_url(mime, &std::fs::read(caminho).ok()?)
}

/// Tipo MIME pela extensão, só para os formatos de imagem aceitos.
pub fn mime_imagem(caminho: &Path) -> Option<&'static str> {
    let ext = caminho.extension()?.to_string_lossy().to_ascii_lowercase();
    EXTENSOES_IMAGEM.iter().find(|(e, _)| *e == ext).map(|(_, m)| *m)
}

pub const CAPA_MAX: usize = CAPA_MAX_BYTES;

#[cfg(test)]
mod testes {
    use super::*;

    /// Pasta de teste com `capa/embutida.mp3` (capa embutida) e `pasta/x.mp3` + `pasta/Cover.PNG`.
    /// Rodar com: SKALD_TEST_DIR=/pasta cargo test details -- --nocapture
    #[test]
    fn encontra_capas() {
        let Ok(dir) = std::env::var("SKALD_TEST_DIR") else { return };
        let embutida = completar_do_arquivo(Detalhes { path: format!("{dir}/capa/embutida.mp3"), ..Default::default() });
        let capa = embutida.capa.expect("capa embutida");
        assert!(capa.starts_with("data:image/png;base64,"), "{}", &capa[..40]);
        println!("embutida: {} kbps, {} Hz, {:?} canais, {:?}", embutida.bitrate_kbps.unwrap(), embutida.sample_rate.unwrap(), embutida.canais, embutida.formato);

        let da_pasta = completar_do_arquivo(Detalhes { path: format!("{dir}/pasta/x.mp3"), ..Default::default() });
        assert!(da_pasta.capa.expect("capa da pasta").starts_with("data:image/png;base64,"));

        let sem = completar_do_arquivo(Detalhes { path: format!("{dir}/a.mp3"), ..Default::default() });
        assert!(sem.capa.is_none());

        let ausente = completar_do_arquivo(Detalhes { path: format!("{dir}/nao-existe.mp3"), ..Default::default() });
        assert!(ausente.capa.is_none() && ausente.bitrate_kbps.is_none());
        assert_eq!(ausente.formato.as_deref(), Some("MP3"));
    }
}
