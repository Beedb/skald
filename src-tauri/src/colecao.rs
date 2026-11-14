//! Visões de Artistas e Álbuns (seção 4). Agrupam sem diferenciar acentos nem maiúsculas
//! (`SEMACENTO`). Um álbum é o mesmo nome de álbum com o mesmo artista do álbum; sem essa
//! tag, as faixas da mesma pasta (como em `library::mesmo_album`).

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::library::{MediaRow, linha};

/// Pasta do arquivo (com a barra final), em SQL.
const PASTA: &str = "rtrim(path, replace(path, '/', ''))";

#[derive(Serialize, Debug)]
pub struct Artista {
    /// `None`: músicas sem artista ("Desconhecido").
    pub nome: Option<String>,
    pub albuns: i64,
    pub musicas: i64,
    /// Música cuja capa representa o artista.
    pub capa_id: i64,
}

/// Identifica um álbum: nome (pode faltar) + artista do álbum ou pasta.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ChaveAlbum {
    pub album: Option<String>,
    pub grupo: String,
}

#[derive(Serialize, Debug)]
pub struct Album {
    pub chave: ChaveAlbum,
    /// Artista do álbum; senão o artista comum às faixas; `None` se variam ("Vários artistas").
    pub artista: Option<String>,
    pub ano: Option<i64>,
    pub musicas: i64,
    pub duracao_ms: i64,
    pub capa_id: i64,
}

#[derive(Serialize, Debug)]
pub struct Faixa {
    #[serde(flatten)]
    pub media: MediaRow,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
}

pub fn artistas(conn: &Connection) -> rusqlite::Result<Vec<Artista>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT MIN(artist), COUNT(DISTINCT (COALESCE(album, '') || char(0) || COALESCE(album_artist, {PASTA})) COLLATE SEMACENTO),
                COUNT(*), MIN(id)
         FROM media WHERE category = 'music'
         GROUP BY artist COLLATE SEMACENTO
         ORDER BY artist IS NULL, artist COLLATE SEMACENTO"
    ))?;
    let linhas = stmt.query_map([], |r| {
        Ok(Artista { nome: r.get(0)?, albuns: r.get(1)?, musicas: r.get(2)?, capa_id: r.get(3)? })
    })?;
    linhas.collect()
}

/// Condição "a faixa é do artista" (nome nulo: sem artista).
const DO_ARTISTA: &str = "((?1 IS NULL AND artist IS NULL) OR artist = ?1 COLLATE SEMACENTO)";

/// Álbuns da biblioteca ou, com `artista`, só os que têm faixas dele. Agrupado em Rust
/// (uma leitura da tabela), com as chaves normalizadas sem acentos nem maiúsculas.
pub fn albuns(conn: &Connection, artista: Option<Option<&str>>) -> rusqlite::Result<Vec<Album>> {
    use crate::search::normalizar;
    use std::collections::HashMap;

    struct Acum {
        album: Option<String>,
        grupo: String,
        album_artist: Option<String>,
        artistas: Vec<Option<String>>,
        ano: Option<i64>,
        musicas: i64,
        duracao_ms: i64,
        capa: (i64, i64, i64), // (disco, faixa, id) da primeira faixa
        do_artista: bool,
    }
    let alvo = artista.map(|a| a.map(normalizar));
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT id, album, album_artist, artist, COALESCE(album_artist, {PASTA}), year, duration_ms,
                disc_number, track_number
         FROM media WHERE category = 'music' ORDER BY album, id"
    ))?;
    let mut grupos: HashMap<(Option<String>, String), Acum> = HashMap::new();
    let mut ordem = Vec::new();
    let mut linhas = stmt.query([])?;
    while let Some(r) = linhas.next()? {
        let id: i64 = r.get(0)?;
        let album: Option<String> = r.get(1)?;
        let album_artist: Option<String> = r.get(2)?;
        let artist: Option<String> = r.get(3)?;
        let grupo: String = r.get(4)?;
        let ano: Option<i64> = r.get(5)?;
        let dur: Option<i64> = r.get(6)?;
        let disco: Option<i64> = r.get(7)?;
        let faixa: Option<i64> = r.get(8)?;
        let chave = (album.as_deref().map(normalizar), normalizar(&grupo));
        let a = grupos.entry(chave.clone()).or_insert_with(|| {
            ordem.push(chave);
            Acum {
                album: album.clone(),
                grupo,
                album_artist: None,
                artistas: Vec::new(),
                ano: None,
                musicas: 0,
                duracao_ms: 0,
                capa: (i64::MAX, i64::MAX, i64::MAX),
                do_artista: false,
            }
        });
        a.album_artist = a.album_artist.take().or(album_artist);
        if let Some(alvo) = &alvo {
            a.do_artista |= artist.as_deref().map(normalizar) == *alvo;
        }
        a.artistas.push(artist);
        a.ano = a.ano.max(ano);
        a.musicas += 1;
        a.duracao_ms += dur.unwrap_or(0);
        a.capa = a.capa.min((disco.unwrap_or(0), faixa.unwrap_or(0), id));
    }
    let mut lista: Vec<Album> = ordem
        .into_iter()
        .filter_map(|k| grupos.remove(&k))
        .filter(|a| alvo.is_none() || a.do_artista)
        .map(|a| {
            // Artista do álbum; senão o artista comum a todas as faixas.
            // Mesma grafia que a lista de artistas mostra (a menor, como o MIN do SQL).
            let primeiro = a.artistas.iter().flatten().min().cloned();
            let comum = a.artistas.iter().all(|x| x.is_some() && x.as_deref().map(normalizar) == primeiro.as_deref().map(normalizar));
            Album {
                chave: ChaveAlbum { album: a.album, grupo: a.grupo },
                artista: a.album_artist.or(if comum { primeiro } else { None }),
                ano: a.ano,
                musicas: a.musicas,
                duracao_ms: a.duracao_ms,
                capa_id: a.capa.2,
            }
        })
        .collect();
    let chave_ordem = |a: &Album| {
        (
            a.chave.album.is_none(),
            a.chave.album.as_deref().map(normalizar),
            a.artista.as_deref().map(normalizar),
        )
    };
    lista.sort_by_cached_key(chave_ordem);
    Ok(lista)
}

/// Faixas do álbum em ordem de disco e faixa.
pub fn faixas_do_album(conn: &Connection, chave: &ChaveAlbum) -> rusqlite::Result<Vec<Faixa>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT id, title, artist, album, duration_ms, missing, track_number, disc_number FROM media
         WHERE category = 'music'
           AND ((?1 IS NULL AND album IS NULL) OR album = ?1 COLLATE SEMACENTO)
           AND COALESCE(album_artist, {PASTA}) = ?2 COLLATE SEMACENTO
         ORDER BY disc_number, track_number, title COLLATE SEMACENTO, id"
    ))?;
    let linhas = stmt.query_map(params![chave.album, chave.grupo], |r| {
        Ok(Faixa { media: linha(r)?, track_number: r.get(6)?, disc_number: r.get(7)? })
    })?;
    linhas.collect()
}

/// Todas as músicas do artista, na ordem da biblioteca (álbum, disco, faixa).
pub fn ids_do_artista(conn: &Connection, nome: Option<&str>) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT id FROM media WHERE category = 'music' AND {DO_ARTISTA}
         ORDER BY album IS NULL, album COLLATE SEMACENTO, disc_number, track_number, title COLLATE SEMACENTO, id"
    ))?;
    let ids = stmt.query_map([nome], |r| r.get(0))?;
    ids.collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn agrupa_artistas_e_albuns() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        conn.execute_batch(
            "INSERT INTO media (id, path, title, artist, album, album_artist, track_number, duration_ms) VALUES
             (1, '/m/a/2.mp3', 'Dois', 'Anirap', 'Ágape', NULL, 2, 100),
             (2, '/m/a/1.mp3', 'Um', 'anirap', 'agape', NULL, 1, 100),
             (3, '/m/c/1.mp3', 'X', 'Djavan', 'Coletânea', 'Vários', 1, 100),
             (4, '/m/d/2.mp3', 'Y', 'Anirap', 'Coletânea', 'Vários', 2, 100),
             (5, '/m/e/solta.mp3', 'Sem', NULL, NULL, NULL, NULL, 100),
             (6, '/m/f/1.mp3', 'Outro', 'Lia', 'Ágape', NULL, 1, 100);",
        )
        .unwrap();
        let a = artistas(&conn).unwrap();
        let resumo: Vec<_> = a.iter().map(|x| (x.nome.clone(), x.albuns, x.musicas)).collect();
        assert_eq!(
            resumo,
            [
                (Some("Anirap".into()), 2, 3),
                (Some("Djavan".into()), 1, 1),
                (Some("Lia".into()), 1, 1),
                (None, 1, 1)
            ]
        );

        let todos = albuns(&conn, None).unwrap();
        let nomes: Vec<_> = todos.iter().map(|x| (x.chave.album.clone(), x.artista.clone(), x.musicas)).collect();
        // "Ágape" de Anirap (pasta /m/a/) e de Lia (/m/f/) são álbuns diferentes.
        assert_eq!(
            nomes,
            [
                (Some("agape".into()), Some("Anirap".into()), 2),
                (Some("Ágape".into()), Some("Lia".into()), 1),
                (Some("Coletânea".into()), Some("Vários".into()), 2),
                (None, None, 1)
            ]
        );
        assert_eq!(todos[0].capa_id, 2); // faixa 1

        let de_anirap = albuns(&conn, Some(Some("ANIRAP"))).unwrap();
        assert_eq!(de_anirap.len(), 2);
        let faixas = faixas_do_album(&conn, &todos[0].chave).unwrap();
        assert_eq!(faixas.iter().map(|f| f.media.id).collect::<Vec<_>>(), [2, 1]);
        let col = faixas_do_album(&conn, &todos[2].chave).unwrap();
        assert_eq!(col.iter().map(|f| f.media.id).collect::<Vec<_>>(), [3, 4]);
        let sem = albuns(&conn, Some(None)).unwrap();
        assert_eq!(sem.len(), 1);
        assert_eq!(faixas_do_album(&conn, &sem[0].chave).unwrap().len(), 1);
        assert_eq!(ids_do_artista(&conn, Some("Anirap")).unwrap(), [2, 1, 4]);
    }
}

#[cfg(test)]
mod ensaio {
    /// Rodar com: SKALD_DB_ENSAIO=/copia.db cargo test ensaio_colecao -- --nocapture
    #[test]
    fn ensaio_colecao() {
        let Ok(caminho) = std::env::var("SKALD_DB_ENSAIO") else { return };
        let conn = crate::db::open(std::path::Path::new(&caminho)).unwrap();
        let t = std::time::Instant::now();
        let a = super::artistas(&conn).unwrap();
        let b = super::albuns(&conn, None).unwrap();
        println!("{} artistas, {} álbuns em {:?}", a.len(), b.len(), t.elapsed());
        for x in b.iter().take(8) {
            println!("{:?} — {:?} ({})", x.chave.album, x.artista, x.musicas);
        }
    }
}
