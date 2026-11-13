use std::path::Path;
use std::time::UNIX_EPOCH;

use lofty::prelude::*;
use lofty::tag::ItemKey;
use rusqlite::{Connection, params};
use serde::Serialize;
use walkdir::WalkDir;

const EXTENSOES: &[&str] = &["mp3", "flac", "ogg", "opus", "oga", "m4a", "aac", "wav"];

#[derive(Serialize, Default)]
pub struct ImportResumo {
    pub encontrados: u32,
    pub adicionados: u32,
    pub existentes: u32,
    pub falhas: Vec<String>,
}

struct Faixa {
    path: String,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    album_artist: Option<String>,
    genre: Option<String>,
    year: Option<u32>,
    track_number: Option<u32>,
    disc_number: Option<u32>,
    duration_ms: i64,
    file_size: i64,
    file_mtime: Option<i64>,
}

fn eh_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSOES.contains(&e.to_ascii_lowercase().as_str()))
}

fn ler_faixa(path: &Path) -> Result<Faixa, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let arquivo = lofty::read_from_path(path).map_err(|e| e.to_string())?;
    let tag = arquivo.primary_tag().or_else(|| arquivo.first_tag());
    let texto = |s: Option<std::borrow::Cow<'_, str>>| {
        s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
    };

    // Sem título nas tags: usar o nome do arquivo (seção 6, informações ausentes).
    let title = texto(tag.and_then(|t| t.title())).or_else(|| {
        path.file_stem().map(|s| s.to_string_lossy().into_owned())
    });

    Ok(Faixa {
        path: path.to_string_lossy().into_owned(),
        title,
        artist: texto(tag.and_then(|t| t.artist())),
        album: texto(tag.and_then(|t| t.album())),
        album_artist: tag
            .and_then(|t| t.get_string(ItemKey::AlbumArtist))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        genre: texto(tag.and_then(|t| t.genre())),
        year: tag.and_then(|t| t.date()).map(|d| d.year as u32),
        track_number: tag.and_then(|t| t.track()),
        disc_number: tag.and_then(|t| t.disk()),
        duration_ms: arquivo.properties().duration().as_millis() as i64,
        file_size: meta.len() as i64,
        file_mtime: meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64),
    })
}

/// Lê a pasta (recursivamente) sem tocar no banco.
fn ler_pasta(raiz: &Path, resumo: &mut ImportResumo) -> Vec<Faixa> {
    let mut faixas = Vec::new();
    for entrada in WalkDir::new(raiz).follow_links(true).into_iter().filter_map(Result::ok) {
        let path = entrada.path();
        if !entrada.file_type().is_file() || !eh_audio(path) {
            continue;
        }
        resumo.encontrados += 1;
        match ler_faixa(path) {
            Ok(f) => faixas.push(f),
            Err(e) => resumo.falhas.push(format!("{}: {e}", path.display())),
        }
    }
    faixas
}

/// Grava as faixas. Caminhos já presentes na biblioteca são mantidos como estão,
/// para nunca sobrescrever metadados silenciosamente.
fn gravar(conn: &mut Connection, faixas: &[Faixa], resumo: &mut ImportResumo) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO media
             (path, title, artist, album, album_artist, genre, year,
              track_number, disc_number, duration_ms, file_size, file_mtime)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;
        for f in faixas {
            let n = stmt.execute(params![
                f.path, f.title, f.artist, f.album, f.album_artist, f.genre, f.year,
                f.track_number, f.disc_number, f.duration_ms, f.file_size, f.file_mtime
            ])?;
            if n == 1 {
                resumo.adicionados += 1;
            } else {
                resumo.existentes += 1;
            }
        }
    }
    tx.commit()
}

pub fn importar(conn_db: &std::sync::Mutex<Connection>, raiz: &Path) -> Result<ImportResumo, String> {
    if !raiz.is_dir() {
        return Err("A pasta selecionada não existe.".into());
    }
    let mut resumo = ImportResumo::default();
    // A leitura dos arquivos acontece antes de travar o banco.
    let faixas = ler_pasta(raiz, &mut resumo);
    let mut conn = conn_db.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
    gravar(&mut conn, &faixas, &mut resumo).map_err(|e| e.to_string())?;
    Ok(resumo)
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Rodar com: SKALD_TEST_DIR=/pasta cargo test -- --nocapture
    #[test]
    fn importa_pasta_de_teste() {
        let Ok(dir) = std::env::var("SKALD_TEST_DIR") else { return };
        let db = std::sync::Mutex::new(crate::db::open(Path::new(":memory:")).unwrap());
        let r = importar(&db, Path::new(&dir)).unwrap();
        println!("encontrados={} adicionados={} falhas={:?}", r.encontrados, r.adicionados, r.falhas);
        let conn = db.lock().unwrap();
        let mut st = conn.prepare("SELECT title, artist, album, album_artist, year, track_number, duration_ms FROM media ORDER BY path").unwrap();
        let linhas = st.query_map([], |r| Ok(format!("{:?} {:?} {:?} {:?} {:?} {:?} {:?}",
            r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?, r.get::<_, Option<i64>>(4)?, r.get::<_, Option<i64>>(5)?, r.get::<_, i64>(6)?))).unwrap();
        for l in linhas { println!("{}", l.unwrap()); }
        drop(st); drop(conn);
        let r2 = importar(&db, Path::new(&dir)).unwrap();
        assert_eq!(r2.adicionados, 0);
        assert_eq!(r2.existentes, r.adicionados);
    }
}
