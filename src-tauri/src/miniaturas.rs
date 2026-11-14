//! Miniaturas de capa (grades de álbuns e artistas). Ficam em cache no disco, na pasta de
//! cache do app; os arquivos de música e as imagens originais nunca são alterados.

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rusqlite::Connection;

/// Lado da miniatura em pixels (o dobro do exibido, para telas de alta densidade).
const LADO: u32 = 256;

/// Data URL JPEG da miniatura da capa da música, ou `None` se ela não tem capa.
pub fn obter(conn_db: &std::sync::Mutex<Connection>, pasta_cache: &Path, id: i64) -> Result<Option<String>, String> {
    Ok(arquivo(conn_db, pasta_cache, id)?.and_then(|jpg| std::fs::read(jpg).ok()).map(|b| data_url(&b)))
}

/// Caminho do JPEG da miniatura em cache (gerado se preciso), ou `None` se não há capa.
/// A chave do cache inclui o `mtime` do arquivo: capa trocada gera miniatura nova.
pub fn arquivo(conn_db: &std::sync::Mutex<Connection>, pasta_cache: &Path, id: i64) -> Result<Option<PathBuf>, String> {
    let (path, mtime): (String, Option<i64>) = {
        let conn = conn_db.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
        match conn.query_row("SELECT path, file_mtime FROM media WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))) {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        }
    };
    let base = pasta_cache.join(format!("{id}-{}", mtime.unwrap_or(0)));
    let jpg = base.with_extension("jpg");
    let sem = base.with_extension("sem");
    if jpg.is_file() {
        return Ok(Some(jpg));
    }
    if sem.exists() {
        return Ok(None);
    }
    let _ = std::fs::create_dir_all(pasta_cache);
    match gerar(Path::new(&path)) {
        Some(bytes) => {
            std::fs::write(&jpg, &bytes).map_err(|e| e.to_string())?;
            Ok(Some(jpg))
        }
        None => {
            // Arquivo presente sem capa: lembra para não reler. Ausente: tenta de novo depois.
            if Path::new(&path).is_file() {
                let _ = std::fs::write(&sem, b"");
            }
            Ok(None)
        }
    }
}

fn data_url(jpg: &[u8]) -> String {
    format!("data:image/jpeg;base64,{}", STANDARD.encode(jpg))
}

fn gerar(musica: &Path) -> Option<Vec<u8>> {
    let (_, bytes) = crate::details::capa_bruta(musica)?;
    let img = image::load_from_memory(&bytes).ok()?;
    let mini = img.thumbnail(LADO, LADO).to_rgb8();
    let mut saida = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut saida, 85).encode_image(&mini).ok()?;
    Some(saida)
}

pub fn pasta(cache_dir: PathBuf) -> PathBuf {
    cache_dir.join("miniaturas")
}

#[cfg(test)]
mod testes {
    /// Rodar com: SKALD_DB_ENSAIO=/copia.db cargo test ensaio_miniaturas -- --nocapture
    #[test]
    fn ensaio_miniaturas() {
        let Ok(caminho) = std::env::var("SKALD_DB_ENSAIO") else { return };
        let db = std::sync::Mutex::new(crate::db::open(std::path::Path::new(&caminho)).unwrap());
        let pasta = std::env::temp_dir().join(format!("skald-mini-{}", std::process::id()));
        let ids: Vec<i64> = db.lock().unwrap().prepare("SELECT id FROM media LIMIT 20").unwrap()
            .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        let t = std::time::Instant::now();
        let com = ids.iter().filter(|&&id| super::obter(&db, &pasta, id).unwrap().is_some()).count();
        println!("{com}/{} com capa em {:?}", ids.len(), t.elapsed());
        let t = std::time::Instant::now();
        let url = super::obter(&db, &pasta, ids[0]).unwrap();
        println!("do cache em {:?}, {} bytes", t.elapsed(), url.map_or(0, |u| u.len()));
        let _ = std::fs::remove_dir_all(&pasta);
    }
}
