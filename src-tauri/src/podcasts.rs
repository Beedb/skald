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
pub(crate) fn dias_desde_1970(ano: i64, mes: i64, dia: i64) -> i64 {
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

// ---------- Telas e reprodução (etapas 55 a 58) ----------

/// Programa na grade de Podcasts.
#[derive(serde::Serialize)]
pub struct Programa {
    pub id: i64,
    pub titulo: String,
    pub autor: Option<String>,
    pub descricao: Option<String>,
    /// Episódio mais novo com áudio, para a capa.
    pub capa_media: Option<i64>,
    pub episodios: i64,
    pub nao_ouvidos: i64,
    pub ultimo_em: Option<i64>,
    /// Assinado por RSS (pode atualizar).
    pub assinado: bool,
    /// Capa baixada do feed (comando `podcast_cover`).
    pub tem_capa: bool,
}

const SELECT_PROGRAMA: &str = "SELECT s.id, s.titulo, s.autor, s.descricao,
        (SELECT e2.media_id FROM podcast_episode e2 WHERE e2.show_id = s.id AND e2.media_id IS NOT NULL
         ORDER BY e2.publicado_em DESC, e2.id DESC LIMIT 1),
        COUNT(e.id), COALESCE(SUM(e.ouvido_em IS NULL), 0), MAX(e.publicado_em),
        s.feed_url IS NOT NULL, s.capa IS NOT NULL
    FROM podcast_show s LEFT JOIN podcast_episode e ON e.show_id = s.id";

fn linha_programa(r: &rusqlite::Row) -> rusqlite::Result<Programa> {
    Ok(Programa {
        id: r.get(0)?,
        titulo: r.get(1)?,
        autor: r.get(2)?,
        descricao: r.get(3)?,
        capa_media: r.get(4)?,
        episodios: r.get(5)?,
        nao_ouvidos: r.get(6)?,
        ultimo_em: r.get(7)?,
        assinado: r.get(8)?,
        tem_capa: r.get(9)?,
    })
}

/// Programas, com episódio mais recente primeiro.
pub fn programas(conn: &Connection) -> rusqlite::Result<Vec<Programa>> {
    let mut st = conn.prepare(&format!(
        "{SELECT_PROGRAMA} GROUP BY s.id ORDER BY MAX(e.publicado_em) DESC, s.titulo COLLATE SEMACENTO"
    ))?;
    let v = st.query_map([], linha_programa)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn programa(conn: &Connection, id: i64) -> rusqlite::Result<Option<Programa>> {
    conn.query_row(&format!("{SELECT_PROGRAMA} WHERE s.id = ?1 GROUP BY s.id"), [id], linha_programa)
        .optional()
}

/// Episódio na tela do programa ou em "Continuar ouvindo".
#[derive(serde::Serialize)]
pub struct Episodio {
    pub id: i64,
    pub show_id: i64,
    pub programa: String,
    pub titulo: String,
    pub publicado_em: Option<i64>,
    pub descricao: Option<String>,
    pub duracao_ms: Option<i64>,
    pub media_id: Option<i64>,
    pub ouvido: bool,
    pub posicao_ms: Option<i64>,
    /// Arquivo não encontrado (registro mantido).
    pub ausente: bool,
    /// Do feed e ainda não baixado.
    pub baixavel: bool,
    pub tamanho: Option<i64>,
}

const SELECT_EPISODIO: &str = "SELECT e.id, e.show_id, s.titulo, e.titulo, e.publicado_em, e.descricao,
        COALESCE(e.duracao_ms, m.duration_ms), e.media_id, e.ouvido_em IS NOT NULL, e.posicao_ms,
        COALESCE(m.missing, 0), e.media_id IS NULL AND e.audio_url IS NOT NULL, e.audio_tamanho
    FROM podcast_episode e JOIN podcast_show s ON s.id = e.show_id LEFT JOIN media m ON m.id = e.media_id";

fn linha_episodio(r: &rusqlite::Row) -> rusqlite::Result<Episodio> {
    Ok(Episodio {
        id: r.get(0)?,
        show_id: r.get(1)?,
        programa: r.get(2)?,
        titulo: r.get(3)?,
        publicado_em: r.get(4)?,
        descricao: r.get(5)?,
        duracao_ms: r.get(6)?,
        media_id: r.get(7)?,
        ouvido: r.get(8)?,
        posicao_ms: r.get(9)?,
        ausente: r.get(10)?,
        baixavel: r.get(11)?,
        tamanho: r.get(12)?,
    })
}

/// Episódios do programa, do mais novo ao mais antigo.
pub fn episodios(conn: &Connection, show_id: i64) -> rusqlite::Result<Vec<Episodio>> {
    let mut st = conn.prepare(&format!(
        "{SELECT_EPISODIO} WHERE e.show_id = ?1 ORDER BY e.publicado_em DESC, e.id DESC"
    ))?;
    let v = st.query_map([show_id], linha_episodio)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// "Continuar ouvindo" (Início): começados e não terminados, tocados mais recentemente primeiro.
pub fn em_andamento(conn: &Connection, limite: i64) -> rusqlite::Result<Vec<Episodio>> {
    let mut st = conn.prepare(&format!(
        "{SELECT_EPISODIO}
         WHERE e.posicao_ms > 0 AND e.ouvido_em IS NULL AND m.missing = 0
         ORDER BY (SELECT MAX(h.started_at) FROM play_history h WHERE h.media_id = e.media_id) DESC
         LIMIT ?1"
    ))?;
    let v = st.query_map([limite], linha_episodio)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

fn agora() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Marca à mão como ouvido ou não ouvido (os dois zeram a posição de retomada).
pub fn marcar(conn: &Connection, episodio: i64, ouvido: bool) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE podcast_episode SET ouvido_em = CASE WHEN ?2 THEN COALESCE(ouvido_em, ?3) END,
         posicao_ms = NULL WHERE id = ?1",
        params![episodio, ouvido, agora()],
    )?;
    Ok(())
}

/// Episódio de um registro de `media`: (id, programa, posição de retomada, ouvido).
pub type DeMedia = (i64, i64, Option<i64>, bool);

pub fn de_media(conn: &Connection, media_id: i64) -> rusqlite::Result<Option<DeMedia>> {
    conn.query_row(
        "SELECT id, show_id, posicao_ms, ouvido_em IS NOT NULL FROM podcast_episode WHERE media_id = ?1",
        [media_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .optional()
}

/// "Ouvido": faltam até 2 min (e já passou da metade) ou chegou a 95%.
pub fn quase_no_fim(posicao_ms: i64, duracao_ms: i64) -> bool {
    duracao_ms > 0
        && posicao_ms * 100 >= duracao_ms * 50
        && (duracao_ms - posicao_ms <= 120_000 || posicao_ms * 100 >= duracao_ms * 95)
}

/// Grava a posição do episódio que está tocando. Devolve `true` se ele acabou de virar ouvido.
pub fn salvar_posicao(conn: &Connection, media_id: i64, posicao_ms: i64, duracao_ms: i64) -> rusqlite::Result<bool> {
    let Some((id, _, _, ouvido)) = de_media(conn, media_id)? else { return Ok(false) };
    conn.execute("UPDATE podcast_episode SET posicao_ms = ?2 WHERE id = ?1", params![id, posicao_ms])?;
    if !ouvido && quase_no_fim(posicao_ms, duracao_ms) {
        conn.execute("UPDATE podcast_episode SET ouvido_em = ?2 WHERE id = ?1", params![id, agora()])?;
        return Ok(true);
    }
    Ok(false)
}

/// O episódio tocou até o fim: ouvido, e a retomada volta ao começo.
pub fn concluir(conn: &Connection, media_id: i64) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE podcast_episode SET ouvido_em = COALESCE(ouvido_em, ?2), posicao_ms = NULL WHERE media_id = ?1",
        params![media_id, agora()],
    )?;
    Ok(n > 0)
}

/// Depois de `media_id`, os não ouvidos do mesmo programa com áudio: primeiro os publicados
/// depois dele (do mais antigo ao mais novo), depois os anteriores.
pub fn proximos(conn: &Connection, media_id: i64) -> rusqlite::Result<Vec<i64>> {
    let mut st = conn.prepare(
        "SELECT e.media_id FROM podcast_episode e
         JOIN podcast_episode atual ON atual.media_id = ?1 AND atual.show_id = e.show_id
         JOIN media m ON m.id = e.media_id
         WHERE e.ouvido_em IS NULL AND m.missing = 0 AND e.media_id <> ?1
         ORDER BY (COALESCE(e.publicado_em, 0) < COALESCE(atual.publicado_em, 0)
                   OR (COALESCE(e.publicado_em, 0) = COALESCE(atual.publicado_em, 0) AND e.id < atual.id)),
                  e.publicado_em, e.id",
    )?;
    let v = st.query_map([media_id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// É um episódio de podcast?
pub fn e_podcast(conn: &Connection, media_id: i64) -> bool {
    conn.query_row("SELECT category = 'podcast' FROM media WHERE id = ?1", [media_id], |r| r.get(0))
        .unwrap_or(false)
}

// ---------- Assinatura por RSS (etapa 59) ----------

/// Cria (ou acha, pelo endereço) o programa do feed e acrescenta os episódios que ainda não
/// tem. Título, autor e descrição vêm do feed. Devolve (programa, episódios novos).
pub fn assinar(conn: &mut Connection, url: &str, feed: &crate::rss::Feed) -> rusqlite::Result<(i64, u32)> {
    let tx = conn.transaction()?;
    let id: i64 = match tx
        .query_row("SELECT id FROM podcast_show WHERE feed_url = ?1", [url], |r| r.get(0))
        .optional()?
    {
        Some(id) => {
            tx.execute(
                "UPDATE podcast_show SET titulo = ?2, autor = COALESCE(?3, autor), descricao = COALESCE(?4, descricao)
                 WHERE id = ?1",
                params![id, feed.titulo, feed.autor, feed.descricao],
            )?;
            id
        }
        None => {
            tx.execute(
                "INSERT INTO podcast_show (titulo, autor, descricao, feed_url, criado_em) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![feed.titulo, feed.autor, feed.descricao, url, agora()],
            )?;
            tx.last_insert_rowid()
        }
    };
    let mut novos = 0;
    {
        let mut ins = tx.prepare(
            "INSERT OR IGNORE INTO podcast_episode
             (show_id, guid, titulo, publicado_em, descricao, duracao_ms, audio_url, audio_tamanho, audio_tipo)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for it in &feed.itens {
            novos += ins.execute(params![
                id, it.guid, it.titulo, it.publicado_em, it.descricao, it.duracao_ms, it.url, it.tamanho, it.tipo
            ])? as u32;
        }
    }
    tx.commit()?;
    Ok((id, novos))
}

/// Programas assinados: (id, endereço do feed).
pub fn assinaturas(conn: &Connection) -> rusqlite::Result<Vec<(i64, String)>> {
    let mut st = conn.prepare("SELECT id, feed_url FROM podcast_show WHERE feed_url IS NOT NULL ORDER BY id")?;
    let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn definir_capa(conn: &Connection, id: i64, arquivo: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE podcast_show SET capa = ?2 WHERE id = ?1", params![id, arquivo])?;
    Ok(())
}

pub fn capa(conn: &Connection, id: i64) -> rusqlite::Result<Option<String>> {
    Ok(conn.query_row("SELECT capa FROM podcast_show WHERE id = ?1", [id], |r| r.get(0)).optional()?.flatten())
}

/// O que é preciso para baixar um episódio.
pub struct ParaBaixar {
    pub programa: String,
    pub titulo: String,
    pub publicado_em: Option<i64>,
    pub url: String,
    pub tipo: Option<String>,
}

pub fn para_baixar(conn: &Connection, episodio: i64) -> rusqlite::Result<Option<ParaBaixar>> {
    conn.query_row(
        "SELECT s.titulo, e.titulo, e.publicado_em, e.audio_url, e.audio_tipo FROM podcast_episode e
         JOIN podcast_show s ON s.id = e.show_id
         WHERE e.id = ?1 AND e.media_id IS NULL AND e.audio_url IS NOT NULL",
        [episodio],
        |r| Ok(ParaBaixar { programa: r.get(0)?, titulo: r.get(1)?, publicado_em: r.get(2)?, url: r.get(3)?, tipo: r.get(4)? }),
    )
    .optional()
}

/// Liga o arquivo baixado (já importado como `media_id`) ao episódio do feed. Se o
/// monitoramento chegou antes e criou um episódio "local" para ele, esse sai.
pub fn ligar(conn: &Connection, episodio: i64, media_id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM podcast_episode WHERE media_id = ?1 AND guid LIKE 'local:%'", [media_id])?;
    conn.execute(
        "UPDATE podcast_episode SET media_id = ?2,
         duracao_ms = COALESCE((SELECT duration_ms FROM media WHERE id = ?2), duracao_ms)
         WHERE id = ?1",
        params![episodio, media_id],
    )?;
    conn.execute(
        "DELETE FROM podcast_show WHERE feed_url IS NULL
         AND NOT EXISTS (SELECT 1 FROM podcast_episode e WHERE e.show_id = podcast_show.id)",
        [],
    )?;
    Ok(())
}

/// Cancela a assinatura: para de atualizar e tira da lista os episódios não baixados.
/// Os baixados (e os arquivos) ficam.
pub fn cancelar_assinatura(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM podcast_episode WHERE show_id = ?1 AND media_id IS NULL AND guid NOT LIKE 'local:%'",
        [id],
    )?;
    conn.execute("UPDATE podcast_show SET feed_url = NULL WHERE id = ?1", [id])?;
    conn.execute(
        "DELETE FROM podcast_show WHERE id = ?1
         AND NOT EXISTS (SELECT 1 FROM podcast_episode e WHERE e.show_id = podcast_show.id)",
        [id],
    )?;
    Ok(())
}

/// Nome do arquivo baixado: "2026-11-17 - Título.mp3" (sem caracteres proibidos).
pub fn nome_arquivo(titulo: &str, publicado_em: Option<i64>, ext: &str) -> String {
    let data = publicado_em.map(|s| {
        let d = s.div_euclid(86_400);
        // Inverso de `dias_desde_1970` (Howard Hinnant).
        let z = d + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let dia = doy - (153 * mp + 2) / 5 + 1;
        let mes = if mp < 10 { mp + 3 } else { mp - 9 };
        let ano = yoe + era * 400 + i64::from(mes <= 2);
        format!("{ano:04}-{mes:02}-{dia:02} - ")
    });
    format!("{}{}.{ext}", data.unwrap_or_default(), nome_seguro(titulo, 120))
}

/// Sem separadores nem caracteres proibidos no Windows/Android, e com tamanho limitado.
pub fn nome_seguro(nome: &str, max: usize) -> String {
    let limpo: String = nome
        .chars()
        .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .take(max)
        .collect();
    let limpo = limpo.trim().trim_matches('.').to_string();
    if limpo.is_empty() { "Episódio".into() } else { limpo }
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

    #[test]
    fn ouvido_retomar_e_proximos() {
        let conn = crate::db::open(Path::new(":memory:")).unwrap();
        conn.execute_batch(
            "INSERT INTO media (id, path, title, duration_ms, category) VALUES
               (1, '/p/1.mp3', 'Um', 3600000, 'podcast'), (2, '/p/2.mp3', 'Dois', 3600000, 'podcast'),
               (3, '/p/3.mp3', 'Três', 3600000, 'podcast'), (4, '/p/4.mp3', 'Quatro', 3600000, 'podcast');
             INSERT INTO podcast_show (id, titulo, pasta, criado_em) VALUES (1, 'P', '/p', 0);
             INSERT INTO podcast_episode (id, show_id, guid, titulo, publicado_em, media_id) VALUES
               (1, 1, 'local:1', 'Um', 100, 1), (2, 1, 'local:2', 'Dois', 200, 2),
               (3, 1, 'local:3', 'Três', 300, 3), (4, 1, 'local:4', 'Quatro', 400, 4);",
        )
        .unwrap();
        // Faltando 2 min de 1 h: ouvido.
        assert!(!salvar_posicao(&conn, 2, 600_000, 3_600_000).unwrap());
        assert_eq!(de_media(&conn, 2).unwrap().unwrap().2, Some(600_000));
        assert!(salvar_posicao(&conn, 2, 3_480_000, 3_600_000).unwrap());
        assert!(de_media(&conn, 2).unwrap().unwrap().3);
        // Episódio curto: começar não marca como ouvido.
        assert!(!quase_no_fim(5_000, 60_000));
        assert!(quase_no_fim(58_000, 60_000));
        // Depois do 3: o 4 (mais novo), depois o 1 (o 2 já foi ouvido).
        assert_eq!(proximos(&conn, 3).unwrap(), [4, 1]);
        marcar(&conn, 4, true).unwrap();
        assert_eq!(proximos(&conn, 3).unwrap(), [1]);
        marcar(&conn, 2, false).unwrap();
        assert_eq!(proximos(&conn, 3).unwrap(), [1, 2]);
        assert!(concluir(&conn, 1).unwrap());
        let (_, _, pos, ouvido) = de_media(&conn, 1).unwrap().unwrap();
        assert_eq!((pos, ouvido), (None, true));
        assert!(!concluir(&conn, 99).unwrap());
        let progs = programas(&conn).unwrap();
        assert_eq!((progs[0].episodios, progs[0].nao_ouvidos, progs[0].capa_media), (4, 2, Some(4)));
        assert_eq!(episodios(&conn, 1).unwrap().iter().map(|e| e.id).collect::<Vec<_>>(), [4, 3, 2, 1]);
    }

    #[test]
    fn assinar_baixar_e_cancelar() {
        let mut conn = crate::db::open(Path::new(":memory:")).unwrap();
        let item = |guid: &str, data: i64| crate::rss::Item {
            guid: guid.into(),
            titulo: format!("Ep {guid}"),
            publicado_em: Some(data),
            url: format!("https://x/{guid}.mp3"),
            ..Default::default()
        };
        let mut feed = crate::rss::Feed { titulo: "Pod".into(), itens: vec![item("a", 100), item("b", 200)], ..Default::default() };
        let (id, novos) = assinar(&mut conn, "https://x/feed", &feed).unwrap();
        assert_eq!(novos, 2);
        feed.itens.push(item("c", 300));
        assert_eq!(assinar(&mut conn, "https://x/feed", &feed).unwrap(), (id, 1), "só o novo entra");
        let eps = episodios(&conn, id).unwrap();
        assert!(eps.iter().all(|e| e.baixavel && !e.ausente));
        let p = programa(&conn, id).unwrap().unwrap();
        assert!(p.assinado && !p.tem_capa);
        // Baixado: o monitoramento chegou antes e criou um episódio local; ligar resolve.
        conn.execute("INSERT INTO media (id, path, title, duration_ms, category) VALUES (9, '/d/b.mp3', 'b', 5000, 'podcast')", []).unwrap();
        let db = Mutex::new(conn);
        assert_eq!(sincronizar(&db).unwrap(), 1, "episódio local provisório");
        let conn = db.into_inner().unwrap();
        let ep_b: i64 = conn.query_row("SELECT id FROM podcast_episode WHERE guid = 'b'", [], |r| r.get(0)).unwrap();
        assert!(para_baixar(&conn, ep_b).unwrap().is_some());
        ligar(&conn, ep_b, 9).unwrap();
        assert!(para_baixar(&conn, ep_b).unwrap().is_none());
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM podcast_show", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "o programa local provisório saiu");
        let b = episodios(&conn, id).unwrap().into_iter().find(|e| e.id == ep_b).unwrap();
        assert_eq!((b.media_id, b.baixavel, b.duracao_ms), (Some(9), false, Some(5000)));
        // Cancelar: só o baixado fica.
        cancelar_assinatura(&conn, id).unwrap();
        let eps = episodios(&conn, id).unwrap();
        assert_eq!(eps.len(), 1);
        assert!(!programa(&conn, id).unwrap().unwrap().assinado);
        assert_eq!(nome_arquivo("A/B: c?", Some(20_774 * 86_400 + 50), "mp3"), "2026-11-17 - A_B_ c_.mp3");
    }
}
