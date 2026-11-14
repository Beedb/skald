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
/// Faixas consecutivas do mesmo álbum (para não usar crossfade entre elas): mesmo nome de
/// álbum, sem diferenciar acentos/maiúsculas, e mesmo artista do álbum ou mesma pasta.
pub fn mesmo_album(conn: &Connection, a: i64, b: i64) -> rusqlite::Result<bool> {
    let dados = |id: i64| {
        conn.query_row("SELECT album, album_artist, path FROM media WHERE id = ?1", [id], |r| {
            Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?))
        })
    };
    let (album_a, artista_a, path_a) = dados(a)?;
    let (album_b, artista_b, path_b) = dados(b)?;
    let norm = |s: &str| crate::search::normalizar(s.trim());
    let (Some(album_a), Some(album_b)) = (album_a, album_b) else { return Ok(false) };
    if norm(&album_a) != norm(&album_b) {
        return Ok(false);
    }
    let mesma_pasta = std::path::Path::new(&path_a).parent() == std::path::Path::new(&path_b).parent();
    let mesmo_artista = matches!((artista_a, artista_b), (Some(x), Some(y)) if norm(&x) == norm(&y));
    Ok(mesma_pasta || mesmo_artista)
}

pub fn marcar_ausente(conn: &Connection, id: i64, ausente: bool) -> rusqlite::Result<()> {
    conn.execute("UPDATE media SET missing = ?2 WHERE id = ?1", rusqlite::params![id, ausente])?;
    Ok(())
}

/// Exclui da biblioteca: apaga só o registro, nunca o arquivo (seção 5). O histórico fica
/// (com a cópia dos dados e `media_id` nulo); os itens de playlist saem junto. O caminho é
/// lembrado para a importação e o monitoramento não o trazerem de volta.
pub fn excluir(conn: &mut Connection, ids: &[i64]) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;
    let mut n = 0;
    {
        let mut lembrar = tx.prepare(
            "INSERT OR REPLACE INTO excluded_path (path) SELECT path FROM media WHERE id = ?1",
        )?;
        let mut apagar = tx.prepare("DELETE FROM media WHERE id = ?1")?;
        for &id in ids {
            lembrar.execute([id])?;
            n += apagar.execute([id])?;
        }
    }
    tx.commit()?;
    Ok(n)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn excluir_mantem_historico_e_lembra_o_caminho() {
        let mut conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        conn.execute_batch(
            "INSERT INTO media (id, path, title) VALUES (1, '/a.mp3', 'A'), (2, '/b.mp3', 'B');
             INSERT INTO playlist (id, name) VALUES (9, 'P');
             INSERT INTO playlist_item (playlist_id, media_id, position) VALUES (9, 1, 0), (9, 2, 1);
             INSERT INTO play_history (media_id, title, started_at) VALUES (1, 'A', 0);",
        )
        .unwrap();
        assert_eq!(excluir(&mut conn, &[1]).unwrap(), 1);
        let conta = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(conta("SELECT COUNT(*) FROM media"), 1);
        assert_eq!(conta("SELECT COUNT(*) FROM playlist_item"), 1);
        assert_eq!(conta("SELECT COUNT(*) FROM play_history WHERE media_id IS NULL AND title = 'A'"), 1);
        assert_eq!(conta("SELECT COUNT(*) FROM excluded_path WHERE path = '/a.mp3'"), 1);
    }

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

    #[test]
    fn regra_do_mesmo_album() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        for (id, album, artista, path) in [
            (1, Some("Clube da Esquina"), Some("Milton"), "/m/clube/01.flac"),
            (2, Some("CLUBE DA ESQUINA"), None, "/m/clube/02.flac"),     // mesma pasta
            (3, Some("Clube da Esquina"), Some("milton"), "/outra/03.flac"), // mesmo artista do álbum
            (4, Some("Clube da Esquina"), Some("Lô"), "/outra/04.flac"),    // homônimo em outro lugar
            (5, None, Some("Milton"), "/m/clube/05.flac"),                 // sem álbum
        ] {
            conn.execute(
                "INSERT INTO media (id, path, album, album_artist) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, path, album, artista],
            )
            .unwrap();
        }
        assert!(mesmo_album(&conn, 1, 2).unwrap());
        assert!(mesmo_album(&conn, 1, 3).unwrap());
        assert!(!mesmo_album(&conn, 1, 4).unwrap());
        assert!(!mesmo_album(&conn, 1, 5).unwrap());
    }
}
