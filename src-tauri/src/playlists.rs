use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::library::{self, MediaRow};

#[derive(Serialize, Debug)]
pub struct Resumo {
    pub id: i64,
    pub name: String,
    pub total: i64,
    pub duracao_ms: i64,
    /// Primeira música na ordem personalizada: dá a capa automática.
    pub primeira_media_id: Option<i64>,
}

#[derive(Serialize, Debug)]
pub struct Item {
    /// Id do item (não da música): a mesma música pode aparecer mais de uma vez.
    pub item_id: i64,
    #[serde(flatten)]
    pub media: MediaRow,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Ordem {
    Personalizada,
    Titulo,
    Artista,
}

impl Ordem {
    fn sql(self) -> &'static str {
        match self {
            Ordem::Personalizada => "ORDER BY pi.position, pi.id",
            Ordem::Titulo => "ORDER BY m.title IS NULL, m.title COLLATE SEMACENTO, pi.position, pi.id",
            Ordem::Artista => {
                "ORDER BY m.artist IS NULL, m.artist COLLATE SEMACENTO, m.album COLLATE SEMACENTO,
                          m.disc_number, m.track_number, m.title COLLATE SEMACENTO, pi.position, pi.id"
            }
        }
    }
}

fn nome_valido(nome: &str) -> Result<String, String> {
    let nome = nome.trim();
    if nome.is_empty() {
        return Err("O nome da playlist não pode ficar vazio.".into());
    }
    Ok(nome.chars().take(200).collect())
}

fn sql(e: rusqlite::Error) -> String {
    e.to_string()
}

pub fn listar(conn: &Connection) -> Result<Vec<Resumo>, String> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT p.id, p.name, COUNT(pi.id), COALESCE(SUM(m.duration_ms), 0),
                    (SELECT media_id FROM playlist_item WHERE playlist_id = p.id ORDER BY position, id LIMIT 1)
             FROM playlist p
             LEFT JOIN playlist_item pi ON pi.playlist_id = p.id
             LEFT JOIN media m ON m.id = pi.media_id
             GROUP BY p.id
             ORDER BY p.name COLLATE SEMACENTO, p.id",
        )
        .map_err(sql)?;
    let linhas = stmt
        .query_map([], |r| {
            Ok(Resumo {
                id: r.get(0)?,
                name: r.get(1)?,
                total: r.get(2)?,
                duracao_ms: r.get(3)?,
                primeira_media_id: r.get(4)?,
            })
        })
        .map_err(sql)?;
    linhas.collect::<Result<_, _>>().map_err(sql)
}

pub fn criar(conn: &Connection, nome: &str) -> Result<i64, String> {
    conn.execute("INSERT INTO playlist (name) VALUES (?1)", [nome_valido(nome)?]).map_err(sql)?;
    Ok(conn.last_insert_rowid())
}

pub fn renomear(conn: &Connection, id: i64, nome: &str) -> Result<(), String> {
    let n = conn
        .execute(
            "UPDATE playlist SET name = ?2, updated_at = unixepoch() WHERE id = ?1",
            params![id, nome_valido(nome)?],
        )
        .map_err(sql)?;
    if n == 0 { Err("Playlist não encontrada.".into()) } else { Ok(()) }
}

/// Exclui só a playlist e suas referências; as músicas continuam na biblioteca.
pub fn excluir(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM playlist WHERE id = ?1", [id]).map_err(sql)?;
    Ok(())
}

/// Acrescenta músicas ao fim da ordem personalizada.
pub fn adicionar(conn: &mut Connection, id: i64, media_ids: &[i64]) -> Result<(), String> {
    let tx = conn.transaction().map_err(sql)?;
    let existe: Option<i64> = tx
        .query_row("SELECT id FROM playlist WHERE id = ?1", [id], |r| r.get(0))
        .optional()
        .map_err(sql)?;
    if existe.is_none() {
        return Err("Playlist não encontrada.".into());
    }
    let mut proxima: i64 = tx
        .query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_item WHERE playlist_id = ?1", [id], |r| r.get(0))
        .map_err(sql)?;
    {
        let mut ins = tx
            .prepare("INSERT INTO playlist_item (playlist_id, media_id, position) VALUES (?1, ?2, ?3)")
            .map_err(sql)?;
        for &m in media_ids {
            ins.execute(params![id, m, proxima]).map_err(sql)?;
            proxima += 1;
        }
    }
    tx.execute("UPDATE playlist SET updated_at = unixepoch() WHERE id = ?1", [id]).map_err(sql)?;
    tx.commit().map_err(sql)
}

pub fn itens(conn: &Connection, id: i64, ordem: Ordem) -> Result<Vec<Item>, String> {
    let mut stmt = conn
        .prepare_cached(&format!(
            "SELECT m.id, m.title, m.artist, m.album, m.duration_ms, m.missing, pi.id
             FROM playlist_item pi JOIN media m ON m.id = pi.media_id
             WHERE pi.playlist_id = ?1 {}",
            ordem.sql()
        ))
        .map_err(sql)?;
    let linhas = stmt
        .query_map([id], |r| Ok(Item { media: library::linha(r)?, item_id: r.get(6)? }))
        .map_err(sql)?;
    linhas.collect::<Result<_, _>>().map_err(sql)
}

/// Remove a música desta playlist apenas (não da biblioteca nem de outras playlists).
pub fn remover_item(conn: &Connection, item_id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM playlist_item WHERE id = ?1", [item_id]).map_err(sql)?;
    Ok(())
}

/// Move um item para `nova_posicao` (0 = topo) na ordem personalizada e renumera a playlist.
pub fn mover_item(conn: &mut Connection, item_id: i64, nova_posicao: usize) -> Result<(), String> {
    let tx = conn.transaction().map_err(sql)?;
    let playlist: i64 = tx
        .query_row("SELECT playlist_id FROM playlist_item WHERE id = ?1", [item_id], |r| r.get(0))
        .optional()
        .map_err(sql)?
        .ok_or("Item não encontrado na playlist.")?;
    let mut ordem: Vec<i64> = {
        let mut stmt = tx
            .prepare("SELECT id FROM playlist_item WHERE playlist_id = ?1 ORDER BY position, id")
            .map_err(sql)?;
        let ids = stmt.query_map([playlist], |r| r.get(0)).map_err(sql)?;
        ids.collect::<Result<_, _>>().map_err(sql)?
    };
    let atual = ordem.iter().position(|&i| i == item_id).expect("item pertence à playlist");
    ordem.remove(atual);
    ordem.insert(nova_posicao.min(ordem.len()), item_id);
    {
        let mut upd = tx.prepare("UPDATE playlist_item SET position = ?2 WHERE id = ?1").map_err(sql)?;
        for (pos, id) in ordem.iter().enumerate() {
            upd.execute(params![id, pos as i64]).map_err(sql)?;
        }
    }
    tx.commit().map_err(sql)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn banco() -> Connection {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        conn.execute_batch(
            "INSERT INTO media (id, path, title, artist) VALUES
             (1, '/1', 'Zebra', 'Banda'),
             (2, '/2', 'Ágape', 'Zé'),
             (3, '/3', 'Bola', 'Árvore');",
        )
        .unwrap();
        conn
    }

    fn titulos(conn: &Connection, id: i64, ordem: Ordem) -> Vec<String> {
        itens(conn, id, ordem).unwrap().into_iter().map(|i| i.media.title.unwrap()).collect()
    }

    #[test]
    fn cria_adiciona_ordena_e_move() {
        let mut conn = banco();
        let p = criar(&conn, "  HipHop  ").unwrap();
        assert!(criar(&conn, "   ").is_err());
        adicionar(&mut conn, p, &[1, 2, 3, 1]).unwrap();

        assert_eq!(titulos(&conn, p, Ordem::Personalizada), ["Zebra", "Ágape", "Bola", "Zebra"]);
        assert_eq!(titulos(&conn, p, Ordem::Titulo), ["Ágape", "Bola", "Zebra", "Zebra"]);
        assert_eq!(titulos(&conn, p, Ordem::Artista), ["Bola", "Zebra", "Zebra", "Ágape"]);

        let ultimo = itens(&conn, p, Ordem::Personalizada).unwrap()[3].item_id;
        mover_item(&mut conn, ultimo, 0).unwrap();
        let bola = itens(&conn, p, Ordem::Personalizada).unwrap()[3].item_id;
        mover_item(&mut conn, bola, 1).unwrap();
        assert_eq!(titulos(&conn, p, Ordem::Personalizada), ["Zebra", "Bola", "Zebra", "Ágape"]);

        let r = &listar(&conn).unwrap()[0];
        assert_eq!((r.name.as_str(), r.total, r.primeira_media_id), ("HipHop", 4, Some(1)));
    }

    #[test]
    fn remover_e_excluir_nao_tocam_na_biblioteca() {
        let mut conn = banco();
        let a = criar(&conn, "HipHop").unwrap();
        let b = criar(&conn, "Batata Doce").unwrap();
        adicionar(&mut conn, a, &[1, 2]).unwrap();
        adicionar(&mut conn, b, &[1]).unwrap();

        let primeiro = itens(&conn, a, Ordem::Personalizada).unwrap()[0].item_id;
        remover_item(&conn, primeiro).unwrap();
        assert_eq!(titulos(&conn, a, Ordem::Personalizada), ["Ágape"]);
        assert_eq!(titulos(&conn, b, Ordem::Personalizada), ["Zebra"]);
        // Capa automática passa para a nova primeira música.
        let resumo_a = listar(&conn).unwrap().into_iter().find(|r| r.id == a).unwrap();
        assert_eq!(resumo_a.primeira_media_id, Some(2));

        excluir(&conn, a).unwrap();
        let musicas: i64 = conn.query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0)).unwrap();
        assert_eq!(musicas, 3);
        assert_eq!(listar(&conn).unwrap().len(), 1);
        assert_eq!(titulos(&conn, b, Ordem::Personalizada), ["Zebra"]);

        renomear(&conn, b, "Batata").unwrap();
        assert!(renomear(&conn, 999, "x").is_err());
        assert!(adicionar(&mut conn, 999, &[1]).is_err());
    }
}
