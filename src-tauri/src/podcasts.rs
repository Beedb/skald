//! Podcasts (Fase 10). Etapa 54: programas e episódios a partir de arquivos locais.
//!
//! Um arquivo de uma pasta importada como Podcasts vira uma linha de `media` com
//! `category = 'podcast'` (feito em `import::gravar`); `sincronizar` cria o episódio
//! correspondente e o programa (um por pasta: título do álbum ou o nome da pasta).

use std::path::Path;
use std::sync::Mutex;

use lofty::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};

/// Áudio de podcast ainda sem episódio: id, caminho, título, álbum, autor, duração e mtime.
type Pendente = (i64, String, Option<String>, Option<String>, Option<String>, Option<i64>, Option<i64>);

/// Episódio novo lido do disco antes de gravar.
struct Novo {
    media_id: i64,
    pasta: String,
    nome_pasta: String,
    titulo: String,
    album: Option<String>,
    autor: Option<String>,
    duracao_ms: Option<i64>,
    publicado_em: Option<i64>,
    descricao: Option<String>,
}

/// Cria episódios (e programas) para os áudios de podcast que ainda não têm, e apaga
/// episódios locais cujo registro saiu da biblioteca e programas locais vazios.
/// As tags são lidas sem travar o banco. Devolve quantos episódios foram criados.
pub fn sincronizar(db: &Mutex<Connection>) -> Result<u32, String> {
    let pendentes: Vec<Pendente> = {
        let conn = db.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
        let mut st = conn
            .prepare(
                "SELECT m.id, m.path, m.title, m.album, COALESCE(m.album_artist, m.artist), m.duration_ms, m.file_mtime
                 FROM media m
                 WHERE m.category = 'podcast'
                   AND NOT EXISTS (SELECT 1 FROM podcast_episode e WHERE e.media_id = m.id)",
            )
            .map_err(|e| e.to_string())?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let novos: Vec<Novo> = pendentes
        .into_iter()
        .map(|(media_id, caminho, titulo, album, autor, duracao_ms, mtime)| {
            let p = Path::new(&caminho);
            let pasta = p.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
            let nome_pasta = p
                .parent()
                .and_then(|d| d.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Podcast".into());
            let (data, descricao) = ler_extras(p);
            Novo {
                media_id,
                pasta,
                nome_pasta,
                titulo: titulo
                    .or_else(|| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "Episódio".into()),
                album,
                autor,
                duracao_ms,
                publicado_em: data.or(mtime),
                descricao,
            }
        })
        .collect();
    let mut conn = db.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
    gravar(&mut conn, &novos).map_err(|e| e.to_string())
}

fn gravar(conn: &mut Connection, novos: &[Novo]) -> rusqlite::Result<u32> {
    let tx = conn.transaction()?;
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut criados = 0;
    {
        let mut achar = tx.prepare("SELECT id FROM podcast_show WHERE pasta = ?1")?;
        let mut criar = tx.prepare(
            "INSERT INTO podcast_show (titulo, autor, pasta, criado_em) VALUES (?1, ?2, ?3, ?4)",
        )?;
        let mut completar_autor = tx.prepare("UPDATE podcast_show SET autor = ?2 WHERE id = ?1 AND autor IS NULL")?;
        let mut episodio = tx.prepare(
            "INSERT OR IGNORE INTO podcast_episode
             (show_id, guid, titulo, publicado_em, descricao, duracao_ms, media_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for n in novos {
            let show: i64 = match achar.query_row([&n.pasta], |r| r.get(0)).optional()? {
                Some(id) => {
                    if let Some(a) = &n.autor {
                        completar_autor.execute(params![id, a])?;
                    }
                    id
                }
                None => {
                    let titulo = n.album.clone().unwrap_or_else(|| n.nome_pasta.clone());
                    criar.execute(params![titulo, n.autor, n.pasta, agora])?;
                    tx.last_insert_rowid()
                }
            };
            criados += episodio.execute(params![
                show,
                format!("local:{}", n.media_id),
                n.titulo,
                n.publicado_em,
                n.descricao,
                n.duracao_ms,
                n.media_id
            ])? as u32;
        }
    }
    // Registro removido da biblioteca: o episódio local some junto (o de RSS fica, sem áudio).
    tx.execute("DELETE FROM podcast_episode WHERE media_id IS NULL AND guid LIKE 'local:%'", [])?;
    tx.execute(
        "DELETE FROM podcast_show WHERE feed_url IS NULL
         AND NOT EXISTS (SELECT 1 FROM podcast_episode e WHERE e.show_id = podcast_show.id)",
        [],
    )?;
    tx.commit()?;
    Ok(criados)
}

/// Data de publicação (segundos Unix) e descrição tiradas das tags, se houver.
fn ler_extras(p: &Path) -> (Option<i64>, Option<String>) {
    let Ok(arquivo) = lofty::read_from_path(p) else { return (None, None) };
    let Some(tag) = arquivo.primary_tag().or_else(|| arquivo.first_tag()) else { return (None, None) };
    let data = tag.date().and_then(|d| {
        let mes = d.month.unwrap_or(1);
        let dia = d.day.unwrap_or(1);
        let s = d.hour.unwrap_or(0) as i64 * 3600 + d.minute.unwrap_or(0) as i64 * 60 + d.second.unwrap_or(0) as i64;
        (d.year > 0).then(|| dias_desde_1970(d.year as i64, mes as i64, dia as i64) * 86_400 + s)
    });
    let descricao = [ItemKey::PodcastDescription, ItemKey::Description, ItemKey::Comment]
        .into_iter()
        .find_map(|k| tag.get_string(k).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
    (data, descricao)
}

/// Dias entre 1970-01-01 e a data (calendário gregoriano; algoritmo de Howard Hinnant).
fn dias_desde_1970(ano: i64, mes: i64, dia: i64) -> i64 {
    let a = if mes <= 2 { ano - 1 } else { ano };
    let era = a.div_euclid(400);
    let ano_era = a - era * 400;
    let m = (mes + 9) % 12;
    let dia_ano = (153 * m + 2) / 5 + dia - 1;
    let dia_era = ano_era * 365 + ano_era / 4 - ano_era / 100 + dia_ano;
    era * 146_097 + dia_era - 719_468
}

/// Passa a pasta para a categoria Podcasts: ela (e o que já estava importado dentro dela)
/// deixa de ser música. Nada é apagado nem movido.
pub fn marcar_pasta(conn: &Connection, pasta: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO watched_folder (path, category) VALUES (?1, 'podcast')
         ON CONFLICT(path) DO UPDATE SET category = 'podcast'",
        [pasta],
    )?;
    conn.execute(
        &format!(
            "UPDATE media SET category = 'podcast'
             WHERE substr(path, 1, length(?1) + 1) = ?1 || {sep}",
            sep = crate::library::SEP_SQL
        ),
        [pasta],
    )?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas() {
        assert_eq!(dias_desde_1970(1970, 1, 1), 0);
        assert_eq!(dias_desde_1970(2000, 3, 1), 11_017);
        assert_eq!(dias_desde_1970(2026, 11, 17), 20_774);
    }

    #[test]
    fn programas_e_episodios_por_pasta() {
        let nativo = crate::teste_util::nativo;
        let db = Mutex::new(crate::db::open(Path::new(":memory:")).unwrap());
        {
            let conn = db.lock().unwrap();
            conn.execute_batch(&format!(
                "INSERT INTO media (id, path, title, album, artist, duration_ms, file_mtime, category) VALUES
                 (1, '{a}', 'Ep 1', 'Meu Programa', 'Fulana', 60000, 100, 'music'),
                 (2, '{b}', 'Ep 2', NULL, NULL, 70000, 200, 'music'),
                 (3, '{c}', NULL, NULL, 'Beltrano', 80000, 300, 'music'),
                 (4, '{d}', 'Canção', 'Álbum', 'Banda', 90000, 400, 'music');",
                a = nativo("/pod/um/a.mp3"),
                b = nativo("/pod/um/b.mp3"),
                c = nativo("/pod/Outro Programa/c.mp3"),
                d = nativo("/musica/d.mp3"),
            ))
            .unwrap();
            marcar_pasta(&conn, &nativo("/pod")).unwrap();
        }
        assert_eq!(sincronizar(&db).unwrap(), 3);
        assert_eq!(sincronizar(&db).unwrap(), 0, "não repete");
        let conn = db.lock().unwrap();
        let shows: Vec<(String, Option<String>, i64)> = conn
            .prepare(
                "SELECT s.titulo, s.autor, COUNT(e.id) FROM podcast_show s
                 JOIN podcast_episode e ON e.show_id = s.id GROUP BY s.id ORDER BY s.titulo",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            shows,
            [("Meu Programa".into(), Some("Fulana".into()), 2), ("Outro Programa".into(), Some("Beltrano".into()), 1)]
        );
        // Sem título: nome do arquivo; sem data nas tags: data do arquivo.
        let (titulo, data): (String, i64) = conn
            .query_row("SELECT titulo, publicado_em FROM podcast_episode WHERE media_id = 3", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((titulo.as_str(), data), ("c", 300));
        let musica: String = conn.query_row("SELECT category FROM media WHERE id = 4", [], |r| r.get(0)).unwrap();
        assert_eq!(musica, "music");
        drop(conn);
        // Removido da biblioteca: episódio e programa vazio saem.
        crate::library::excluir(&mut db.lock().unwrap(), &[3]).unwrap();
        sincronizar(&db).unwrap();
        let conn = db.lock().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM podcast_show", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }
}
