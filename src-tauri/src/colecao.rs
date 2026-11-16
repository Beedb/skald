//! Visões de Artistas e Álbuns (seção 4). Agrupam sem diferenciar acentos nem maiúsculas
//! (`SEMACENTO`). Um álbum é o mesmo nome de álbum com o mesmo artista do álbum; sem essa
//! tag, as faixas da mesma pasta (como em `library::mesmo_album`).

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::library::{MediaRow, linha};

/// Pasta do arquivo (com a barra final), em SQL.
const PASTA: &str = crate::library::PASTA_SQL;

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
    /// Quando a música mais recente do álbum entrou na biblioteca (segundos Unix).
    pub adicionado_em: i64,
}

#[derive(Serialize, Debug)]
pub struct Faixa {
    #[serde(flatten)]
    pub media: MediaRow,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
    /// Créditos separados (para um link por artista).
    pub artistas: Vec<String>,
}

/// Artistas da biblioteca (créditos divididos, apelidos aplicados). Agrupado em Rust numa
/// leitura só: com a collation `SEMACENTO` no SQL, cada comparação normalizava os textos.
pub fn artistas(conn: &Connection) -> rusqlite::Result<Vec<Artista>> {
    use crate::search::normalizar;
    use std::collections::{HashMap, HashSet};

    struct Acum {
        nome: Option<String>,
        albuns: HashSet<(String, String)>,
        musicas: i64,
        capa_id: i64,
    }
    let apelidos = crate::artistas::mapa(conn);
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT id, artist, album, COALESCE(album_artist, {PASTA}) FROM media WHERE category = 'music'"
    ))?;
    let mut grupos: HashMap<Option<String>, Acum> = HashMap::new();
    let mut linhas = stmt.query([])?;
    while let Some(r) = linhas.next()? {
        let id: i64 = r.get(0)?;
        let artist: Option<String> = r.get(1)?;
        let album: Option<String> = r.get(2)?;
        let grupo: String = r.get(3)?;
        let chave_album = (album.as_deref().map(normalizar).unwrap_or_default(), normalizar(&grupo));
        let nomes = artist.as_deref().map(|t| crate::artistas::separar_mapeado(t, &apelidos)).unwrap_or_default();
        let nomes: Vec<Option<String>> = if nomes.is_empty() { vec![None] } else { nomes.into_iter().map(Some).collect() };
        for nome in nomes {
            let a = grupos.entry(nome.as_deref().map(normalizar)).or_insert_with(|| Acum {
                nome: nome.clone(),
                albuns: HashSet::new(),
                musicas: 0,
                capa_id: id,
            });
            // Mesma grafia de exibição que antes (a menor, como o MIN do SQL).
            if nome < a.nome {
                a.nome = nome;
            }
            a.albuns.insert(chave_album.clone());
            a.musicas += 1;
            a.capa_id = a.capa_id.min(id);
        }
    }
    let mut lista: Vec<(Option<String>, Artista)> = grupos
        .into_iter()
        .map(|(chave, a)| {
            (chave, Artista { nome: a.nome, albuns: a.albuns.len() as i64, musicas: a.musicas, capa_id: a.capa_id })
        })
        .collect();
    // Sem artista no fim; o resto em ordem alfabética sem acento.
    lista.sort_by(|(a, _), (b, _)| (a.is_none(), a).cmp(&(b.is_none(), b)));
    Ok(lista.into_iter().map(|(_, a)| a).collect())
}

/// Condição "a faixa tem o artista entre os créditos" (nome nulo: sem artista).
const DO_ARTISTA: &str = "EXISTS (SELECT 1 FROM json_each(ARTISTAS(artist)) a
    WHERE (?1 IS NULL AND a.value IS NULL) OR a.value = ?1 COLLATE SEMACENTO)";

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
        adicionado_em: i64,
    }
    let alvo = artista.map(|a| a.map(normalizar));
    let apelidos = crate::artistas::mapa(conn);
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT id, album, album_artist, artist, COALESCE(album_artist, {PASTA}), year, duration_ms,
                disc_number, track_number, added_at
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
        let adicionado: i64 = r.get(9)?;
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
                adicionado_em: 0,
            }
        });
        a.adicionado_em = a.adicionado_em.max(adicionado);
        a.album_artist = a.album_artist.take().or(album_artist);
        if let Some(alvo) = &alvo {
            a.do_artista |= match (artist.as_deref().map(|t| crate::artistas::separar_mapeado(t, &apelidos)), alvo) {
                (Some(nomes), Some(alvo)) => nomes.iter().any(|n| normalizar(n) == *alvo),
                (Some(nomes), None) => nomes.is_empty(),
                (None, alvo) => alvo.is_none(),
            };
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
                adicionado_em: a.adicionado_em,
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

/// Álbuns adicionados por último à biblioteca (tela inicial).
pub fn albuns_recentes(conn: &Connection, n: usize) -> rusqlite::Result<Vec<Album>> {
    let mut lista = albuns(conn, None)?;
    lista.sort_by_key(|a| std::cmp::Reverse(a.adicionado_em));
    lista.truncate(n);
    Ok(lista)
}

/// Músicas tocadas por último, sem repetir (tela inicial).
pub fn tocadas_recentes(conn: &Connection, n: usize) -> rusqlite::Result<Vec<MediaRow>> {
    let ids: Vec<i64> = conn
        .prepare_cached(
            "SELECT media_id FROM play_history WHERE media_id IS NOT NULL AND category = 'music'
             GROUP BY media_id ORDER BY MAX(started_at) DESC LIMIT ?1",
        )?
        .query_map([n as i64], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    crate::library::obter_varios(conn, &ids)
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
    let apelidos = crate::artistas::mapa(conn);
    let linhas = stmt.query_map(params![chave.album, chave.grupo], |r| {
        let media = linha(r)?;
        let artistas = media.artist.as_deref().map(|t| crate::artistas::separar_mapeado(t, &apelidos)).unwrap_or_default();
        Ok(Faixa { media, track_number: r.get(6)?, disc_number: r.get(7)?, artistas })
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

/// "Continuar tocando" depois de `id`: toda a música presente da biblioteca, em grupos
/// embaralhados — primeiro do mesmo artista (qualquer participante), depois do mesmo
/// gênero, depois o resto (Shuffle Bag da biblioteca inteira). `id` fica de fora.
pub fn continuacao(conn: &Connection, id: i64) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare_cached(
        "WITH base AS (SELECT artist, genre FROM media WHERE id = ?1)
         SELECT m.id,
                CASE
                    WHEN EXISTS (SELECT 1 FROM base, json_each(ARTISTAS(m.artist)) a, json_each(ARTISTAS(base.artist)) b
                                 WHERE a.value IS NOT NULL AND a.value = b.value COLLATE SEMACENTO) THEN 0
                    WHEN m.genre IS NOT NULL AND m.genre = (SELECT genre FROM base) COLLATE SEMACENTO THEN 1
                    ELSE 2
                END
         FROM media m
         WHERE m.category = 'music' AND m.missing = 0 AND m.id != ?1",
    )?;
    let mut grupos: [Vec<i64>; 3] = Default::default();
    for linha in stmt.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, usize>(1)?)))? {
        let (m, g) = linha?;
        grupos[g.min(2)].push(m);
    }
    for g in &mut grupos {
        fastrand::shuffle(g);
    }
    Ok(grupos.concat())
}

/// "Relacionados" do player completo: o resto do álbum e mais músicas dos artistas da faixa.
#[derive(Serialize, Debug)]
pub struct Relacionadas {
    pub album: Vec<MediaRow>,
    pub artistas: Vec<String>,
    pub do_artista: Vec<MediaRow>,
}

const MAX_DO_ARTISTA: usize = 20;

pub fn relacionadas(conn: &Connection, id: i64) -> rusqlite::Result<Relacionadas> {
    let (album, album_artist, artista, caminho): (Option<String>, Option<String>, Option<String>, String) = conn
        .query_row("SELECT album, album_artist, artist, path FROM media WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
    let pasta = |p: &str| std::path::Path::new(p).parent().map(|x| x.to_path_buf());
    let norm = |s: &str| crate::search::normalizar(s.trim());

    // Mesmo álbum: mesmo nome e (mesma pasta ou mesmo artista do álbum), como em `library::mesmo_album`.
    let mut ids_album = Vec::new();
    if let Some(nome) = &album {
        let mut stmt = conn.prepare_cached(
            "SELECT id, album_artist, path FROM media
             WHERE category = 'music' AND album = ?1 COLLATE SEMACENTO AND id <> ?2
             ORDER BY disc_number, track_number, title COLLATE SEMACENTO, id",
        )?;
        let linhas = stmt.query_map(params![nome, id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?)))?;
        for l in linhas {
            let (outro, aa, p) = l?;
            let mesmo_artista = matches!((&album_artist, &aa), (Some(x), Some(y)) if norm(x) == norm(y));
            if mesmo_artista || pasta(&p) == pasta(&caminho) {
                ids_album.push(outro);
            }
        }
    }

    let apelidos = crate::artistas::mapa(conn);
    let artistas = artista.as_deref().map(|a| crate::artistas::separar_mapeado(a, &apelidos)).unwrap_or_default();
    let mut vistos: std::collections::HashSet<i64> = ids_album.iter().copied().collect();
    vistos.insert(id);
    let mut ids_artista = Vec::new();
    for nome in &artistas {
        for outro in ids_do_artista(conn, Some(nome))? {
            if ids_artista.len() == MAX_DO_ARTISTA {
                break;
            }
            if vistos.insert(outro) {
                ids_artista.push(outro);
            }
        }
    }
    Ok(Relacionadas {
        album: crate::library::obter_varios(conn, &ids_album)?,
        artistas,
        do_artista: crate::library::obter_varios(conn, &ids_artista)?,
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn creditos_divididos_contam_para_cada_artista() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        for (id, artista, album) in [(1, "Rodrigo Zin/GEO", "Invasão"), (2, "Rodrigo Zin", "Invasão"), (3, "GEO feat. Hiro", "Solo")] {
            conn.execute(
                "INSERT INTO media (id, path, title, artist, album) VALUES (?1, ?2, 't', ?3, ?4)",
                params![id, format!("/m/{album}/{id}.mp3"), artista, album],
            )
            .unwrap();
        }
        let nomes: Vec<_> = artistas(&conn).unwrap().into_iter().map(|a| (a.nome.unwrap(), a.musicas)).collect();
        assert_eq!(nomes, [("GEO".to_string(), 2), ("Hiro".to_string(), 1), ("Rodrigo Zin".to_string(), 2)]);
        assert_eq!(ids_do_artista(&conn, Some("geo")).unwrap(), [1, 3]);
        let albuns_geo: Vec<_> = albuns(&conn, Some(Some("GEO"))).unwrap().into_iter().map(|a| a.chave.album.unwrap()).collect();
        assert_eq!(albuns_geo, ["Invasão", "Solo"]);
        let f = faixas_do_album(&conn, &ChaveAlbum { album: Some("Invasão".into()), grupo: "/m/Invasão/".into() }).unwrap();
        assert_eq!(f[0].artistas, ["Rodrigo Zin", "GEO"]);
        let r = relacionadas(&conn, 1).unwrap();
        assert_eq!(r.album.iter().map(|m| m.id).collect::<Vec<_>>(), [2]);
        assert_eq!(r.artistas, ["Rodrigo Zin", "GEO"]);
        assert_eq!(r.do_artista.iter().map(|m| m.id).collect::<Vec<_>>(), [3], "sem repetir as do álbum");
    }

    #[test]
    fn continuacao_por_artista_genero_e_resto() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        conn.execute_batch(
            "INSERT INTO media (id, path, title, artist, genre, missing) VALUES
             (1, '/m/1.mp3', 't', 'Djavan', 'MPB', 0),
             (2, '/m/2.mp3', 't', 'djavan feat. Gal', 'Pop', 0),
             (3, '/m/3.mp3', 't', 'Gal', 'mpb', 0),
             (4, '/m/4.mp3', 't', 'Outro', NULL, 0),
             (5, '/m/5.mp3', 't', 'Djavan', 'MPB', 1);",
        )
        .unwrap();
        // Ausente (5) e a própria (1) ficam de fora; cada grupo tem um só aqui.
        assert_eq!(continuacao(&conn, 1).unwrap(), [2, 3, 4]);
    }

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

        conn.execute_batch(
            "UPDATE media SET added_at = 100; UPDATE media SET added_at = 900 WHERE id = 6;
             INSERT INTO play_history (media_id, started_at) VALUES (3, 10), (1, 20), (3, 30);",
        )
        .unwrap();
        let rec = albuns_recentes(&conn, 2).unwrap();
        assert_eq!(rec[0].artista.as_deref(), Some("Lia"));
        assert_eq!(rec.len(), 2);
        let tocadas: Vec<_> = tocadas_recentes(&conn, 5).unwrap().iter().map(|m| m.id).collect();
        assert_eq!(tocadas, [3, 1]);
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
        let ta = t.elapsed();
        let b = super::albuns(&conn, None).unwrap();
        println!("{} artistas em {ta:?}, {} álbuns em {:?}", a.len(), b.len(), t.elapsed() - ta);
        for x in b.iter().take(8) {
            println!("{:?} — {:?} ({})", x.chave.album, x.artista, x.musicas);
        }
    }
}
