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
    /// `user` ou `curtidas`.
    pub kind: String,
    /// Capa escolhida pelo usuário (senão, a automática).
    pub capa_escolhida: bool,
    /// Última vez que foi tocada (segundos Unix), se já foi.
    pub tocada_em: Option<i64>,
}

#[derive(Serialize, Debug)]
pub struct Item {
    /// Id do item (não da música): a mesma música pode aparecer mais de uma vez.
    pub item_id: i64,
    #[serde(flatten)]
    pub media: MediaRow,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
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

/// Registra que a playlist começou a tocar agora (tela inicial).
pub fn marcar_tocada(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("UPDATE playlist SET played_at = unixepoch() WHERE id = ?1", [id]).map_err(sql)?;
    Ok(())
}

pub fn listar(conn: &Connection) -> Result<Vec<Resumo>, String> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT p.id, p.name, COUNT(pi.id), COALESCE(SUM(m.duration_ms), 0),
                    (SELECT media_id FROM playlist_item WHERE playlist_id = p.id ORDER BY position, id LIMIT 1),
                    p.kind, p.cover_path IS NOT NULL, p.played_at
             FROM playlist p
             LEFT JOIN playlist_item pi ON pi.playlist_id = p.id
             LEFT JOIN media m ON m.id = pi.media_id
             GROUP BY p.id
             ORDER BY p.kind = 'curtidas' DESC, p.name COLLATE SEMACENTO, p.id",
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
                kind: r.get(5)?,
                capa_escolhida: r.get(6)?,
                tocada_em: r.get(7)?,
            })
        })
        .map_err(sql)?;
    linhas.collect::<Result<_, _>>().map_err(sql)
}

pub fn criar(conn: &Connection, nome: &str) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO playlist (name, uuid) VALUES (?1, lower(hex(randomblob(16))))",
        [nome_valido(nome)?],
    )
    .map_err(sql)?;
    Ok(conn.last_insert_rowid())
}

/// Recusa alterações proibidas na playlist fixa "Curtidas".
fn exigir_comum(conn: &Connection, id: i64, acao: &str) -> Result<(), String> {
    let kind: Option<String> = conn
        .query_row("SELECT kind FROM playlist WHERE id = ?1", [id], |r| r.get(0))
        .optional()
        .map_err(sql)?;
    match kind.as_deref() {
        None => Err("Playlist não encontrada.".into()),
        Some("curtidas") => Err(format!("A playlist Curtidas não pode ser {acao}.")),
        _ => Ok(()),
    }
}

pub fn renomear(conn: &Connection, id: i64, nome: &str) -> Result<(), String> {
    exigir_comum(conn, id, "renomeada")?;
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
    exigir_comum(conn, id, "excluída")?;
    // A marca faz a exclusão valer nos outros aparelhos (sincronização).
    conn.execute(
        "INSERT INTO playlist_removida (uuid, removida_em)
         SELECT uuid, unixepoch() FROM playlist WHERE id = ?1 AND uuid IS NOT NULL
         ON CONFLICT(uuid) DO UPDATE SET removida_em = excluded.removida_em",
        [id],
    )
    .map_err(sql)?;
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
    conn.execute(
        "UPDATE playlist SET updated_at = unixepoch()
         WHERE id = (SELECT playlist_id FROM playlist_item WHERE id = ?1)",
        [item_id],
    )
    .map_err(sql)?;
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
    tx.execute("UPDATE playlist SET updated_at = unixepoch() WHERE id = ?1", [playlist]).map_err(sql)?;
    tx.commit().map_err(sql)
}

pub fn curtidas_id(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT id FROM playlist WHERE kind = 'curtidas'", [], |r| r.get(0)).map_err(sql)
}

/// Curte ou descurte. Devolve se a música ficou curtida.
pub fn alternar_curtida(conn: &mut Connection, media_id: i64) -> Result<bool, String> {
    let id = curtidas_id(conn)?;
    let removidas = conn
        .execute("DELETE FROM playlist_item WHERE playlist_id = ?1 AND media_id = ?2", params![id, media_id])
        .map_err(sql)?;
    if removidas > 0 {
        conn.execute("UPDATE playlist SET updated_at = unixepoch() WHERE id = ?1", [id]).map_err(sql)?;
        return Ok(false);
    }
    adicionar(conn, id, &[media_id])?;
    Ok(true)
}

pub fn ids_curtidos(conn: &Connection) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT DISTINCT pi.media_id FROM playlist_item pi JOIN playlist p ON p.id = pi.playlist_id
             WHERE p.kind = 'curtidas'",
        )
        .map_err(sql)?;
    let ids = stmt.query_map([], |r| r.get(0)).map_err(sql)?;
    ids.collect::<Result<_, _>>().map_err(sql)
}

pub fn capa(conn: &Connection, id: i64) -> Result<Option<String>, String> {
    conn.query_row("SELECT cover_path FROM playlist WHERE id = ?1", [id], |r| r.get(0))
        .optional()
        .map(Option::flatten)
        .map_err(sql)
}

/// Define (ou limpa, com `None`) a capa escolhida. Devolve o caminho anterior.
pub fn definir_capa(conn: &Connection, id: i64, caminho: Option<&str>) -> Result<Option<String>, String> {
    let anterior = capa(conn, id)?;
    let n = conn
        .execute("UPDATE playlist SET cover_path = ?2, updated_at = unixepoch() WHERE id = ?1", params![id, caminho])
        .map_err(sql)?;
    if n == 0 { Err("Playlist não encontrada.".into()) } else { Ok(anterior) }
}

/// Conteúdo M3U (estendido) da playlist na ordem personalizada, com caminhos absolutos.
pub fn m3u(conn: &Connection, id: i64) -> Result<(String, usize), String> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT m.path, m.title, m.artist, m.duration_ms
             FROM playlist_item pi JOIN media m ON m.id = pi.media_id
             WHERE pi.playlist_id = ?1 ORDER BY pi.position, pi.id",
        )
        .map_err(sql)?;
    let linhas = stmt
        .query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<i64>>(3)?))
        })
        .map_err(sql)?;
    let mut texto = String::from("#EXTM3U\n");
    let mut n = 0;
    for linha in linhas {
        let (path, titulo, artista, duracao) = linha.map_err(sql)?;
        let segundos = duracao.map_or(-1, |ms| (ms + 500) / 1000);
        let titulo = titulo.unwrap_or_else(|| {
            std::path::Path::new(&path).file_stem().map_or(path.clone(), |s| s.to_string_lossy().into_owned())
        });
        let nome = match artista {
            Some(a) => format!("{a} - {titulo}"),
            None => titulo,
        };
        // Quebras de linha nas tags quebrariam o formato.
        let nome = nome.replace(['\n', '\r'], " ");
        texto.push_str(&format!("#EXTINF:{segundos},{nome}\n{path}\n"));
        n += 1;
    }
    Ok((texto, n))
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

        let r = listar(&conn).unwrap().into_iter().find(|r| r.id == p).unwrap();
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
        assert_eq!(listar(&conn).unwrap().len(), 2); // Batata Doce + Curtidas
        assert_eq!(titulos(&conn, b, Ordem::Personalizada), ["Zebra"]);

        renomear(&conn, b, "Batata").unwrap();
        assert!(renomear(&conn, 999, "x").is_err());
        assert!(adicionar(&mut conn, 999, &[1]).is_err());
    }

    #[test]
    fn curtidas_fixa() {
        let mut conn = banco();
        let c = curtidas_id(&conn).unwrap();
        assert_eq!(listar(&conn).unwrap()[0].kind, "curtidas");
        assert!(renomear(&conn, c, "Outra").is_err());
        assert!(excluir(&conn, c).is_err());

        assert!(alternar_curtida(&mut conn, 2).unwrap());
        assert!(alternar_curtida(&mut conn, 3).unwrap());
        assert_eq!(titulos(&conn, c, Ordem::Personalizada), ["Ágape", "Bola"]);
        assert!(!alternar_curtida(&mut conn, 2).unwrap());
        assert_eq!(ids_curtidos(&conn).unwrap(), [3]);
    }

    #[test]
    fn capa_escolhida_nao_muda_ao_remover_a_primeira() {
        let mut conn = banco();
        let p = criar(&conn, "Com capa").unwrap();
        adicionar(&mut conn, p, &[1, 2]).unwrap();
        assert_eq!(definir_capa(&conn, p, Some("/capas/x.png")).unwrap(), None);
        let primeiro = itens(&conn, p, Ordem::Personalizada).unwrap()[0].item_id;
        remover_item(&conn, primeiro).unwrap();
        let r = listar(&conn).unwrap().into_iter().find(|r| r.id == p).unwrap();
        assert!(r.capa_escolhida);
        assert_eq!(capa(&conn, p).unwrap().as_deref(), Some("/capas/x.png"));
        assert_eq!(definir_capa(&conn, p, None).unwrap().as_deref(), Some("/capas/x.png"));
        assert!(!listar(&conn).unwrap().into_iter().find(|r| r.id == p).unwrap().capa_escolhida);
    }

    #[test]
    fn exporta_m3u() {
        let mut conn = banco();
        conn.execute("UPDATE media SET duration_ms = 61400 WHERE id = 1", []).unwrap();
        conn.execute("INSERT INTO media (id, path) VALUES (4, '/musicas/sem tags.flac')", []).unwrap();
        let p = criar(&conn, "Exportar").unwrap();
        adicionar(&mut conn, p, &[1, 4]).unwrap();
        let (texto, n) = m3u(&conn, p).unwrap();
        assert_eq!(n, 2);
        assert_eq!(texto, "#EXTM3U\n#EXTINF:61,Banda - Zebra\n/1\n#EXTINF:-1,sem tags\n/musicas/sem tags.flac\n");
    }
}
