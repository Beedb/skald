//! Máquina do Tempo (seção 11.4): os mais escutados e a evolução em um período, sobre todo
//! o histórico (inclusive músicas que já saíram da biblioteca).
//!
//! Período: "" (sempre), "AAAA" (ano) ou "AAAA-MM" (mês), em horário local.
//! Rankings contam reproduções escutadas (`counted`); o tempo soma todas as reproduções.

use rusqlite::{Connection, OptionalExtension, params};
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
    /// Capa (data URL), preenchida só nas retrospectivas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capa: Option<String>,
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

pub fn periodo_valido(p: &str) -> bool {
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
            capa: None,
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

/// Retrospectiva (seções 11.5 e 11.6) de um mês ("AAAA-MM") ou ano ("AAAA") encerrado.
#[derive(Serialize, Debug)]
pub struct Retrospectiva {
    pub periodo: String,
    pub tempo_ms: i64,
    pub escutadas: i64,
    pub artistas: Vec<Item>,
    pub musicas: Vec<Item>,
    pub albuns: Vec<Item>,
    pub genero: Option<Item>,
    /// A música escutada mais vezes num mesmo dia.
    pub mais_repetida: Option<Repetida>,
    /// O artista com a maior fatia do tempo ouvido.
    pub dominante: Option<Dominante>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Repetida {
    pub titulo: String,
    pub artista: Option<String>,
    pub vezes: i64,
    /// "AAAA-MM-DD".
    pub dia: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Dominante {
    pub nome: String,
    /// Porcentagem (0–100) do tempo ouvido no período.
    pub percentual: f64,
}

pub fn retrospectiva(conn: &Connection, periodo: &str) -> Result<Retrospectiva, String> {
    if periodo.is_empty() || !periodo_valido(periodo) {
        return Err("Período inválido.".into());
    }
    let erro = |e: rusqlite::Error| e.to_string();
    let p = painel(conn, periodo)?;
    let mais_repetida = conn
        .query_row(
            &format!(
                "SELECT MAX(h.title), MAX(h.artist), SUM(h.counted) AS vezes,
                        strftime('%Y-%m-%d', h.started_at, 'unixepoch', 'localtime') AS dia
                 FROM play_history h
                 WHERE {NO_PERIODO} AND h.title IS NOT NULL
                 GROUP BY dia, TRIM(h.title) COLLATE SEMACENTO, TRIM(COALESCE(h.artist, '')) COLLATE SEMACENTO
                 HAVING vezes > 0
                 ORDER BY vezes DESC, SUM(h.listened_ms) DESC, dia
                 LIMIT 1"
            ),
            [periodo],
            |r| Ok(Repetida { titulo: r.get(0)?, artista: r.get(1)?, vezes: r.get(2)?, dia: r.get(3)? }),
        )
        .optional()
        .map_err(erro)?;
    let dominante = conn
        .query_row(
            &format!(
                "SELECT MAX(h.artist), SUM(h.listened_ms)
                 FROM play_history h
                 WHERE {NO_PERIODO} AND h.artist IS NOT NULL AND TRIM(h.artist) <> ''
                 GROUP BY TRIM(h.artist) COLLATE SEMACENTO
                 ORDER BY SUM(h.listened_ms) DESC
                 LIMIT 1"
            ),
            [periodo],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(erro)?
        .filter(|_| p.tempo_ms > 0)
        .map(|(nome, ms)| Dominante { nome: nome.trim().to_string(), percentual: ms as f64 * 100.0 / p.tempo_ms as f64 });
    let corta = |mut v: Vec<Item>, n: usize| {
        v.truncate(n);
        v
    };
    Ok(Retrospectiva {
        periodo: p.periodo,
        tempo_ms: p.tempo_ms,
        escutadas: p.escutadas,
        artistas: corta(p.artistas, 5),
        musicas: corta(p.musicas, 10),
        albuns: corta(p.albuns, 5),
        genero: p.generos.into_iter().next(),
        mais_repetida,
        dominante,
    })
}

/// Retrospectivas já liberadas, da mais recente à mais antiga: meses antes de `mes_atual`
/// ("AAAA-MM") e anos antes do atual, que tenham alguma música escutada.
pub fn retrospectivas_disponiveis(conn: &Connection, mes_atual: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "WITH m AS (
            SELECT DISTINCT strftime('%Y-%m', started_at, 'unixepoch', 'localtime') AS p
            FROM play_history WHERE category = 'music' AND counted = 1
         )
         SELECT p FROM (
            SELECT p FROM m WHERE p < ?1
            UNION
            SELECT DISTINCT substr(p, 1, 4) FROM m WHERE substr(p, 1, 4) < substr(?1, 1, 4)
         )
         ORDER BY substr(p, 1, 4) DESC, length(p) ASC, p DESC",
    )?;
    let lista = stmt.query_map([mes_atual], |r| r.get(0))?;
    lista.collect()
}

/// Grava o PNG da retrospectiva em `base/AAAA-MM/retrospectiva-AAAA-MM.png` (ou `base/AAAA/...`).
/// Sobrescreve só o próprio arquivo da retrospectiva, nunca outro.
pub fn salvar_png(base: &std::path::Path, periodo: &str, png: &[u8]) -> Result<std::path::PathBuf, String> {
    if periodo.is_empty() || !periodo_valido(periodo) {
        return Err("Período inválido.".into());
    }
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("A imagem gerada não é um PNG válido.".into());
    }
    let pasta = base.join(periodo);
    std::fs::create_dir_all(&pasta).map_err(|e| format!("Não foi possível criar {}: {e}", pasta.display()))?;
    let arquivo = pasta.join(format!("retrospectiva-{periodo}.png"));
    std::fs::write(&arquivo, png).map_err(|e| format!("Não foi possível gravar {}: {e}", arquivo.display()))?;
    Ok(arquivo)
}

/// Mês atual em horário local ("AAAA-MM").
pub fn mes_atual(conn: &Connection) -> rusqlite::Result<String> {
    conn.query_row("SELECT strftime('%Y-%m', 'now', 'localtime')", [], |r| r.get(0))
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

    #[test]
    fn retrospectiva_e_disponibilidade() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        tocar(&conn, "2025-12-20 10:00:00", "Velha", "Antigo", "Disco", "Rock", 100_000, true);
        for h in 10..13 {
            tocar(&conn, &format!("2026-10-05 {h}:00:00"), "Oceano", "Djavan", "Djavan", "MPB", 200_000, true);
        }
        tocar(&conn, "2026-10-06 10:00:00", "Flor de Lis", "Djavan", "A Voz", "MPB", 180_000, true);
        tocar(&conn, "2026-10-07 10:00:00", "Trevo", "Anavitória", "Anavitória", "Pop", 220_000, true);
        tocar(&conn, "2026-10-07 11:00:00", "Trevo", "Anavitória", "Anavitória", "Pop", 220_000, true);
        tocar(&conn, "2026-11-02 10:00:00", "Agora", "Novo", "Novo", "Pop", 100_000, true);

        let r = retrospectiva(&conn, "2026-10").unwrap();
        assert_eq!(r.tempo_ms, 1_220_000);
        assert_eq!(r.artistas[0].nome, "Djavan");
        assert_eq!(r.genero.as_ref().map(|g| g.nome.as_str()), Some("MPB"));
        assert_eq!(
            r.mais_repetida,
            Some(Repetida { titulo: "Oceano".into(), artista: Some("Djavan".into()), vezes: 3, dia: "2026-10-05".into() })
        );
        let d = r.dominante.unwrap();
        assert_eq!(d.nome, "Djavan");
        assert!((d.percentual - 780.0 / 1220.0 * 100.0).abs() < 0.01);
        assert!(r.musicas.len() <= 10 && r.artistas.len() <= 5 && r.albuns.len() <= 5);
        assert!(retrospectiva(&conn, "").is_err());

        // Em novembro de 2026: outubro e dezembro/2025 liberados, e o ano de 2025.
        assert_eq!(retrospectivas_disponiveis(&conn, "2026-11").unwrap(), ["2026-10", "2025", "2025-12"]);
        // Em janeiro de 2027: o ano de 2026 também.
        assert_eq!(
            retrospectivas_disponiveis(&conn, "2027-01").unwrap(),
            ["2026", "2026-11", "2026-10", "2025", "2025-12"]
        );
    }

    #[test]
    fn salva_png_na_pasta_do_periodo() {
        let base = std::env::temp_dir().join(format!("skald-teste-png-{}", std::process::id()));
        let png = b"\x89PNG\r\n\x1a\nresto";
        let arquivo = salvar_png(&base, "2026-10", png).unwrap();
        assert_eq!(arquivo, base.join("2026-10/retrospectiva-2026-10.png"));
        assert_eq!(std::fs::read(&arquivo).unwrap(), png);
        assert!(salvar_png(&base, "2026", png).unwrap().ends_with("2026/retrospectiva-2026.png"));
        assert!(salvar_png(&base, "../x", png).is_err());
        assert!(salvar_png(&base, "2026-10", b"GIF89a").is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
