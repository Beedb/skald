//! Audiolivros: uma pasta marcada como Audiolivros vira a categoria `audiobook`; cada pasta
//! com arquivos dentro dela é um livro (um `.m4b` só ou um arquivo por capítulo). O livro
//! guarda onde parou (capítulo e posição). Na reprodução vale o mesmo que nos podcasts:
//! velocidade sem mudar o tom, −15/+30 s, sem aleatório, crossfade nem emenda; ao fim de um
//! capítulo vem o seguinte, sempre em ordem. Fora de estatísticas, mixes e retrospectivas.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

/// Falta tão pouco no último capítulo que o livro conta como terminado.
const FIM_MS: i64 = 30_000;
/// Abaixo disso não vale retomar (é praticamente o começo).
const RETOMAR_MIN_MS: i64 = 5_000;

pub fn marcar_pasta(conn: &Connection, pasta: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO watched_folder (path, category) VALUES (?1, 'audiobook')
         ON CONFLICT(path) DO UPDATE SET category = 'audiobook'",
        [pasta],
    )?;
    conn.execute(
        &format!(
            "UPDATE media SET category = 'audiobook'
             WHERE substr(path, 1, length(?1) + 1) = ?1 || {sep}",
            sep = crate::library::SEP_SQL
        ),
        [pasta],
    )?;
    Ok(())
}

fn pasta_de(caminho: &str) -> String {
    Path::new(caminho).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Compara nomes com números pelo valor ("Capítulo 2" antes de "Capítulo 10").
fn natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n.push(c);
                    x.next();
                }
                let mut m = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    m.push(d);
                    y.next();
                }
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                match n.len().cmp(&m.len()).then_with(|| n.cmp(m)) {
                    Ordering::Equal => {}
                    o => return o,
                }
            }
            (Some(c), Some(d)) => {
                let (c2, d2) = (c.to_lowercase().next(), d.to_lowercase().next());
                match c2.cmp(&d2) {
                    Ordering::Equal => {
                        x.next();
                        y.next();
                    }
                    o => return o,
                }
            }
        }
    }
}

struct Arquivo {
    id: i64,
    caminho: String,
    titulo: Option<String>,
    album: Option<String>,
    autor: Option<String>,
    disco: Option<i64>,
    faixa: Option<i64>,
    duracao_ms: i64,
}

fn arquivos(conn: &Connection) -> rusqlite::Result<Vec<Arquivo>> {
    let mut st = conn.prepare_cached(
        "SELECT id, path, title, album, COALESCE(album_artist, artist), disc_number, track_number, COALESCE(duration_ms, 0)
         FROM media WHERE category = 'audiobook'",
    )?;
    let linhas = st.query_map([], |r| {
        Ok(Arquivo {
            id: r.get(0)?,
            caminho: r.get(1)?,
            titulo: r.get(2)?,
            album: r.get(3)?,
            autor: r.get(4)?,
            disco: r.get(5)?,
            faixa: r.get(6)?,
            duracao_ms: r.get(7)?,
        })
    })?;
    linhas.collect()
}

/// Arquivos agrupados por pasta, cada grupo na ordem dos capítulos.
fn por_pasta(conn: &Connection) -> rusqlite::Result<HashMap<String, Vec<Arquivo>>> {
    let mut grupos: HashMap<String, Vec<Arquivo>> = HashMap::new();
    for a in arquivos(conn)? {
        grupos.entry(pasta_de(&a.caminho)).or_default().push(a);
    }
    for v in grupos.values_mut() {
        v.sort_by(|a, b| {
            a.disco
                .unwrap_or(0)
                .cmp(&b.disco.unwrap_or(0))
                .then_with(|| a.faixa.unwrap_or(0).cmp(&b.faixa.unwrap_or(0)))
                .then_with(|| natural(&a.caminho, &b.caminho))
        });
    }
    Ok(grupos)
}

/// Cria um livro para cada pasta nova e apaga os livros sem arquivos (o progresso de um livro
/// que continua existindo nunca é mexido).
pub fn sincronizar(db: &Mutex<Connection>) -> Result<(), String> {
    let conn = db.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
    let grupos = por_pasta(&conn).map_err(|e| e.to_string())?;
    for (pasta, caps) in &grupos {
        let primeiro = &caps[0];
        let titulo = primeiro
            .album
            .clone()
            .filter(|_| caps.iter().all(|c| c.album == primeiro.album))
            .or_else(|| (caps.len() == 1).then(|| primeiro.titulo.clone()).flatten())
            .unwrap_or_else(|| Path::new(pasta).file_name().map_or("Audiolivro".into(), |n| n.to_string_lossy().into_owned()));
        conn.execute(
            "INSERT INTO audiolivro (pasta, titulo, autor) VALUES (?1, ?2, ?3) ON CONFLICT(pasta) DO NOTHING",
            params![pasta, titulo, primeiro.autor],
        )
        .map_err(|e| e.to_string())?;
    }
    let existentes: Vec<(i64, String)> = {
        let mut st = conn.prepare("SELECT id, pasta FROM audiolivro").map_err(|e| e.to_string())?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    for (id, pasta) in existentes {
        if !grupos.contains_key(&pasta) {
            conn.execute("DELETE FROM audiolivro WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[derive(Serialize, Debug)]
pub struct Livro {
    pub id: i64,
    pub titulo: String,
    pub autor: Option<String>,
    pub capitulos: usize,
    pub duracao_ms: i64,
    /// Quanto já foi ouvido (capítulos anteriores ao atual + posição no atual).
    pub ouvido_ms: i64,
    /// Capítulo onde parou (índice a partir de 0).
    pub capitulo_atual: usize,
    /// Um arquivo do livro, para a capa.
    pub capa_media: Option<i64>,
    pub concluido: bool,
    /// Última vez que foi ouvido (segundos Unix).
    pub ouvido_em: Option<i64>,
}

#[derive(Serialize, Debug)]
pub struct Capitulo {
    pub media_id: i64,
    pub titulo: String,
    pub duracao_ms: i64,
    pub missing: bool,
}

struct Progresso {
    id: i64,
    titulo: String,
    autor: Option<String>,
    pasta: String,
    capitulo: Option<i64>,
    posicao: i64,
    ouvido_em: Option<i64>,
    concluido: bool,
}

fn progressos(conn: &Connection, filtro: &str, p: &[&dyn rusqlite::ToSql]) -> rusqlite::Result<Vec<Progresso>> {
    let mut st = conn.prepare(&format!(
        "SELECT id, titulo, autor, pasta, capitulo_media_id, posicao_ms, ouvido_em, concluido_em IS NOT NULL
         FROM audiolivro {filtro}"
    ))?;
    let linhas = st.query_map(p, |r| {
        Ok(Progresso {
            id: r.get(0)?,
            titulo: r.get(1)?,
            autor: r.get(2)?,
            pasta: r.get(3)?,
            capitulo: r.get(4)?,
            posicao: r.get(5)?,
            ouvido_em: r.get(6)?,
            concluido: r.get(7)?,
        })
    })?;
    linhas.collect()
}

fn montar(p: &Progresso, caps: &[Arquivo]) -> Livro {
    let duracao_ms = caps.iter().map(|c| c.duracao_ms).sum();
    let atual = p.capitulo.and_then(|m| caps.iter().position(|c| c.id == m)).unwrap_or(0);
    let ouvido_ms = if p.concluido {
        duracao_ms
    } else {
        caps[..atual].iter().map(|c| c.duracao_ms).sum::<i64>() + p.posicao
    };
    Livro {
        id: p.id,
        titulo: p.titulo.clone(),
        autor: p.autor.clone(),
        capitulos: caps.len(),
        duracao_ms,
        ouvido_ms,
        capitulo_atual: atual,
        capa_media: caps.first().map(|c| c.id),
        concluido: p.concluido,
        ouvido_em: p.ouvido_em,
    }
}

/// Todos os livros: os ouvidos por último primeiro, depois os nunca abertos por título.
pub fn livros(conn: &Connection) -> rusqlite::Result<Vec<Livro>> {
    let grupos = por_pasta(conn)?;
    let mut lista: Vec<Livro> = progressos(conn, "", &[])?
        .iter()
        .filter_map(|p| grupos.get(&p.pasta).map(|caps| montar(p, caps)))
        .collect();
    lista.sort_by(|a, b| {
        b.ouvido_em
            .cmp(&a.ouvido_em)
            .then_with(|| crate::search::normalizar(&a.titulo).cmp(&crate::search::normalizar(&b.titulo)))
    });
    Ok(lista)
}

pub fn livro(conn: &Connection, id: i64) -> rusqlite::Result<Option<Livro>> {
    let grupos = por_pasta(conn)?;
    Ok(progressos(conn, "WHERE id = ?1", &[&id])?
        .first()
        .and_then(|p| grupos.get(&p.pasta).map(|caps| montar(p, caps))))
}

pub fn capitulos(conn: &Connection, id: i64) -> rusqlite::Result<Vec<Capitulo>> {
    let Some(p) = progressos(conn, "WHERE id = ?1", &[&id])?.pop() else { return Ok(Vec::new()) };
    let ausentes: std::collections::HashSet<i64> = {
        let mut st = conn.prepare_cached("SELECT id FROM media WHERE category = 'audiobook' AND missing = 1")?;
        st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    Ok(por_pasta(conn)?
        .remove(&p.pasta)
        .unwrap_or_default()
        .into_iter()
        .map(|a| Capitulo {
            titulo: a
                .titulo
                .clone()
                .unwrap_or_else(|| Path::new(&a.caminho).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned())),
            missing: ausentes.contains(&a.id),
            media_id: a.id,
            duracao_ms: a.duracao_ms,
        })
        .collect())
}

/// Livro do arquivo `media_id` e os capítulos dele, em ordem.
fn do_arquivo(conn: &Connection, media_id: i64) -> rusqlite::Result<Option<(i64, Vec<i64>)>> {
    let caminho: Option<String> = conn
        .query_row("SELECT path FROM media WHERE id = ?1 AND category = 'audiobook'", [media_id], |r| r.get(0))
        .optional()?;
    let Some(caminho) = caminho else { return Ok(None) };
    let pasta = pasta_de(&caminho);
    let livro: Option<i64> = conn.query_row("SELECT id FROM audiolivro WHERE pasta = ?1", [&pasta], |r| r.get(0)).optional()?;
    let Some(livro) = livro else { return Ok(None) };
    let caps = por_pasta(conn)?.remove(&pasta).unwrap_or_default().into_iter().map(|a| a.id).collect();
    Ok(Some((livro, caps)))
}

pub fn e_audiolivro(conn: &Connection, media_id: i64) -> bool {
    conn.query_row("SELECT category = 'audiobook' FROM media WHERE id = ?1", [media_id], |r| r.get(0))
        .unwrap_or(false)
}

pub fn livro_do_arquivo(conn: &Connection, media_id: i64) -> Option<i64> {
    do_arquivo(conn, media_id).ok().flatten().map(|(l, _)| l)
}

/// Capítulos depois de `media_id` (o que toca ao acabar o atual).
pub fn proximos(conn: &Connection, media_id: i64) -> Vec<i64> {
    match do_arquivo(conn, media_id) {
        Ok(Some((_, caps))) => caps.iter().skip_while(|&&c| c != media_id).skip(1).copied().collect(),
        _ => Vec::new(),
    }
}

/// Guarda onde parou. No último capítulo, faltando ≤ 30 s, o livro vira concluído (devolve
/// verdadeiro quando isso muda, para a tela se atualizar).
pub fn salvar_posicao(conn: &Connection, media_id: i64, posicao_ms: i64, duracao_ms: i64) -> rusqlite::Result<bool> {
    let Some((livro, caps)) = do_arquivo(conn, media_id)? else { return Ok(false) };
    let ultimo = caps.last() == Some(&media_id);
    let terminou = ultimo && duracao_ms > 0 && duracao_ms - posicao_ms <= FIM_MS;
    let antes: bool = conn.query_row("SELECT concluido_em IS NOT NULL FROM audiolivro WHERE id = ?1", [livro], |r| r.get(0))?;
    conn.execute(
        "UPDATE audiolivro SET capitulo_media_id = ?2, posicao_ms = ?3, ouvido_em = unixepoch(),
                concluido_em = CASE WHEN ?4 THEN COALESCE(concluido_em, unixepoch()) ELSE concluido_em END
         WHERE id = ?1",
        params![livro, media_id, posicao_ms.max(0), terminou],
    )?;
    Ok(terminou && !antes)
}

/// Ponto de onde `media_id` deve começar (capítulo onde o livro parou, não concluído).
pub fn retomada(conn: &Connection, media_id: i64) -> Option<i64> {
    let (livro, _) = do_arquivo(conn, media_id).ok()??;
    conn.query_row(
        "SELECT posicao_ms FROM audiolivro WHERE id = ?1 AND capitulo_media_id = ?2 AND concluido_em IS NULL",
        params![livro, media_id],
        |r| r.get::<_, i64>(0),
    )
    .ok()
    .filter(|&p| p > RETOMAR_MIN_MS)
}

/// Capítulo para "Continuar": onde parou; concluído ou nunca aberto, o primeiro (e o
/// concluído recomeça do zero).
pub fn para_tocar(conn: &Connection, id: i64) -> rusqlite::Result<Option<i64>> {
    let caps: Vec<i64> = capitulos(conn, id)?.into_iter().map(|c| c.media_id).collect();
    let Some(p) = progressos(conn, "WHERE id = ?1", &[&id])?.pop() else { return Ok(None) };
    if p.concluido {
        conn.execute(
            "UPDATE audiolivro SET concluido_em = NULL, capitulo_media_id = NULL, posicao_ms = 0 WHERE id = ?1",
            [id],
        )?;
        return Ok(caps.first().copied());
    }
    Ok(p.capitulo.filter(|c| caps.contains(c)).or(caps.first().copied()))
}

/// Livro terminado volta a "em andamento" (tocar um capítulo dele de novo), sem perder o ponto.
pub fn reabrir(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("UPDATE audiolivro SET concluido_em = NULL WHERE id = ?1", [id])?;
    Ok(())
}

/// Marca à mão como terminado ou não (não terminado volta ao começo).
pub fn marcar(conn: &Connection, id: i64, concluido: bool) -> rusqlite::Result<()> {
    if concluido {
        conn.execute("UPDATE audiolivro SET concluido_em = unixepoch() WHERE id = ?1", [id])?;
    } else {
        conn.execute(
            "UPDATE audiolivro SET concluido_em = NULL, capitulo_media_id = NULL, posicao_ms = 0 WHERE id = ?1",
            [id],
        )?;
    }
    Ok(())
}

/// Livros começados e não terminados (Início → Continuar ouvindo).
pub fn em_andamento(conn: &Connection, max: usize) -> rusqlite::Result<Vec<Livro>> {
    Ok(livros(conn)?
        .into_iter()
        .filter(|l| !l.concluido && l.ouvido_em.is_some() && l.ouvido_ms > 0)
        .take(max)
        .collect())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn banco() -> Mutex<Connection> {
        let conn = crate::db::open(Path::new(":memory:")).unwrap();
        conn.execute_batch(
            "INSERT INTO media (id, path, title, album, artist, duration_ms, category) VALUES
             (1, '/livros/Dom Casmurro/Capítulo 10.mp3', 'Capítulo 10', 'Dom Casmurro', 'Machado', 100000, 'audiobook'),
             (2, '/livros/Dom Casmurro/Capítulo 2.mp3', 'Capítulo 2', 'Dom Casmurro', 'Machado', 200000, 'audiobook'),
             (3, '/livros/Dom Casmurro/Capítulo 1.mp3', 'Capítulo 1', 'Dom Casmurro', 'Machado', 300000, 'audiobook'),
             (4, '/livros/Solo/livro.m4b', 'O Livro', NULL, 'Autora', 600000, 'audiobook'),
             (5, '/musica/x.mp3', 'X', 'Y', 'Z', 1000, 'music');",
        )
        .unwrap();
        Mutex::new(conn)
    }

    #[test]
    fn ordem_natural() {
        assert_eq!(natural("Cap 2", "Cap 10"), Ordering::Less);
        assert_eq!(natural("cap 02", "Cap 2"), Ordering::Equal);
        assert_eq!(natural("b", "a"), Ordering::Greater);
    }

    #[test]
    fn livros_capitulos_e_progresso() {
        let db = banco();
        sincronizar(&db).unwrap();
        let c = db.lock().unwrap();
        let l = livros(&c).unwrap();
        assert_eq!(l.len(), 2);
        let dom = l.iter().find(|x| x.titulo == "Dom Casmurro").unwrap();
        assert_eq!((dom.capitulos, dom.duracao_ms, dom.autor.as_deref()), (3, 600000, Some("Machado")));
        assert_eq!(l.iter().find(|x| x.capitulos == 1).unwrap().titulo, "O Livro");
        let caps: Vec<i64> = capitulos(&c, dom.id).unwrap().iter().map(|x| x.media_id).collect();
        assert_eq!(caps, [3, 2, 1]);
        assert_eq!(para_tocar(&c, dom.id).unwrap(), Some(3));
        assert_eq!(proximos(&c, 3), [2, 1]);
        assert!(proximos(&c, 5).is_empty());

        // Parou no capítulo 2, aos 50 s.
        assert!(!salvar_posicao(&c, 2, 50_000, 200_000).unwrap());
        let dom = livro(&c, dom.id).unwrap().unwrap();
        assert_eq!((dom.capitulo_atual, dom.ouvido_ms), (1, 350_000));
        assert_eq!(retomada(&c, 2), Some(50_000));
        assert_eq!(retomada(&c, 1), None);
        assert_eq!(para_tocar(&c, dom.id).unwrap(), Some(2));
        assert_eq!(em_andamento(&c, 10).unwrap().len(), 1);

        // Fim do último capítulo: concluído; "Continuar" recomeça.
        assert!(salvar_posicao(&c, 1, 80_000, 100_000).unwrap());
        assert!(livro(&c, dom.id).unwrap().unwrap().concluido);
        // Seguir tocando (ou marcar terminado enquanto toca) não desfaz o "terminado".
        assert!(!salvar_posicao(&c, 2, 1_000, 200_000).unwrap());
        assert!(livro(&c, dom.id).unwrap().unwrap().concluido);
        assert_eq!(retomada(&c, 1), None);
        assert_eq!(para_tocar(&c, dom.id).unwrap(), Some(3));
        assert!(!livro(&c, dom.id).unwrap().unwrap().concluido);
        marcar(&c, dom.id, true).unwrap();
        assert!(em_andamento(&c, 10).unwrap().is_empty());
    }

    #[test]
    fn pasta_sem_arquivos_some() {
        let db = banco();
        sincronizar(&db).unwrap();
        db.lock().unwrap().execute("DELETE FROM media WHERE id = 4", []).unwrap();
        sincronizar(&db).unwrap();
        assert_eq!(livros(&db.lock().unwrap()).unwrap().len(), 1);
    }
}
