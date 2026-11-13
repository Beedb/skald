use rusqlite::{Connection, OptionalExtension, params_from_iter};

use crate::search;
use serde::Serialize;

#[derive(Serialize, Debug, Clone)]
pub struct MediaRow {
    pub id: i64,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub missing: bool,
}

/// Ordem da biblioteca: artista do álbum, álbum, disco, faixa, título; campos vazios no fim.
const ORDEM: &str = "ORDER BY COALESCE(album_artist, artist) IS NULL, COALESCE(album_artist, artist) COLLATE NOCASE,
                  album IS NULL, album COLLATE NOCASE, disc_number, track_number, title COLLATE NOCASE, id";

/// Todos os ids de músicas (filtrados pela busca) na ordem da biblioteca: contexto da fila.
pub fn ids_em_ordem(conn: &Connection, busca: &str) -> rusqlite::Result<Vec<i64>> {
    let (cond, termos) = search::filtro(busca);
    let mut stmt = conn.prepare_cached(&format!("SELECT id FROM media WHERE category = 'music' AND {cond} {ORDEM}"))?;
    let ids = stmt.query_map(params_from_iter(termos), |r| r.get(0))?;
    ids.collect()
}

/// Quantas músicas a busca encontra (busca vazia: a biblioteca toda).
pub fn contar(conn: &Connection, busca: &str) -> rusqlite::Result<i64> {
    let (cond, termos) = search::filtro(busca);
    conn.prepare_cached(&format!("SELECT COUNT(*) FROM media WHERE category = 'music' AND {cond}"))?
        .query_row(params_from_iter(termos), |r| r.get(0))
}

/// Várias músicas por id, na ordem pedida (ids inexistentes são ignorados).
pub fn obter_varios(conn: &Connection, ids: &[i64]) -> rusqlite::Result<Vec<MediaRow>> {
    let mut stmt = conn.prepare_cached("SELECT id, title, artist, album, duration_ms, missing FROM media WHERE id = ?1")?;
    let mut linhas = Vec::with_capacity(ids.len());
    for &id in ids {
        if let Some(m) = stmt.query_row([id], linha).optional()? {
            linhas.push(m);
        }
    }
    Ok(linhas)
}

/// Uma página da biblioteca (só músicas), em ordem estável para a lista virtualizada.
/// Campos vazios vão para o fim de cada grupo.
pub fn list_media(conn: &Connection, offset: u32, limit: u32, busca: &str) -> rusqlite::Result<Vec<MediaRow>> {
    let (cond, termos) = search::filtro(busca);
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT id, title, artist, album, duration_ms, missing
         FROM media WHERE category = 'music' AND {cond} {ORDEM} LIMIT ? OFFSET ?"
    ))?;
    use rusqlite::types::Value;
    let valores = termos
        .into_iter()
        .map(Value::Text)
        .chain([Value::Integer(limit.into()), Value::Integer(offset.into())]);
    let rows = stmt.query_map(params_from_iter(valores), linha)?;
    rows.collect()
}

pub(crate) fn linha(r: &rusqlite::Row) -> rusqlite::Result<MediaRow> {
    Ok(MediaRow {
        id: r.get(0)?,
        title: r.get(1)?,
        artist: r.get(2)?,
        album: r.get(3)?,
        duration_ms: r.get(4)?,
        missing: r.get(5)?,
    })
}

/// Música e caminho no disco, ou `None` se o id não existe.
pub fn obter(conn: &Connection, id: i64) -> rusqlite::Result<Option<(MediaRow, String)>> {
    conn.query_row(
        "SELECT id, title, artist, album, duration_ms, missing, path FROM media WHERE id = ?1",
        [id],
        |r| Ok((linha(r)?, r.get::<_, String>(6)?)),
    )
    .optional()
}

/// Marca (ou desmarca) o arquivo como não encontrado. O registro nunca é apagado.
pub fn marcar_ausente(conn: &Connection, id: i64, ausente: bool) -> rusqlite::Result<()> {
    conn.execute("UPDATE media SET missing = ?2 WHERE id = ?1", rusqlite::params![id, ausente])?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn pagina_e_ordena() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        conn.execute_batch(
            "INSERT INTO media (path, title, artist, album, track_number) VALUES
             ('/c', 'Sem artista', NULL, NULL, NULL),
             ('/b', 'Faixa 2', 'anirap', 'X', 2),
             ('/a', 'Faixa 1', 'Anirap', 'X', 1),
             ('/d', 'Outra', 'Banda', 'Y', 1);",
        )
        .unwrap();
        let mut conn = conn;
        crate::search::preencher_pendentes(&mut conn).unwrap();
        let achar = |busca: &str| -> Vec<String> {
            list_media(&conn, 0, 10, busca).unwrap().into_iter().map(|m| m.title.unwrap()).collect()
        };
        let todas: Vec<_> = list_media(&conn, 0, 10, "").unwrap().into_iter().map(|m| m.title.unwrap()).collect();
        assert_eq!(todas, ["Faixa 1", "Faixa 2", "Outra", "Sem artista"]);
        let pag: Vec<_> = list_media(&conn, 1, 2, "").unwrap().into_iter().map(|m| m.title.unwrap()).collect();
        assert_eq!(pag, ["Faixa 2", "Outra"]);
        let ids = ids_em_ordem(&conn, "").unwrap();
        let titulos: Vec<_> = obter_varios(&conn, &ids).unwrap().into_iter().map(|m| m.title.unwrap()).collect();
        assert_eq!(titulos, todas);

        // Busca: trecho, sem acento/caixa, várias palavras em campos diferentes.
        assert_eq!(achar("ani"), ["Faixa 1", "Faixa 2"]);
        assert_eq!(achar("BANDA"), ["Outra"]);
        assert_eq!(achar("anirap 2"), ["Faixa 2"]);
        assert_eq!(achar("sem artista"), ["Sem artista"]);
        assert!(achar("inexistente").is_empty());
        assert_eq!(contar(&conn, "ani").unwrap(), 2);
        assert_eq!(contar(&conn, "").unwrap(), 4);
        assert_eq!(ids_em_ordem(&conn, "banda").unwrap().len(), 1);
    }
}
