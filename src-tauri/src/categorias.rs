//! Categorias (várias por música): "MPB", "Samba", "Anos 70"... Ficam só no banco do Skáld
//! (nunca nos arquivos). Vêm do gênero das tags, do MusicBrainz (tags da gravação e gêneros do
//! artista) e da década do lançamento; a pessoa confere antes de gravar e pode pôr à mão.
//! Cada categoria é uma lista tocável; uma música aparece em quantas tiver.

use std::collections::HashMap;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

use crate::enriquecer;

#[derive(Serialize, Debug, PartialEq)]
pub struct Categoria {
    pub id: i64,
    pub nome: String,
    /// Músicas presentes nela.
    pub musicas: i64,
    /// Uma música dela, para a capa.
    pub primeira_media_id: Option<i64>,
}

#[derive(Serialize, Debug)]
pub struct MusicaCategoria {
    pub id: i64,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub missing: bool,
    /// Categorias que ela já tem.
    pub categorias: Vec<String>,
}

/// Sugestões para uma música. `incerto`: achada só pelo título (a tela mostra desmarcada).
#[derive(Serialize, Debug, Default)]
pub struct Sugestao {
    pub nomes: Vec<String>,
    pub incerto: bool,
}

/// Nomes em português e com maiúsculas para as tags mais comuns do MusicBrainz (em inglês).
const TRADUCOES: &[(&str, &str)] = &[
    ("mpb", "MPB"),
    ("música popular brasileira", "MPB"),
    ("musica popular brasileira", "MPB"),
    ("brazilian", "Brasileira"),
    ("brazilian music", "Brasileira"),
    ("bossa nova", "Bossa Nova"),
    ("samba", "Samba"),
    ("pagode", "Pagode"),
    ("forró", "Forró"),
    ("forro", "Forró"),
    ("sertanejo", "Sertanejo"),
    ("funk carioca", "Funk Carioca"),
    ("baile funk", "Funk Carioca"),
    ("axé", "Axé"),
    ("hip hop", "Hip Hop"),
    ("hip-hop", "Hip Hop"),
    ("rap", "Rap"),
    ("brazilian hip hop", "Rap Nacional"),
    ("rock", "Rock"),
    ("hard rock", "Hard Rock"),
    ("heavy metal", "Heavy Metal"),
    ("pop", "Pop"),
    ("pop rock", "Pop Rock"),
    ("electronic", "Eletrônica"),
    ("classical", "Clássica"),
    ("soundtrack", "Trilha Sonora"),
    ("christian", "Gospel"),
    ("gospel", "Gospel"),
    ("soul", "Soul"),
    ("jazz", "Jazz"),
    ("blues", "Blues"),
    ("reggae", "Reggae"),
    ("country", "Country"),
    ("folk", "Folk"),
    ("r&b", "R&B"),
    ("rnb", "R&B"),
    ("contemporary r&b", "R&B"),
    ("singer-songwriter", "Cantor e Compositor"),
    ("latin", "Latina"),
    ("latin pop", "Pop Latino"),
    ("anime", "Anime"),
    ("j-pop", "J-Pop"),
    ("k-pop", "K-Pop"),
    ("video game music", "Trilha de Jogos"),
];

/// Tags que não dizem nada sobre o estilo.
const IGNORAR: &[&str] = &[
    "seen live", "favorites", "favourite", "favorite", "albums i own", "owned", "under 2000 listeners",
    "male vocalists", "female vocalists", "male vocalist", "female vocalist", "spotify", "check out", "beautiful",
    "love", "awesome", "cover", "live", "single", "remaster", "remastered",
];

/// Nome de categoria a partir de uma tag: tradução conhecida, senão cada palavra com maiúscula.
pub fn nome_de_tag(tag: &str) -> Option<String> {
    let t = tag.split_whitespace().collect::<Vec<_>>().join(" ");
    let baixo = t.to_lowercase();
    if baixo.is_empty() || baixo.len() > 40 || IGNORAR.contains(&baixo.as_str()) {
        return None;
    }
    // Décadas em tags ("80s", "1980s") viram o mesmo rótulo da década calculada.
    if let Some(d) = decada_de_tag(&baixo) {
        return Some(d);
    }
    if let Some((_, n)) = TRADUCOES.iter().find(|(k, _)| *k == baixo) {
        return Some((*n).to_string());
    }
    // Siglas curtas ficam como estão ("EDM", "MPB"); o resto ganha maiúscula no começo das palavras.
    if t.len() <= 4 && t.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) {
        return Some(t);
    }
    Some(
        t.split(' ')
            .map(|p| {
                let mut c = p.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// "Anos 70", "Anos 2000", "Anos 2010".
pub fn decada(ano: i64) -> Option<String> {
    if !(1900..=2100).contains(&ano) {
        return None;
    }
    let d = ano / 10 * 10;
    Some(if d < 2000 { format!("Anos {}", d % 100) } else { format!("Anos {d}") })
}

fn decada_de_tag(t: &str) -> Option<String> {
    let num = t.strip_suffix('s')?;
    let ano: i64 = num.parse().ok()?;
    match num.len() {
        2 => decada(1900 + ano),
        4 if ano % 10 == 0 => decada(ano),
        _ => None,
    }
}

/// Gêneros do artista no MusicBrainz (curados), guardados em memória por id.
fn generos_do_artista(id: &str) -> Vec<String> {
    static CACHE: Mutex<Option<HashMap<String, Vec<String>>>> = Mutex::new(None);
    if let Some(v) = CACHE.lock().ok().and_then(|c| c.as_ref().and_then(|m| m.get(id).cloned())) {
        return v;
    }
    if id.len() != 36 || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Vec::new();
    }
    let url = format!("https://musicbrainz.org/ws/2/artist/{id}");
    let Ok(json) = enriquecer::consultar_mb(&url, &[("inc", "genres"), ("fmt", "json")]) else {
        return Vec::new();
    };
    let v = generos(&json);
    if let Ok(mut c) = CACHE.lock() {
        c.get_or_insert_with(HashMap::new).insert(id.to_string(), v.clone());
    }
    v
}

/// Até 3 gêneros mais votados de uma resposta com `genres`.
fn generos(json: &Value) -> Vec<String> {
    let mut g: Vec<(i64, String)> = json["genres"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| Some((x["count"].as_i64().unwrap_or(0), x["name"].as_str()?.to_string())))
        .collect();
    g.sort_by_key(|(n, _)| -n);
    g.into_iter().take(3).map(|(_, n)| n).collect()
}

/// Junta nomes sem repetir (sem diferenciar maiúsculas e acentos), na ordem.
fn juntar(nomes: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut vistos = std::collections::HashSet::new();
    nomes.into_iter().filter(|n| vistos.insert(crate::search::normalizar(n))).collect()
}

/// Gênero das tags do arquivo dividido ("Rock; Pop", "MPB / Samba").
fn do_genero_local(genero: Option<&str>) -> Vec<String> {
    genero
        .into_iter()
        .flat_map(|g| g.split([';', '/', ',', '|']))
        .filter_map(nome_de_tag)
        .collect()
}

/// Sugere categorias para uma música (precisa de internet para o MusicBrainz; sem ela, só
/// as que vêm do arquivo). Nada é gravado aqui.
pub fn sugerir(conn_db: &Mutex<Connection>, id: i64) -> Result<Sugestao, String> {
    let (titulo, artista, genero, ano, duracao): (Option<String>, Option<String>, Option<String>, Option<i64>, Option<i64>) = {
        let conn = conn_db.lock().map_err(|_| "Banco indisponível.".to_string())?;
        conn.query_row("SELECT title, artist, genre, year, duration_ms FROM media WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("Música não encontrada.")?
    };
    let mut nomes = do_genero_local(genero.as_deref());
    let mut ano_lancamento = ano;
    let mut incerto = false;
    if let Some(t) = titulo.as_deref().filter(|t| !t.trim().is_empty()) {
        let candidatos = enriquecer::buscar_flexivel(t, artista.as_deref(), duracao)?;
        let bons: Vec<&enriquecer::Candidato> = candidatos.iter().filter(|c| enriquecer::confiavel(c, duracao)).collect();
        if let Some(melhor) = bons.first() {
            incerto = melhor.incerto;
            nomes.extend(melhor.tags.iter().filter_map(|t| nome_de_tag(t)));
            if let Some(a) = melhor.artista_id.as_deref() {
                nomes.extend(generos_do_artista(a).iter().filter_map(|t| nome_de_tag(t)));
            }
            // Década do primeiro lançamento conhecido da mesma gravação (o mais antigo).
            let primeiro = bons.iter().filter(|c| c.campos.title == melhor.campos.title).filter_map(|c| c.campos.year).min();
            ano_lancamento = match (primeiro, ano) {
                (Some(p), Some(a)) => Some(p.min(a)),
                (p, a) => p.or(a),
            };
        }
    }
    // Até 6 estilos (arquivo, gravação, artista, nessa ordem) e a década.
    let mut nomes: Vec<String> = juntar(nomes).into_iter().filter(|n| !n.starts_with("Anos ")).take(6).collect();
    nomes.extend(ano_lancamento.and_then(decada));
    Ok(Sugestao { nomes: juntar(nomes), incerto })
}

/// Id da categoria com esse nome (sem diferenciar maiúsculas), criando se não existir.
fn garantir(conn: &Connection, nome: &str) -> rusqlite::Result<i64> {
    conn.execute("INSERT OR IGNORE INTO tag (nome) VALUES (?1)", [nome])?;
    conn.query_row("SELECT id FROM tag WHERE nome = ?1", [nome], |r| r.get(0))
}

/// Põe a música nas categorias (as que já tem ficam). `origem`: "manual" ou "sugestao".
pub fn adicionar(conn: &mut Connection, media_id: i64, nomes: &[String], origem: &str) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;
    let mut n = 0;
    for nome in nomes {
        let nome = nome.split_whitespace().collect::<Vec<_>>().join(" ");
        if nome.is_empty() || nome.chars().count() > 60 {
            continue;
        }
        let tag = garantir(&tx, &nome)?;
        n += tx.execute(
            "INSERT OR IGNORE INTO media_tag (media_id, tag_id, origem) VALUES (?1, ?2, ?3)",
            params![media_id, tag, origem],
        )?;
    }
    tx.commit()?;
    Ok(n)
}

/// Tira a música de uma categoria; categoria que fica vazia some.
pub fn remover(conn: &Connection, media_id: i64, nome: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM media_tag WHERE media_id = ?1 AND tag_id = (SELECT id FROM tag WHERE nome = ?2)",
        params![media_id, nome],
    )?;
    limpar_vazias(conn)
}

/// Apaga a categoria (as músicas ficam na biblioteca).
pub fn excluir(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM media_tag WHERE tag_id = ?1", [id])?;
    conn.execute("DELETE FROM tag WHERE id = ?1", [id])?;
    Ok(())
}

/// Renomeia; se já existir outra com o novo nome, junta as duas.
pub fn renomear(conn: &mut Connection, id: i64, nome: &str) -> Result<(), String> {
    let nome = nome.split_whitespace().collect::<Vec<_>>().join(" ");
    if nome.is_empty() {
        return Err("Informe um nome.".into());
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let outra: Option<i64> = tx
        .query_row("SELECT id FROM tag WHERE nome = ?1 AND id <> ?2", params![nome, id], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    match outra {
        Some(o) => {
            tx.execute("INSERT OR IGNORE INTO media_tag (media_id, tag_id, origem) SELECT media_id, ?2, origem FROM media_tag WHERE tag_id = ?1", params![id, o])
                .map_err(|e| e.to_string())?;
            excluir(&tx, id).map_err(|e| e.to_string())?;
        }
        None => {
            tx.execute("UPDATE tag SET nome = ?2 WHERE id = ?1", params![id, nome]).map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

fn limpar_vazias(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM tag WHERE id NOT IN (SELECT tag_id FROM media_tag)", [])?;
    Ok(())
}

/// Todas as categorias com músicas presentes, por nome.
pub fn listar(conn: &Connection) -> rusqlite::Result<Vec<Categoria>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.nome, COUNT(m.id), MIN(m.id) FROM tag t
         JOIN media_tag mt ON mt.tag_id = t.id
         JOIN media m ON m.id = mt.media_id AND m.category = 'music'
         GROUP BY t.id ORDER BY t.nome COLLATE SEMACENTO",
    )?;
    let v = stmt.query_map([], |r| Ok(Categoria { id: r.get(0)?, nome: r.get(1)?, musicas: r.get(2)?, primeira_media_id: r.get(3)? }))?;
    v.collect()
}

pub fn nome(conn: &Connection, id: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT nome FROM tag WHERE id = ?1", [id], |r| r.get(0)).optional()
}

const ORDEM: &str = "ORDER BY COALESCE(m.album_artist, m.artist) IS NULL, COALESCE(m.album_artist, m.artist) COLLATE SEMACENTO,
                     m.album COLLATE SEMACENTO, m.disc_number, m.track_number, m.title COLLATE SEMACENTO, m.id";

/// Ids das músicas da categoria, na ordem da biblioteca.
pub fn ids(conn: &Connection, id: i64) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT m.id FROM media m JOIN media_tag mt ON mt.media_id = m.id
         WHERE mt.tag_id = ?1 AND m.category = 'music' {ORDEM}"
    ))?;
    let v = stmt.query_map([id], |r| r.get(0))?;
    v.collect()
}

fn categorias_de(conn: &Connection) -> rusqlite::Result<HashMap<i64, Vec<String>>> {
    let mut stmt = conn.prepare("SELECT mt.media_id, t.nome FROM media_tag mt JOIN tag t ON t.id = mt.tag_id ORDER BY t.nome COLLATE SEMACENTO")?;
    let mut mapa: HashMap<i64, Vec<String>> = HashMap::new();
    for linha in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, nome) = linha?;
        mapa.entry(id).or_default().push(nome);
    }
    Ok(mapa)
}

/// Músicas de uma categoria (`Some`) ou todas as de música presentes (`None`), cada uma com
/// as categorias que tem.
pub fn musicas(conn: &Connection, categoria: Option<i64>) -> rusqlite::Result<Vec<MusicaCategoria>> {
    let mapa = categorias_de(conn)?;
    let sql = match categoria {
        Some(_) => format!(
            "SELECT m.id, m.title, m.artist, m.album, m.duration_ms, m.missing FROM media m
             JOIN media_tag mt ON mt.media_id = m.id WHERE mt.tag_id = ?1 AND m.category = 'music' {ORDEM}"
        ),
        None => format!(
            "SELECT m.id, m.title, m.artist, m.album, m.duration_ms, m.missing FROM media m
             WHERE m.category = 'music' AND m.missing = 0 AND ?1 IS NULL {ORDEM}"
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    let v = stmt.query_map([categoria], |r| {
        let id: i64 = r.get(0)?;
        Ok(MusicaCategoria {
            id,
            title: r.get(1)?,
            artist: r.get(2)?,
            album: r.get(3)?,
            duration_ms: r.get(4)?,
            missing: r.get(5)?,
            categorias: mapa.get(&id).cloned().unwrap_or_default(),
        })
    })?;
    v.collect()
}

/// Categorias de uma música.
pub fn da_musica(conn: &Connection, media_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.nome FROM tag t JOIN media_tag mt ON mt.tag_id = t.id WHERE mt.media_id = ?1 ORDER BY t.nome COLLATE SEMACENTO",
    )?;
    let v = stmt.query_map([media_id], |r| r.get(0))?;
    v.collect()
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::Path;

    #[test]
    fn nomes_e_decadas() {
        assert_eq!(nome_de_tag("mpb").as_deref(), Some("MPB"));
        assert_eq!(nome_de_tag("samba").as_deref(), Some("Samba"));
        assert_eq!(nome_de_tag("new wave").as_deref(), Some("New Wave"));
        assert_eq!(nome_de_tag("seen live"), None);
        assert_eq!(nome_de_tag("80s").as_deref(), Some("Anos 80"));
        assert_eq!(nome_de_tag("1970s").as_deref(), Some("Anos 70"));
        assert_eq!(decada(1976).as_deref(), Some("Anos 70"));
        assert_eq!(decada(2004).as_deref(), Some("Anos 2000"));
        assert_eq!(decada(2019).as_deref(), Some("Anos 2010"));
        assert_eq!(do_genero_local(Some("MPB; samba")), vec!["MPB".to_string(), "Samba".to_string()]);
        assert_eq!(juntar(["MPB".to_string(), "mpb".to_string(), "Samba".to_string()]).len(), 2);
        let json: Value = serde_json::from_str(r#"{"genres":[{"name":"samba","count":3},{"name":"mpb","count":5}]}"#).unwrap();
        assert_eq!(generos(&json), vec!["mpb".to_string(), "samba".to_string()]);
    }

    #[test]
    fn varias_categorias_por_musica() {
        let mut conn = crate::db::open(Path::new(":memory:")).unwrap();
        conn.execute("INSERT INTO media (id, path, title, artist) VALUES (1, '/a.mp3', 'O Mundo É um Moinho', 'Cartola')", []).unwrap();
        conn.execute("INSERT INTO media (id, path, title, artist) VALUES (2, '/b.mp3', 'Garota de Ipanema', 'Tom Jobim')", []).unwrap();
        adicionar(&mut conn, 1, &["MPB".into(), "Anos 70".into(), "Samba".into()], "sugestao").unwrap();
        adicionar(&mut conn, 2, &["mpb".into(), "Bossa Nova".into()], "manual").unwrap();
        let l = listar(&conn).unwrap();
        let mpb = l.iter().find(|c| c.nome == "MPB").unwrap();
        assert_eq!(mpb.musicas, 2); // "mpb" caiu na mesma categoria
        assert_eq!(ids(&conn, mpb.id).unwrap().len(), 2);
        assert_eq!(da_musica(&conn, 1).unwrap(), vec!["Anos 70", "MPB", "Samba"]);
        remover(&conn, 2, "Bossa Nova").unwrap();
        assert!(listar(&conn).unwrap().iter().all(|c| c.nome != "Bossa Nova")); // vazia some
        let samba = listar(&conn).unwrap().into_iter().find(|c| c.nome == "Samba").unwrap();
        renomear(&mut conn, samba.id, "MPB").unwrap(); // junta
        assert_eq!(da_musica(&conn, 1).unwrap(), vec!["Anos 70", "MPB"]);
        // Excluir a música da biblioteca tira das categorias.
        conn.execute("DELETE FROM media WHERE id = 1", []).unwrap();
        assert_eq!(musicas(&conn, None).unwrap().len(), 1);
        assert_eq!(musicas(&conn, Some(mpb.id)).unwrap().len(), 1);
    }

    /// Consulta real: cargo test ensaio_categorias -- --ignored --nocapture
    #[test]
    #[ignore]
    fn ensaio_categorias() {
        let conn = crate::db::open(Path::new(":memory:")).unwrap();
        conn.execute("INSERT INTO media (id, path, title, artist, duration_ms) VALUES (1, '/a.mp3', 'O Mundo É um Moinho', 'Cartola', 236000)", []).unwrap();
        conn.execute("INSERT INTO media (id, path, title, duration_ms) VALUES (2, '/b.mp3', '11 Def Leppard - Armageddon It', 322000)", []).unwrap();
        let db = Mutex::new(conn);
        for id in [1, 2] {
            println!("{id}: {:?}", sugerir(&db, id));
        }
    }
}
