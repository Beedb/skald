//! Máquina do Tempo (seção 11.4): os mais escutados e a evolução em um período, sobre todo
//! o histórico (inclusive músicas que já saíram da biblioteca).
//!
//! Período: "" (sempre), "AAAA" (ano) ou "AAAA-MM" (mês), em horário local.
//! Rankings contam reproduções escutadas (`counted`); o tempo soma todas as reproduções.

use rusqlite::{Connection, params};
use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
pub struct Item {
    pub nome: String,
    /// Artista (de músicas e álbuns).
    pub detalhe: Option<String>,
    /// Música ainda na biblioteca (para tocar).
    pub media_id: Option<i64>,
    pub escutadas: i64,
    pub ms: i64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Ponto {
    /// "AAAA-MM" ou "AAAA-MM-DD".
    pub periodo: String,
    pub ms: i64,
    pub escutadas: i64,
}

#[derive(Serialize, Debug)]
pub struct Painel {
    pub periodo: String,
    pub tempo_ms: i64,
    pub escutadas: i64,
    pub artistas: Vec<Item>,
    pub musicas: Vec<Item>,
    pub albuns: Vec<Item>,
    pub generos: Vec<Item>,
    pub evolucao: Vec<Ponto>,
}

/// Filtro comum: música e dentro do período (`?1`).
const NO_PERIODO: &str = "h.category = 'music'
    AND (?1 = '' OR strftime(CASE length(?1) WHEN 4 THEN '%Y' ELSE '%Y-%m' END,
                             h.started_at, 'unixepoch', 'localtime') = ?1)";

fn periodo_valido(p: &str) -> bool {
    let b = p.as_bytes();
    let digitos = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    match b.len() {
        0 => true,
        4 => digitos(0..4),
        7 => digitos(0..4) && b[4] == b'-' && digitos(5..7),
        _ => false,
    }
}

/// Ranking agrupado por `chave` (expressão SQL), mostrando `nome`/`detalhe`.
fn ranking(conn: &Connection, periodo: &str, nome: &str, detalhe: &str, chave: &str, limite: u32) -> rusqlite::Result<Vec<Item>> {
    let sql = format!(
        "SELECT {nome}, {detalhe}, MAX(m.id), SUM(h.counted), SUM(h.listened_ms)
         FROM play_history h LEFT JOIN media m ON m.id = h.media_id
         WHERE {NO_PERIODO} AND {nome} IS NOT NULL AND TRIM({nome}) <> ''
         GROUP BY {chave}
         HAVING SUM(h.counted) > 0
         ORDER BY SUM(h.counted) DESC, SUM(h.listened_ms) DESC
         LIMIT ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let itens = stmt.query_map(params![periodo, limite], |r| {
        Ok(Item {
            nome: r.get::<_, String>(0)?.trim().to_string(),
            detalhe: r.get(1)?,
            media_id: r.get(2)?,
            escutadas: r.get(3)?,
            ms: r.get(4)?,
        })
    })?;
    itens.collect()
}

pub fn painel(conn: &Connection, periodo: &str) -> Result<Painel, String> {
    if !periodo_valido(periodo) {
        return Err("Período inválido.".into());
    }
    let erro = |e: rusqlite::Error| e.to_string();
    let (tempo_ms, escutadas) = conn
        .query_row(
            &format!("SELECT COALESCE(SUM(h.listened_ms), 0), COALESCE(SUM(h.counted), 0) FROM play_history h WHERE {NO_PERIODO}"),
            [periodo],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(erro)?;
    let artista_album = "COALESCE(m.album_artist, h.artist)";
    let artistas = ranking(conn, periodo, "h.artist", "NULL", "TRIM(h.artist) COLLATE SEMACENTO", 10).map_err(erro)?;
    let musicas = ranking(
        conn, periodo, "h.title", "MAX(h.artist)",
        "TRIM(h.title) COLLATE SEMACENTO, TRIM(COALESCE(h.artist, '')) COLLATE SEMACENTO", 10,
    )
    .map_err(erro)?;
    let albuns = ranking(
        conn, periodo, "h.album", &format!("MAX({artista_album})"),
        &format!("TRIM(h.album) COLLATE SEMACENTO, TRIM(COALESCE({artista_album}, '')) COLLATE SEMACENTO"), 10,
    )
    .map_err(erro)?;
    let generos = ranking(conn, periodo, "h.genre", "NULL", "TRIM(h.genre) COLLATE SEMACENTO", 5).map_err(erro)?;

    // Sempre ou ano: por mês; mês: por dia.
    let formato = if periodo.len() == 7 { "%Y-%m-%d" } else { "%Y-%m" };
    let mut stmt = conn
        .prepare(&format!(
            "SELECT strftime('{formato}', h.started_at, 'unixepoch', 'localtime') AS p,
                    SUM(h.listened_ms), SUM(h.counted)
             FROM play_history h WHERE {NO_PERIODO} GROUP BY p ORDER BY p"
        ))
        .map_err(erro)?;
    let evolucao = stmt
        .query_map([periodo], |r| Ok(Ponto { periodo: r.get(0)?, ms: r.get(1)?, escutadas: r.get(2)? }))
        .map_err(erro)?
        .collect::<rusqlite::Result<_>>()
        .map_err(erro)?;

    Ok(Painel { periodo: periodo.to_string(), tempo_ms, escutadas, artistas, musicas, albuns, generos, evolucao })
}

/// Meses com alguma reprodução, do mais recente ao mais antigo ("AAAA-MM").
pub fn meses(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT strftime('%Y-%m', started_at, 'unixepoch', 'localtime') AS p
         FROM play_history WHERE category = 'music' ORDER BY p DESC",
    )?;
    let meses = stmt.query_map([], |r| r.get(0))?;
    meses.collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Horário local → unix, pelo próprio SQLite (o teste não depende do fuso).
    fn unix(conn: &Connection, data: &str) -> i64 {
        conn.query_row("SELECT strftime('%s', ?1, 'utc')", [data], |r| r.get::<_, String>(0))
            .unwrap()
            .parse()
            .unwrap()
    }

    fn tocar(conn: &Connection, data: &str, titulo: &str, artista: &str, album: &str, genero: &str, ms: i64, contou: bool) {
        let t = unix(conn, data);
        conn.execute(
            "INSERT INTO play_history (title, artist, album, genre, started_at, listened_ms, counted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![titulo, artista, album, genero, t, ms, contou],
        )
        .unwrap();
    }

    #[test]
    fn rankings_e_evolucao_por_periodo() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        tocar(&conn, "2026-10-05 10:00:00", "Oceano", "Djavan", "Djavan", "MPB", 200_000, true);
        tocar(&conn, "2026-10-06 10:00:00", "OCEANO", "djavan", "Djavan", "mpb", 200_000, true);
        tocar(&conn, "2026-10-06 11:00:00", "Flor de Lis", "Djavan", "A Voz", "MPB", 180_000, true);
        tocar(&conn, "2026-10-07 23:30:00", "Trevo", "Anavitória", "Anavitória", "Pop", 30_000, false);
        tocar(&conn, "2026-11-01 00:10:00", "Trevo", "Anavitória", "Anavitória", "Pop", 190_000, true);
        tocar(&conn, "2025-12-31 23:59:00", "Velha", "Antigo", "Disco", "Rock", 100_000, true);

        let out = painel(&conn, "2026-10").unwrap();
        assert_eq!((out.tempo_ms, out.escutadas), (610_000, 3));
        assert_eq!(out.artistas.len(), 1); // Anavitória não foi escutada em outubro
        assert_eq!((out.artistas[0].escutadas, out.artistas[0].ms), (3, 580_000));
        assert_eq!(out.musicas[0].escutadas, 2); // "Oceano" e "OCEANO" juntas
        assert_eq!(out.musicas[0].detalhe.as_deref().map(str::to_lowercase).as_deref(), Some("djavan"));
        assert_eq!(out.albuns.iter().map(|a| a.nome.as_str()).collect::<Vec<_>>(), ["Djavan", "A Voz"]);
        assert_eq!(out.generos.len(), 1);
        assert_eq!(out.evolucao.iter().map(|p| p.periodo.as_str()).collect::<Vec<_>>(), ["2026-10-05", "2026-10-06", "2026-10-07"]);

        let ano = painel(&conn, "2026").unwrap();
        assert_eq!(ano.escutadas, 4);
        assert_eq!(ano.evolucao.iter().map(|p| p.periodo.as_str()).collect::<Vec<_>>(), ["2026-10", "2026-11"]);

        let sempre = painel(&conn, "").unwrap();
        assert_eq!(sempre.escutadas, 5);
        assert_eq!(sempre.artistas[0].nome, "Djavan");

        assert_eq!(meses(&conn).unwrap(), ["2026-11", "2026-10", "2025-12"]);
        assert!(painel(&conn, "2026-1").is_err());
        assert!(painel(&conn, "x'; DROP").is_err());
    }
}
