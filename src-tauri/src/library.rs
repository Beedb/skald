use rusqlite::{Connection, params};
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct MediaRow {
    pub id: i64,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub missing: bool,
}

/// Uma página da biblioteca (só músicas), em ordem estável para a lista virtualizada.
/// Campos vazios vão para o fim de cada grupo.
pub fn list_media(conn: &Connection, offset: u32, limit: u32) -> rusqlite::Result<Vec<MediaRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, title, artist, album, duration_ms, missing
         FROM media
         WHERE category = 'music'
         ORDER BY COALESCE(album_artist, artist) IS NULL, COALESCE(album_artist, artist) COLLATE NOCASE,
                  album IS NULL, album COLLATE NOCASE,
                  disc_number, track_number,
                  title COLLATE NOCASE, id
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], linha)?;
    rows.collect()
}

fn linha(r: &rusqlite::Row) -> rusqlite::Result<MediaRow> {
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
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT id, title, artist, album, duration_ms, missing, path FROM media WHERE id = ?1",
        [id],
        |r| Ok((linha(r)?, r.get::<_, String>(6)?)),
    )
    .optional()
}

/// Marca (ou desmarca) o arquivo como não encontrado. O registro nunca é apagado.
pub fn marcar_ausente(conn: &Connection, id: i64, ausente: bool) -> rusqlite::Result<()> {
    conn.execute("UPDATE media SET missing = ?2 WHERE id = ?1", params![id, ausente])?;
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
        let todas: Vec<_> = list_media(&conn, 0, 10).unwrap().into_iter().map(|m| m.title.unwrap()).collect();
        assert_eq!(todas, ["Faixa 1", "Faixa 2", "Outra", "Sem artista"]);
        let pag: Vec<_> = list_media(&conn, 1, 2).unwrap().into_iter().map(|m| m.title.unwrap()).collect();
        assert_eq!(pag, ["Faixa 2", "Outra"]);
    }
}
