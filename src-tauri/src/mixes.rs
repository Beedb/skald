//! Mixes automáticos da tela inicial: um geral ("Mix de músicas"), um por gênero mais ouvido e,
//! se faltarem gêneros (tags vazias), um por artista mais ouvido.
//! Tudo calculado na hora a partir da biblioteca e do histórico; nada é gravado.

use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;

use crate::search::normalizar;

/// Gêneros com menos músicas que isso não viram mix.
const MINIMO: usize = 5;
/// Quantos mixes de gênero/artista mostrar.
const GRUPOS: usize = 5;
/// Tamanho máximo de um mix ao tocar.
const TAMANHO: usize = 100;

#[derive(Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Tipo {
    Geral,
    Genero,
    Artista,
}

#[derive(Serialize, Debug)]
pub struct Mix {
    pub tipo: Tipo,
    /// Gênero ou artista (nulo no geral).
    pub valor: Option<String>,
    pub nome: String,
    pub descricao: String,
    pub musicas: usize,
    /// Até 4 músicas (de álbuns diferentes) cujas capas formam a arte do mix.
    pub capas: Vec<i64>,
}

struct Linha {
    id: i64,
    album: Option<String>,
    generos: Vec<String>,
    artistas: Vec<String>,
    escutas: i64,
}

/// "Rock; Alternativo" e "R&B/Soul" contam para cada parte.
fn generos(texto: &str) -> Vec<String> {
    texto.split([';', '/', '|']).map(str::trim).filter(|g| !g.is_empty()).map(str::to_string).collect()
}

fn ler(conn: &Connection) -> rusqlite::Result<Vec<Linha>> {
    let mut stmt = conn.prepare_cached(
        "SELECT m.id, m.album, m.genre, m.artist,
                (SELECT COUNT(*) FROM play_history h WHERE h.media_id = m.id AND h.counted = 1)
         FROM media m WHERE m.category = 'music' AND m.missing = 0",
    )?;
    let apelidos = crate::artistas::mapa(conn);
    let linhas = stmt.query_map([], |r| {
        let genero: Option<String> = r.get(2)?;
        let artista: Option<String> = r.get(3)?;
        Ok(Linha {
            id: r.get(0)?,
            album: r.get(1)?,
            generos: genero.as_deref().map(generos).unwrap_or_default(),
            artistas: artista.as_deref().map(|a| crate::artistas::separar_mapeado(a, &apelidos)).unwrap_or_default(),
            escutas: r.get(4)?,
        })
    })?;
    linhas.collect()
}

/// Até 4 capas de álbuns diferentes, das mais ouvidas.
fn capas(linhas: &[&Linha]) -> Vec<i64> {
    let mut ordem: Vec<&&Linha> = linhas.iter().collect();
    ordem.sort_by(|a, b| b.escutas.cmp(&a.escutas).then(a.id.cmp(&b.id)));
    let mut vistos = std::collections::HashSet::new();
    let mut out = Vec::new();
    for l in ordem {
        let album = l.album.as_deref().map(normalizar);
        if album.is_some() && !vistos.insert(album) {
            continue;
        }
        out.push(l.id);
        if out.len() == 4 {
            break;
        }
    }
    out
}

fn valores(l: &Linha, tipo: Tipo) -> &[String] {
    if tipo == Tipo::Genero { &l.generos } else { &l.artistas }
}

/// Gênero/artista normalizado -> (grafia mais comum, músicas).
fn agrupar(linhas: &[Linha], tipo: Tipo) -> HashMap<String, (String, Vec<&Linha>)> {
    let mut grupos: HashMap<String, (HashMap<&str, usize>, Vec<&Linha>)> = HashMap::new();
    for l in linhas {
        for g in valores(l, tipo) {
            let e = grupos.entry(normalizar(g)).or_default();
            *e.0.entry(g.as_str()).or_default() += 1;
            e.1.push(l);
        }
    }
    grupos
        .into_iter()
        .map(|(k, (grafias, ls))| {
            let nome = grafias.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0))).map(|g| g.0.to_string()).unwrap_or_default();
            (k, (nome, ls))
        })
        .collect()
}

/// Grupos com músicas suficientes, mais ouvidos primeiro (sem histórico, os maiores).
fn ranking(linhas: &[Linha], tipo: Tipo) -> Vec<(String, Vec<&Linha>)> {
    let mut grupos: Vec<_> = agrupar(linhas, tipo).into_values().filter(|(_, ls)| ls.len() >= MINIMO).collect();
    let peso = |ls: &Vec<&Linha>| ls.iter().map(|l| l.escutas).sum::<i64>() * 10 + ls.len() as i64;
    grupos.sort_by(|a, b| peso(&b.1).cmp(&peso(&a.1)).then(a.0.cmp(&b.0)));
    grupos
}

pub fn listar(conn: &Connection) -> rusqlite::Result<Vec<Mix>> {
    let linhas = ler(conn)?;
    let mut out = Vec::new();
    if linhas.len() >= MINIMO {
        let todas: Vec<&Linha> = linhas.iter().collect();
        let ouviu = linhas.iter().any(|l| l.escutas > 0);
        out.push(Mix {
            tipo: Tipo::Geral,
            valor: None,
            nome: "Mix de músicas".into(),
            descricao: if ouviu { "Com base no que você ouve".into() } else { "Uma seleção da sua biblioteca".into() },
            musicas: linhas.len().min(TAMANHO),
            capas: capas(&todas),
        });
    }
    let generos = ranking(&linhas, Tipo::Genero);
    let faltam = GRUPOS.saturating_sub(generos.len());
    let artistas = ranking(&linhas, Tipo::Artista);
    let grupos = generos
        .into_iter()
        .take(GRUPOS)
        .map(|g| (Tipo::Genero, g))
        .chain(artistas.into_iter().take(faltam).map(|g| (Tipo::Artista, g)));
    for (tipo, (nome, ls)) in grupos {
        let qtd = format!("{} {}", ls.len(), if ls.len() == 1 { "música" } else { "músicas" });
        out.push(Mix {
            tipo,
            descricao: if tipo == Tipo::Artista { format!("{qtd} do artista") } else { qtd },
            nome: if tipo == Tipo::Artista { format!("Mix {nome}") } else { format!("Mix de {nome}") },
            musicas: ls.len().min(TAMANHO),
            capas: capas(&ls),
            valor: Some(nome),
        });
    }
    Ok(out)
}

/// Sorteio ponderado (Efraimidis–Spirakis): músicas mais ouvidas tendem a vir antes,
/// mas todas têm chance. Devolve até `TAMANHO` ids.
fn sortear(linhas: &[&Linha], reforco: f64) -> Vec<i64> {
    let mut chaves: Vec<(f64, i64)> = linhas
        .iter()
        .map(|l| {
            let peso = 1.0 + l.escutas as f64 * reforco;
            (fastrand::f64().powf(1.0 / peso), l.id)
        })
        .collect();
    chaves.sort_by(|a, b| b.0.total_cmp(&a.0));
    chaves.into_iter().take(TAMANHO).map(|(_, id)| id).collect()
}

/// Músicas do mix, já sorteadas.
pub fn ids(conn: &Connection, tipo: Tipo, valor: Option<&str>) -> rusqlite::Result<Vec<i64>> {
    let linhas = ler(conn)?;
    Ok(match (tipo, valor) {
        (Tipo::Geral, _) | (_, None) => sortear(&linhas.iter().collect::<Vec<_>>(), 3.0),
        (_, Some(v)) => {
            let k = normalizar(v.trim());
            let ls: Vec<&Linha> = linhas.iter().filter(|l| valores(l, tipo).iter().any(|x| normalizar(x) == k)).collect();
            sortear(&ls, 1.0)
        }
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use rusqlite::params;

    #[test]
    fn mixes_por_genero_mais_ouvido() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        // 6 de rock (um "Rock; Indie"), 5 de jazz, 2 de pop (não vira mix), 1 ausente de rock.
        let generos = ["rock", "Rock", "Rock", "Rock", "Rock", "Rock; Indie", "Jazz", "Jazz", "Jazz", "Jazz", "Jazz", "Pop", "Pop"];
        for (i, g) in generos.iter().enumerate() {
            conn.execute(
                "INSERT INTO media (id, path, title, album, genre) VALUES (?1, ?2, 't', ?3, ?4)",
                params![i as i64 + 1, format!("/m/{i}.mp3"), format!("A{}", i % 3), g],
            )
            .unwrap();
        }
        conn.execute("INSERT INTO media (id, path, title, genre, missing) VALUES (99, '/m/x.mp3', 't', 'Rock', 1)", []).unwrap();
        // Jazz bem ouvido passa à frente do rock, que tem mais músicas.
        for _ in 0..3 {
            conn.execute("INSERT INTO play_history (media_id, started_at, counted) VALUES (7, 1, 1)", []).unwrap();
        }
        let m = listar(&conn).unwrap();
        let nomes: Vec<_> = m.iter().map(|x| x.nome.as_str()).collect();
        assert_eq!(nomes, ["Mix de músicas", "Mix de Jazz", "Mix de Rock"]);
        assert_eq!(m[0].descricao, "Com base no que você ouve");
        assert_eq!(m[2].musicas, 6);
        assert_eq!(m[1].capas[0], 7);
        assert!(m[1].capas.len() <= 3, "um por álbum");

        let mut rock = ids(&conn, Tipo::Genero, Some("ROCK")).unwrap();
        rock.sort();
        assert_eq!(rock, [1, 2, 3, 4, 5, 6]);
        assert_eq!(ids(&conn, Tipo::Geral, None).unwrap().len(), 13);
        // Sem artistas nas tags, só 2 gêneros: nenhum mix de artista entra.
        assert!(m.iter().all(|x| x.tipo != Tipo::Artista));
    }

    #[test]
    fn sem_generos_usa_artistas() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        for i in 0..12i64 {
            let artista = if i < 7 { "Radiohead" } else if i < 11 { "Björk" } else { "Radiohead feat. Björk" };
            conn.execute(
                "INSERT INTO media (id, path, title, artist) VALUES (?1, ?2, 't', ?3)",
                params![i + 1, format!("/m/{i}.mp3"), artista],
            )
            .unwrap();
        }
        let m = listar(&conn).unwrap();
        let nomes: Vec<_> = m.iter().map(|x| x.nome.as_str()).collect();
        assert_eq!(nomes, ["Mix de músicas", "Mix Radiohead", "Mix Björk"]);
        assert_eq!(m[2].musicas, 5, "a parceria conta para os dois");
        let mut b = ids(&conn, Tipo::Artista, Some("björk")).unwrap();
        b.sort();
        assert_eq!(b, [8, 9, 10, 11, 12]);
    }
}

#[cfg(test)]
mod ensaio {
    /// Rodar com: SKALD_DB_ENSAIO=/copia.db cargo test ensaio_mixes -- --nocapture
    #[test]
    fn ensaio_mixes() {
        let Ok(caminho) = std::env::var("SKALD_DB_ENSAIO") else { return };
        let conn = crate::db::open(std::path::Path::new(&caminho)).unwrap();
        let t = std::time::Instant::now();
        for m in super::listar(&conn).unwrap() {
            println!("{} — {} ({} músicas, capas {:?})", m.nome, m.descricao, m.musicas, m.capas);
        }
        println!("{:?}", t.elapsed());
    }
}
