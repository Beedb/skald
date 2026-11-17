//! Etapa 65: playlists, curtidas, histórico e apelidos entre aparelhos.
//!
//! Cada aparelho guarda no Drive (`Skáld/dados/aparelho-<id>.json`) tudo o que conhece; ao
//! sincronizar, lê os arquivos de todos, junta (`mesclar`), aplica no banco (`aplicar`) e
//! regrava o seu. A música é identificada pelo hash do arquivo, nunca pelo id local. O `.db`
//! nunca é copiado inteiro.
//!
//! Regras: playlist inteira vence a alterada mais recentemente (Curtidas é uma playlist com o
//! identificador fixo `curtidas`); exclusão vale se for mais nova que a última alteração;
//! histórico é a união (mesmo início + mesma música = a mesma escuta); apelidos são a união
//! (o do aparelho vence em conflito). Remover música da biblioteca não conta como alteração.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::search::normalizar;

pub const VERSAO: u32 = 1;

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Dados {
    pub versao: u32,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
    #[serde(default)]
    pub removidas: Vec<Removida>,
    #[serde(default)]
    pub historico: Vec<Escuta>,
    #[serde(default)]
    pub apelidos: Vec<Apelido>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Playlist {
    pub uuid: String,
    pub nome: String,
    /// "user" ou "curtidas".
    pub tipo: String,
    /// Segundos Unix da última alteração (nome, músicas, ordem).
    pub atualizada_em: i64,
    #[serde(default)]
    pub tocada_em: Option<i64>,
    pub itens: Vec<Item>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Item {
    pub hash: String,
    pub adicionada_em: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Removida {
    pub uuid: String,
    pub removida_em: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Escuta {
    pub hash: Option<String>,
    pub categoria: String,
    pub titulo: Option<String>,
    pub artista: Option<String>,
    pub album: Option<String>,
    pub genero: Option<String>,
    pub duracao_ms: Option<i64>,
    pub inicio: i64,
    pub ouvido_ms: i64,
    pub completa: bool,
    pub contou: bool,
}

impl Escuta {
    /// Mesma escuta em aparelhos diferentes: mesmo instante e mesma música.
    fn chave(&self) -> (i64, String) {
        let musica = match &self.hash {
            Some(h) => h.clone(),
            None => format!(
                "{}\u{1f}{}",
                normalizar(self.titulo.as_deref().unwrap_or("")),
                normalizar(self.artista.as_deref().unwrap_or(""))
            ),
        };
        (self.inicio, musica)
    }

    /// Junta duas cópias da mesma escuta (uma pode ter sido gravada antes de terminar).
    fn juntar(&mut self, outra: &Escuta) {
        self.ouvido_ms = self.ouvido_ms.max(outra.ouvido_ms);
        self.completa |= outra.completa;
        self.contou |= outra.contou;
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Apelido {
    pub apelido: String,
    pub principal: String,
}

fn sql(e: rusqlite::Error) -> String {
    e.to_string()
}

/// Tudo o que este aparelho sabe, no formato do Drive.
pub fn exportar(conn: &Connection) -> Result<Dados, String> {
    conn.execute("UPDATE playlist SET uuid = lower(hex(randomblob(16))) WHERE uuid IS NULL", [])
        .map_err(sql)?;
    let mut playlists = Vec::new();
    {
        let mut st = conn
            .prepare("SELECT id, uuid, name, kind, updated_at, played_at FROM playlist ORDER BY id")
            .map_err(sql)?;
        let mut itens = conn
            .prepare(
                "SELECT m.file_hash, pi.added_at FROM playlist_item pi JOIN media m ON m.id = pi.media_id
                 WHERE pi.playlist_id = ?1 AND m.file_hash IS NOT NULL ORDER BY pi.position, pi.id",
            )
            .map_err(sql)?;
        let linhas = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    Playlist {
                        uuid: r.get(1)?,
                        nome: r.get(2)?,
                        tipo: r.get(3)?,
                        atualizada_em: r.get(4)?,
                        tocada_em: r.get(5)?,
                        itens: Vec::new(),
                    },
                ))
            })
            .map_err(sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql)?;
        for (id, mut p) in linhas {
            p.itens = itens
                .query_map([id], |r| Ok(Item { hash: r.get(0)?, adicionada_em: r.get(1)? }))
                .map_err(sql)?
                .collect::<Result<_, _>>()
                .map_err(sql)?;
            playlists.push(p);
        }
    }
    let removidas = conn
        .prepare("SELECT uuid, removida_em FROM playlist_removida ORDER BY uuid")
        .map_err(sql)?
        .query_map([], |r| Ok(Removida { uuid: r.get(0)?, removida_em: r.get(1)? }))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    let historico = conn
        .prepare(
            "SELECT m.file_hash, h.category, h.title, h.artist, h.album, h.genre, h.duration_ms,
                    h.started_at, h.listened_ms, h.completed, h.counted
             FROM play_history h LEFT JOIN media m ON m.id = h.media_id ORDER BY h.started_at, h.id",
        )
        .map_err(sql)?
        .query_map([], |r| {
            Ok(Escuta {
                hash: r.get(0)?,
                categoria: r.get(1)?,
                titulo: r.get(2)?,
                artista: r.get(3)?,
                album: r.get(4)?,
                genero: r.get(5)?,
                duracao_ms: r.get(6)?,
                inicio: r.get(7)?,
                ouvido_ms: r.get(8)?,
                completa: r.get(9)?,
                contou: r.get(10)?,
            })
        })
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    let apelidos = conn
        .prepare("SELECT alias, canonical FROM artist_alias ORDER BY alias_norm")
        .map_err(sql)?
        .query_map([], |r| Ok(Apelido { apelido: r.get(0)?, principal: r.get(1)? }))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    Ok(Dados { versao: VERSAO, playlists, removidas, historico, apelidos })
}

/// Junta os dados de vários aparelhos. O primeiro (este aparelho) vence os empates.
pub fn mesclar(todos: &[Dados]) -> Dados {
    let mut removidas: HashMap<&str, i64> = HashMap::new();
    for d in todos {
        for r in &d.removidas {
            let e = removidas.entry(r.uuid.as_str()).or_insert(r.removida_em);
            *e = (*e).max(r.removida_em);
        }
    }

    let mut playlists: Vec<Playlist> = Vec::new();
    let mut indice: HashMap<String, usize> = HashMap::new();
    for d in todos {
        for p in &d.playlists {
            match indice.get(&p.uuid) {
                None => {
                    indice.insert(p.uuid.clone(), playlists.len());
                    playlists.push(p.clone());
                }
                Some(&i) => {
                    let tocada = playlists[i].tocada_em.max(p.tocada_em);
                    if p.atualizada_em > playlists[i].atualizada_em {
                        playlists[i] = p.clone();
                    }
                    playlists[i].tocada_em = tocada;
                }
            }
        }
    }
    // Excluída depois da última alteração: sai (Curtidas nunca sai).
    playlists.retain(|p| {
        p.tipo == "curtidas" || removidas.get(p.uuid.as_str()).is_none_or(|&r| r < p.atualizada_em)
    });

    let mut historico: Vec<Escuta> = Vec::new();
    let mut escutas: HashMap<(i64, String), usize> = HashMap::new();
    for d in todos {
        for e in &d.historico {
            match escutas.get(&e.chave()) {
                Some(&i) => historico[i].juntar(e),
                None => {
                    escutas.insert(e.chave(), historico.len());
                    historico.push(e.clone());
                }
            }
        }
    }
    historico.sort_by_key(|e| e.inicio);

    let mut apelidos: Vec<Apelido> = Vec::new();
    let mut vistos = std::collections::HashSet::new();
    for d in todos {
        for a in &d.apelidos {
            if vistos.insert(normalizar(&a.apelido)) {
                apelidos.push(a.clone());
            }
        }
    }

    let mut removidas: Vec<Removida> =
        removidas.into_iter().map(|(u, r)| Removida { uuid: u.to_string(), removida_em: r }).collect();
    removidas.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    Dados { versao: VERSAO, playlists, removidas, historico, apelidos }
}

#[derive(Serialize, Default, Debug, PartialEq)]
pub struct Mudancas {
    pub playlists: u32,
    pub escutas: u32,
    pub apelidos: u32,
}

/// Aplica no banco o resultado de `mesclar`. Músicas que este aparelho não tem ficam de fora
/// das playlists (a sincronização de músicas roda antes e baixa as que faltam).
pub fn aplicar(conn: &mut Connection, d: &Dados) -> Result<Mudancas, String> {
    let tx = conn.transaction().map_err(sql)?;
    let mut m = Mudancas::default();

    // Hash → música (a presente primeiro).
    let musicas: HashMap<String, i64> = {
        let mut st = tx
            .prepare("SELECT file_hash, id FROM media WHERE file_hash IS NOT NULL ORDER BY missing DESC, id DESC")
            .map_err(sql)?;
        let linhas = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(sql)?;
        linhas.collect::<Result<_, _>>().map_err(sql)?
    };

    for r in &d.removidas {
        tx.execute(
            "INSERT INTO playlist_removida (uuid, removida_em) VALUES (?1, ?2)
             ON CONFLICT(uuid) DO UPDATE SET removida_em = max(removida_em, excluded.removida_em)",
            params![r.uuid, r.removida_em],
        )
        .map_err(sql)?;
        m.playlists += tx
            .execute(
                "DELETE FROM playlist WHERE uuid = ?1 AND kind = 'user' AND updated_at <= ?2",
                params![r.uuid, r.removida_em],
            )
            .map_err(sql)? as u32;
    }

    for p in &d.playlists {
        let local: Option<(i64, i64, Option<i64>)> = tx
            .query_row("SELECT id, updated_at, played_at FROM playlist WHERE uuid = ?1", [&p.uuid], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()
            .map_err(sql)?;
        let id = match local {
            Some((id, atualizada, tocada)) => {
                if p.tocada_em > tocada {
                    tx.execute("UPDATE playlist SET played_at = ?2 WHERE id = ?1", params![id, p.tocada_em])
                        .map_err(sql)?;
                }
                if p.atualizada_em <= atualizada {
                    continue;
                }
                tx.execute(
                    "UPDATE playlist SET name = CASE kind WHEN 'curtidas' THEN name ELSE ?2 END, updated_at = ?3
                     WHERE id = ?1",
                    params![id, p.nome, p.atualizada_em],
                )
                .map_err(sql)?;
                tx.execute("DELETE FROM playlist_item WHERE playlist_id = ?1", [id]).map_err(sql)?;
                id
            }
            None if p.tipo == "curtidas" => continue,
            None => {
                tx.execute(
                    "INSERT INTO playlist (uuid, name, kind, created_at, updated_at, played_at)
                     VALUES (?1, ?2, 'user', ?3, ?3, ?4)",
                    params![p.uuid, p.nome, p.atualizada_em, p.tocada_em],
                )
                .map_err(sql)?;
                tx.last_insert_rowid()
            }
        };
        let mut ins = tx
            .prepare("INSERT INTO playlist_item (playlist_id, media_id, position, added_at) VALUES (?1, ?2, ?3, ?4)")
            .map_err(sql)?;
        for (pos, item) in p.itens.iter().filter_map(|i| musicas.get(&i.hash).map(|&mid| (mid, i))).enumerate() {
            ins.execute(params![id, item.0, pos as i64, item.1.adicionada_em]).map_err(sql)?;
        }
        m.playlists += 1;
    }

    // Histórico: o que falta entra; escutas iguais ficam com o maior tempo ouvido.
    let locais: HashMap<(i64, String), (i64, Escuta)> = {
        let mut st = tx
            .prepare(
                "SELECT h.id, m.file_hash, h.category, h.title, h.artist, h.album, h.genre, h.duration_ms,
                        h.started_at, h.listened_ms, h.completed, h.counted
                 FROM play_history h LEFT JOIN media m ON m.id = h.media_id",
            )
            .map_err(sql)?;
        let linhas = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    Escuta {
                        hash: r.get(1)?,
                        categoria: r.get(2)?,
                        titulo: r.get(3)?,
                        artista: r.get(4)?,
                        album: r.get(5)?,
                        genero: r.get(6)?,
                        duracao_ms: r.get(7)?,
                        inicio: r.get(8)?,
                        ouvido_ms: r.get(9)?,
                        completa: r.get(10)?,
                        contou: r.get(11)?,
                    },
                ))
            })
            .map_err(sql)?;
        linhas
            .map(|l| l.map(|(id, e)| (e.chave(), (id, e))))
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    for e in &d.historico {
        match locais.get(&e.chave()) {
            Some((id, local)) => {
                if e.ouvido_ms > local.ouvido_ms || (e.completa && !local.completa) || (e.contou && !local.contou) {
                    tx.execute(
                        "UPDATE play_history SET listened_ms = max(listened_ms, ?2),
                         completed = max(completed, ?3), counted = max(counted, ?4) WHERE id = ?1",
                        params![id, e.ouvido_ms, e.completa, e.contou],
                    )
                    .map_err(sql)?;
                    m.escutas += 1;
                }
            }
            None => {
                let media_id = e.hash.as_ref().and_then(|h| musicas.get(h));
                tx.execute(
                    "INSERT INTO play_history (media_id, category, title, artist, album, genre, duration_ms,
                                               started_at, listened_ms, completed, counted)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        media_id, e.categoria, e.titulo, e.artista, e.album, e.genero, e.duracao_ms, e.inicio,
                        e.ouvido_ms, e.completa, e.contou
                    ],
                )
                .map_err(sql)?;
                m.escutas += 1;
            }
        }
    }

    for a in &d.apelidos {
        m.apelidos += tx
            .execute(
                "INSERT OR IGNORE INTO artist_alias (alias_norm, alias, canonical) VALUES (?1, ?2, ?3)",
                params![normalizar(&a.apelido), a.apelido, a.principal],
            )
            .map_err(sql)? as u32;
    }

    tx.commit().map_err(sql)?;
    if m.apelidos > 0 {
        crate::artistas::apelidos_mudaram();
    }
    Ok(m)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn banco() -> Connection {
        let pasta = crate::teste_util::pasta(&format!("skald-sincdados-{}", rand_nome()));
        let conn = crate::db::open(&pasta.join("t.db")).unwrap();
        conn.execute_batch(
            "INSERT INTO media (id, path, title, artist, file_hash) VALUES
               (1, '/a.mp3', 'A', 'X', 'ha'), (2, '/b.mp3', 'B', 'Y', 'hb'), (3, '/c.mp3', 'C', 'Z', 'hc');",
        )
        .unwrap();
        conn
    }

    fn rand_nome() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::SeqCst)
    }

    fn playlist(conn: &Connection, nome: &str, ids: &[i64], quando: i64) -> String {
        let mut c = crate::db::open(conn.path().unwrap().as_ref()).unwrap();
        let id = crate::playlists::criar(&c, nome).unwrap();
        crate::playlists::adicionar(&mut c, id, ids).unwrap();
        c.execute("UPDATE playlist SET updated_at = ?2 WHERE id = ?1", params![id, quando]).unwrap();
        c.query_row("SELECT uuid FROM playlist WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    fn sincronizar(a: &mut Connection, b: &mut Connection) {
        let (da, db) = (exportar(a).unwrap(), exportar(b).unwrap());
        aplicar(a, &mesclar(&[da.clone(), db.clone()])).unwrap();
        aplicar(b, &mesclar(&[db, da])).unwrap();
    }

    fn itens(conn: &Connection, uuid: &str) -> Vec<String> {
        conn.prepare(
            "SELECT m.file_hash FROM playlist_item pi JOIN media m ON m.id = pi.media_id
             JOIN playlist p ON p.id = pi.playlist_id WHERE p.uuid = ?1 ORDER BY pi.position",
        )
        .unwrap()
        .query_map([uuid], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    }

    #[test]
    fn playlist_nova_e_alteracao_mais_recente() {
        let (mut a, mut b) = (banco(), banco());
        let u = playlist(&a, "Viagem", &[1, 2], 100);
        sincronizar(&mut a, &mut b);
        assert_eq!(itens(&b, &u), ["ha", "hb"]);
        // Alterada no B depois: vence no A.
        b.execute("UPDATE playlist SET name = 'Viagem 2', updated_at = 200 WHERE uuid = ?1", [&u]).unwrap();
        b.execute(
            "INSERT INTO playlist_item (playlist_id, media_id, position) SELECT id, 3, 9 FROM playlist WHERE uuid = ?1",
            [&u],
        )
        .unwrap();
        sincronizar(&mut a, &mut b);
        assert_eq!(itens(&a, &u), ["ha", "hb", "hc"]);
        let nome: String = a.query_row("SELECT name FROM playlist WHERE uuid = ?1", [&u], |r| r.get(0)).unwrap();
        assert_eq!(nome, "Viagem 2");
        // Sincronizar de novo não muda nada.
        let (da, db) = (exportar(&a).unwrap(), exportar(&b).unwrap());
        assert_eq!(aplicar(&mut a, &mesclar(&[da, db])).unwrap(), Mudancas::default());
    }

    #[test]
    fn curtidas_e_exclusao() {
        let (mut a, mut b) = (banco(), banco());
        crate::playlists::alternar_curtida(&mut a, 2).unwrap();
        a.execute("UPDATE playlist SET updated_at = unixepoch() + 5 WHERE kind = 'curtidas'", []).unwrap();
        let u = playlist(&a, "Some", &[1], 100);
        sincronizar(&mut a, &mut b);
        assert_eq!(itens(&b, "curtidas"), ["hb"]);
        let id: i64 = b.query_row("SELECT id FROM playlist WHERE uuid = ?1", [&u], |r| r.get(0)).unwrap();
        crate::playlists::excluir(&b, id).unwrap();
        sincronizar(&mut a, &mut b);
        let n: i64 = a.query_row("SELECT count(*) FROM playlist WHERE uuid = ?1", [&u], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn historico_e_apelidos_sao_uniao() {
        let (mut a, mut b) = (banco(), banco());
        let ha = crate::history::iniciar(&a, 1, 1000).unwrap();
        a.execute("UPDATE play_history SET listened_ms = 5000 WHERE id = ?1", [ha]).unwrap();
        crate::history::iniciar(&b, 2, 2000).unwrap();
        crate::apelidos::adicionar(&a, "Beatles", "The Beatles").unwrap();
        sincronizar(&mut a, &mut b);
        for c in [&a, &b] {
            let n: i64 = c.query_row("SELECT count(*) FROM play_history", [], |r| r.get(0)).unwrap();
            assert_eq!(n, 2);
            let apelido: String =
                c.query_row("SELECT canonical FROM artist_alias", [], |r| r.get(0)).unwrap();
            assert_eq!(apelido, "The Beatles");
        }
        // A escuta continuou no A: o B fica com o tempo maior.
        a.execute("UPDATE play_history SET listened_ms = 9000 WHERE id = ?1", [ha]).unwrap();
        sincronizar(&mut a, &mut b);
        let ouvido: i64 =
            b.query_row("SELECT listened_ms FROM play_history WHERE started_at = 1000", [], |r| r.get(0)).unwrap();
        assert_eq!(ouvido, 9000);
    }
}
