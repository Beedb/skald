//! Apelidos de artistas (seção 11.3): o usuário diz que "Anirap" e "AniRap Oficial" são o
//! mesmo artista. A troca acontece em `artistas::ARTISTAS`, então vale para a coleção e as
//! estatísticas (inclusive o histórico antigo). As tags dos arquivos não mudam.

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::search::normalizar;

#[derive(Serialize, Debug, PartialEq)]
pub struct Apelido {
    pub alias: String,
    pub canonical: String,
}

pub fn listar(conn: &Connection) -> rusqlite::Result<Vec<Apelido>> {
    let mut stmt = conn.prepare("SELECT alias, canonical FROM artist_alias ORDER BY canonical COLLATE SEMACENTO, alias COLLATE SEMACENTO")?;
    let linhas = stmt.query_map([], |r| Ok(Apelido { alias: r.get(0)?, canonical: r.get(1)? }))?;
    linhas.collect()
}

/// `alias` passa a contar como `canonical`. Cadeias são resolvidas: se `canonical` já é
/// apelido de outro nome, vale o nome final; apelidos que apontavam para `alias` passam
/// a apontar para o novo nome principal.
pub fn adicionar(conn: &Connection, alias: &str, canonical: &str) -> Result<(), String> {
    let (alias, canonical) = (alias.trim(), canonical.trim());
    if alias.is_empty() || canonical.is_empty() {
        return Err("Informe os dois nomes.".into());
    }
    if alias.chars().count() > 200 || canonical.chars().count() > 200 {
        return Err("Nome longo demais.".into());
    }
    let destino: String = conn
        .query_row("SELECT canonical FROM artist_alias WHERE alias_norm = ?1", [normalizar(canonical)], |r| r.get(0))
        .unwrap_or_else(|_| canonical.to_string());
    if normalizar(alias) == normalizar(&destino) {
        return Err("Os dois nomes são o mesmo artista.".into());
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO artist_alias (alias_norm, alias, canonical) VALUES (?1, ?2, ?3)
         ON CONFLICT(alias_norm) DO UPDATE SET alias = excluded.alias, canonical = excluded.canonical",
        params![normalizar(alias), alias, destino],
    )
    .map_err(|e| e.to_string())?;
    // Quem apontava para o apelido novo agora aponta para o nome principal.
    let antigos: Vec<String> = {
        let mut stmt = tx.prepare("SELECT alias_norm, canonical FROM artist_alias").map_err(|e| e.to_string())?;
        let linhas = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        linhas
            .filter_map(Result::ok)
            .filter(|(_, c)| normalizar(c) == normalizar(alias))
            .map(|(a, _)| a)
            .collect()
    };
    for a in antigos {
        tx.execute("UPDATE artist_alias SET canonical = ?2 WHERE alias_norm = ?1", params![a, destino])
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    crate::artistas::apelidos_mudaram();
    Ok(())
}

pub fn remover(conn: &Connection, alias: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM artist_alias WHERE alias_norm = ?1", [normalizar(alias)])?;
    crate::artistas::apelidos_mudaram();
    Ok(())
}

/// Chave para achar grafias parecidas: sem acento, maiúsculas, espaços nem pontuação.
fn chave_parecida(nome: &str) -> String {
    normalizar(nome).chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Grupos de nomes de artistas (já divididos e com apelidos aplicados) que diferem só em
/// espaços ou pontuação. Ex.: "7 Minutoz" e "7minutoz".
pub fn sugestoes(conn: &Connection) -> rusqlite::Result<Vec<Vec<String>>> {
    let nomes: Vec<(String, i64)> = crate::colecao::artistas(conn)?
        .into_iter()
        .filter_map(|a| Some((a.nome?, a.musicas)))
        .collect();
    let mut grupos: std::collections::BTreeMap<String, Vec<(String, i64)>> = Default::default();
    for (nome, n) in nomes {
        let k = chave_parecida(&nome);
        if !k.is_empty() {
            grupos.entry(k).or_default().push((nome, n));
        }
    }
    Ok(grupos
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|mut g| {
            // O mais usado primeiro: é o candidato natural a nome principal.
            g.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            g.into_iter().map(|(n, _)| n).collect()
        })
        .collect())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn banco() -> Connection {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        for (id, artista) in [(1, "Anirap"), (2, "AniRap Oficial"), (3, "7 Minutoz"), (4, "7minutoz"), (5, "7 Minutoz/Anirap Oficial")] {
            conn.execute(
                "INSERT INTO media (id, path, title, artist, album) VALUES (?1, ?2, 't', ?3, 'A')",
                params![id, format!("/m/{id}.mp3"), artista],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn apelido_une_na_colecao_e_nas_estatisticas() {
        let conn = banco();
        adicionar(&conn, "AniRap Oficial", "Anirap").unwrap();
        let artistas: Vec<_> = crate::colecao::artistas(&conn).unwrap().into_iter().map(|a| (a.nome.unwrap(), a.musicas)).collect();
        assert_eq!(artistas, [("7 Minutoz".to_string(), 2), ("7minutoz".to_string(), 1), ("Anirap".to_string(), 3)]);
        assert_eq!(crate::colecao::ids_do_artista(&conn, Some("anirap")).unwrap(), [1, 2, 5]);
        // O histórico guarda o texto: o apelido vale também para o passado.
        conn.execute("INSERT INTO play_history (artist, started_at, listened_ms, counted) VALUES ('AniRap Oficial', 0, 1, 1), ('Anirap', 0, 1, 1)", [])
            .unwrap();
        assert_eq!(crate::history::estatisticas(&conn).unwrap().artistas_distintos, 1);
        remover(&conn, "anirap oficial").unwrap();
        assert!(listar(&conn).unwrap().is_empty());
    }

    #[test]
    fn cadeias_e_validacao() {
        let conn = banco();
        adicionar(&conn, "B", "A").unwrap();
        adicionar(&conn, "C", "B").unwrap(); // C → A
        adicionar(&conn, "A", "Z").unwrap(); // A → Z, e B e C passam a Z
        let todos: Vec<_> = listar(&conn).unwrap().into_iter().map(|a| (a.alias, a.canonical)).collect();
        assert_eq!(todos, [("A".into(), "Z".into()), ("B".into(), "Z".into()), ("C".into(), "Z".into())]);
        assert!(adicionar(&conn, "z", "Z").is_err());
        assert!(adicionar(&conn, "", "Z").is_err());
    }

    /// SKALD_DB_ENSAIO=/copia.db cargo test ensaio_apelidos -- --nocapture
    #[test]
    fn ensaio_apelidos() {
        let Ok(caminho) = std::env::var("SKALD_DB_ENSAIO") else { return };
        let conn = crate::db::open(std::path::Path::new(&caminho)).unwrap();
        let t = std::time::Instant::now();
        let s = sugestoes(&conn).unwrap();
        println!("{} grupos parecidos em {:?}: {:?}", s.len(), t.elapsed(), s.iter().take(10).collect::<Vec<_>>());
    }

    #[test]
    fn sugere_grafias_parecidas() {
        let conn = banco();
        assert_eq!(sugestoes(&conn).unwrap(), [vec!["7 Minutoz".to_string(), "7minutoz".to_string()]]);
    }
}
