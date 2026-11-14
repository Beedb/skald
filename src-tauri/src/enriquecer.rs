//! Enriquecimento de metadados (seção 6): sugere informações para campos incompletos a
//! partir do MusicBrainz e capas do Cover Art Archive. Nada é gravado sem o usuário
//! escolher campo por campo; ele decide se grava só no banco ou também no arquivo.
//! Sem internet, a consulta falha com uma mensagem e o resto do app segue igual.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MUSICBRAINZ: &str = "https://musicbrainz.org/ws/2/recording";
const CAPAS: &str = "https://coverartarchive.org/release";
/// Identificação genérica do cliente (o MusicBrainz pede um User-Agent próprio).
const CLIENTE: &str = "Skald/0.1.0 (reprodutor de musica local)";
const LIMITE_CAPA: u64 = 15 * 1024 * 1024;

/// Campos editáveis de uma música. Na gravação, só os `Some` são alterados.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct Campos {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i64>,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
}

#[derive(Serialize, Debug)]
pub struct Atual {
    #[serde(flatten)]
    pub campos: Campos,
    pub tem_capa: bool,
    pub missing: bool,
    pub duracao_ms: Option<i64>,
}

/// Uma sugestão: uma gravação do MusicBrainz num lançamento (álbum) específico.
#[derive(Serialize, Debug, PartialEq)]
pub struct Candidato {
    pub campos: Campos,
    /// 0 a 100, do MusicBrainz, ajustado pela diferença de duração.
    pub pontuacao: i64,
    pub duracao_ms: Option<i64>,
    /// Id do lançamento, para a capa no Cover Art Archive.
    pub release_id: Option<String>,
}

pub fn atual(conn: &Connection, id: i64) -> Result<Atual, String> {
    let (campos, path, missing, duracao_ms) = conn
        .query_row(
            "SELECT title, artist, album, album_artist, genre, year, track_number, disc_number, path, missing, duration_ms
             FROM media WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    Campos {
                        title: r.get(0)?,
                        artist: r.get(1)?,
                        album: r.get(2)?,
                        album_artist: r.get(3)?,
                        genre: r.get(4)?,
                        year: r.get(5)?,
                        track_number: r.get(6)?,
                        disc_number: r.get(7)?,
                    },
                    r.get::<_, String>(8)?,
                    r.get::<_, bool>(9)?,
                    r.get::<_, Option<i64>>(10)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("Música não encontrada na biblioteca.")?;
    let tem_capa = !missing && crate::details::capa_bruta(Path::new(&path)).is_some();
    Ok(Atual { campos, tem_capa, missing, duracao_ms })
}

fn agente() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .user_agent(CLIENTE)
        .build()
        .into()
}

fn erro_rede(e: ureq::Error, servico: &str) -> String {
    match e {
        ureq::Error::StatusCode(503) => format!("O {servico} está ocupado. Tente de novo em alguns segundos."),
        ureq::Error::StatusCode(404) => format!("Nada encontrado no {servico}."),
        ureq::Error::StatusCode(c) => format!("O {servico} respondeu com erro ({c})."),
        ureq::Error::Timeout(_) => format!("O {servico} demorou demais para responder."),
        _ => format!("Sem conexão: não foi possível consultar o {servico}."),
    }
}

/// Aspas e barras escapadas para a consulta Lucene do MusicBrainz.
fn termo(texto: &str) -> String {
    let limpo: String = texto.chars().filter(|c| !c.is_control()).collect();
    format!("\"{}\"", limpo.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Procura gravações pelo título (e artista, se houver). `duracao_ms` ajuda a ordenar.
pub fn buscar(titulo: &str, artista: Option<&str>, duracao_ms: Option<i64>) -> Result<Vec<Candidato>, String> {
    let titulo = titulo.trim();
    if titulo.is_empty() {
        return Err("Informe ao menos o título para buscar.".into());
    }
    let mut consulta = format!("recording:{}", termo(titulo));
    if let Some(a) = artista.map(str::trim).filter(|a| !a.is_empty()) {
        // Créditos divididos: basta um dos artistas bater.
        let nomes: Vec<String> = crate::artistas::separar(a).iter().map(|n| format!("artist:{}", termo(n))).collect();
        if !nomes.is_empty() {
            consulta.push_str(&format!(" AND ({})", nomes.join(" OR ")));
        }
    }
    let texto = agente()
        .get(MUSICBRAINZ)
        .query("query", &consulta)
        .query("fmt", "json")
        .query("limit", "10")
        .call()
        .map_err(|e| erro_rede(e, "MusicBrainz"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| erro_rede(e, "MusicBrainz"))?;
    let json: Value = serde_json::from_str(&texto).map_err(|_| "Resposta inesperada do MusicBrainz.".to_string())?;
    Ok(candidatos(&json, duracao_ms))
}

/// Créditos do MusicBrainz ("A feat. B") montados com as frases de ligação.
fn credito(v: &Value) -> Option<String> {
    let partes = v.as_array()?;
    let texto: String = partes
        .iter()
        .map(|p| {
            let nome = p["name"].as_str().or_else(|| p["artist"]["name"].as_str()).unwrap_or("");
            format!("{nome}{}", p["joinphrase"].as_str().unwrap_or(""))
        })
        .collect();
    Some(texto.trim().to_string()).filter(|t| !t.is_empty())
}

/// Transforma a resposta da busca em candidatos (até 3 lançamentos por gravação).
pub fn candidatos(json: &Value, duracao_ms: Option<i64>) -> Vec<Candidato> {
    let mut lista = Vec::new();
    for rec in json["recordings"].as_array().into_iter().flatten() {
        let titulo = rec["title"].as_str().map(String::from);
        let artista = credito(&rec["artist-credit"]);
        let duracao = rec["length"].as_i64();
        let mut pontuacao = rec["score"].as_i64().unwrap_or(0);
        if let (Some(a), Some(b)) = (duracao, duracao_ms) {
            // Até 3 s de diferença não pesa; depois, perde pontos.
            let dif = (a - b).abs() / 1000;
            pontuacao -= ((dif - 3).max(0) * 2).min(60);
        }
        // Gênero: a tag mais votada da gravação.
        let genero = rec["tags"]
            .as_array()
            .and_then(|t| t.iter().max_by_key(|x| x["count"].as_i64().unwrap_or(0)))
            .and_then(|t| t["name"].as_str())
            .map(String::from);
        let lancamentos: Vec<&Value> = rec["releases"].as_array().map(|r| r.iter().take(3).collect()).unwrap_or_default();
        let base = Campos { title: titulo.clone(), artist: artista.clone(), genre: genero.clone(), ..Default::default() };
        if lancamentos.is_empty() {
            lista.push(Candidato { campos: base, pontuacao, duracao_ms: duracao, release_id: None });
            continue;
        }
        for rel in lancamentos {
            let midia = &rel["media"][0];
            let faixa = midia["track"][0]["number"].as_str().and_then(|n| n.trim().parse().ok());
            let ano = rel["date"].as_str().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok());
            let artista_album = credito(&rel["artist-credit"]).filter(|a| Some(a) != artista.as_ref());
            lista.push(Candidato {
                campos: Campos {
                    album: rel["title"].as_str().map(String::from),
                    album_artist: artista_album,
                    year: ano,
                    track_number: faixa,
                    disc_number: midia["position"].as_i64(),
                    ..base.clone()
                },
                pontuacao,
                duracao_ms: duracao,
                release_id: rel["id"].as_str().map(String::from),
            });
        }
    }
    lista.sort_by_key(|c| -c.pontuacao);
    lista
}

/// Id de lançamento do MusicBrainz (UUID): o único texto aceito para montar a URL da capa.
fn release_valido(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Baixa a capa frontal do lançamento no Cover Art Archive.
pub fn baixar_capa(release_id: &str) -> Result<Vec<u8>, String> {
    if !release_valido(release_id) {
        return Err("Lançamento inválido.".into());
    }
    agente()
        .get(&format!("{CAPAS}/{release_id}/front-500"))
        .call()
        .map_err(|e| erro_rede(e, "Cover Art Archive"))?
        .body_mut()
        .with_config()
        .limit(LIMITE_CAPA)
        .read_to_vec()
        .map_err(|e| erro_rede(e, "Cover Art Archive"))
}

fn validar(c: &Campos) -> Result<Campos, String> {
    let texto = |v: &Option<String>| v.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let c = Campos {
        title: texto(&c.title),
        artist: texto(&c.artist),
        album: texto(&c.album),
        album_artist: texto(&c.album_artist),
        genre: texto(&c.genre),
        ..c.clone()
    };
    if c.year.is_some_and(|a| !(1000..=2100).contains(&a)) {
        return Err("Ano inválido.".into());
    }
    if c.track_number.is_some_and(|n| !(1..=999).contains(&n)) || c.disc_number.is_some_and(|n| !(1..=99).contains(&n)) {
        return Err("Número de faixa ou disco inválido.".into());
    }
    Ok(c)
}

/// Grava as tags no arquivo de áudio (só os campos escolhidos e, se houver, a capa frontal).
fn gravar_no_arquivo(path: &Path, c: &Campos, capa: Option<Vec<u8>>) -> Result<(), String> {
    use lofty::config::WriteOptions;
    use lofty::picture::{Picture, PictureType};
    use lofty::prelude::*;
    use lofty::tag::{ItemKey, Tag};

    let mut arquivo = lofty::read_from_path(path).map_err(|e| format!("Não foi possível ler as tags: {e}"))?;
    let tipo = arquivo.primary_tag_type();
    if arquivo.tag(tipo).is_none() {
        arquivo.insert_tag(Tag::new(tipo));
    }
    let tag = arquivo.tag_mut(tipo).ok_or("Formato sem suporte a tags.")?;
    if let Some(v) = &c.title {
        tag.set_title(v.clone());
    }
    if let Some(v) = &c.artist {
        tag.set_artist(v.clone());
    }
    if let Some(v) = &c.album {
        tag.set_album(v.clone());
    }
    if let Some(v) = &c.album_artist {
        tag.insert_text(ItemKey::AlbumArtist, v.clone());
    }
    if let Some(v) = &c.genre {
        tag.set_genre(v.clone());
    }
    if let Some(v) = c.year {
        tag.set_date(lofty::tag::items::Timestamp { year: v as u16, ..Default::default() });
    }
    if let Some(v) = c.track_number {
        tag.set_track(v as u32);
    }
    if let Some(v) = c.disc_number {
        tag.set_disk(v as u32);
    }
    if let Some(bytes) = capa {
        tag.remove_picture_type(PictureType::CoverFront);
        let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            lofty::picture::MimeType::Png
        } else {
            lofty::picture::MimeType::Jpeg
        };
        tag.push_picture(Picture::unchecked(bytes).pic_type(PictureType::CoverFront).mime_type(mime).build());
    }
    tag.save_to_path(path, WriteOptions::default()).map_err(|e| format!("Não foi possível gravar no arquivo: {e}"))
}

/// Aplica os campos escolhidos: no banco sempre; no arquivo se `no_arquivo` (com a capa, se dada).
/// `conn_db` só é travado para ler o caminho e para gravar no banco, nunca durante a rede.
pub fn aplicar(
    conn_db: &std::sync::Mutex<Connection>,
    id: i64,
    campos: &Campos,
    no_arquivo: bool,
    release_capa: Option<&str>,
) -> Result<(), String> {
    let c = validar(campos)?;
    let path: String = {
        let conn = conn_db.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
        conn.query_row("SELECT path FROM media WHERE id = ?1", [id], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or("Música não encontrada na biblioteca.")?
    };
    if no_arquivo {
        if !Path::new(&path).is_file() {
            return Err("Arquivo não encontrado: as informações não podem ser gravadas nele.".into());
        }
        let capa = release_capa.map(baixar_capa).transpose()?;
        gravar_no_arquivo(Path::new(&path), &c, capa)?;
    } else if release_capa.is_some() {
        return Err("A capa só pode ser gravada no arquivo.".into());
    }
    let mut conn = conn_db.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
    gravar_no_banco(&conn, id, &c).map_err(|e| e.to_string())?;
    if no_arquivo {
        // O arquivo mudou: tamanho e data novos (miniatura e hash são refeitos).
        if let Ok(meta) = std::fs::metadata(&path) {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            conn.execute(
                "UPDATE media SET file_size = ?2, file_mtime = ?3, file_hash = NULL WHERE id = ?1",
                params![id, meta.len() as i64, mtime],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    crate::search::preencher_pendentes(&mut conn).map_err(|e| e.to_string())
}

fn gravar_no_banco(conn: &Connection, id: i64, c: &Campos) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE media SET
            title = COALESCE(?2, title), artist = COALESCE(?3, artist), album = COALESCE(?4, album),
            album_artist = COALESCE(?5, album_artist), genre = COALESCE(?6, genre), year = COALESCE(?7, year),
            track_number = COALESCE(?8, track_number), disc_number = COALESCE(?9, disc_number),
            search_text = NULL
         WHERE id = ?1",
        params![id, c.title, c.artist, c.album, c.album_artist, c.genre, c.year, c.track_number, c.disc_number],
    )?;
    Ok(())
}

/// Música com informações faltando (lista para completar em lote).
#[derive(Serialize, Debug, PartialEq)]
pub struct Incompleta {
    pub id: i64,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    /// Campos vazios entre artista, álbum, ano e gênero.
    pub faltando: Vec<String>,
}

/// Músicas presentes (arquivo encontrado) sem artista, álbum, ano ou gênero.
pub fn incompletas(conn: &Connection) -> rusqlite::Result<Vec<Incompleta>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, artist, album, duration_ms, year, genre FROM media
         WHERE category = 'music' AND missing = 0
           AND (artist IS NULL OR album IS NULL OR year IS NULL OR genre IS NULL)
         ORDER BY artist IS NULL DESC, artist COLLATE NOCASE, album COLLATE NOCASE, title COLLATE NOCASE",
    )?;
    let linhas = stmt.query_map([], |r| {
        let artist: Option<String> = r.get(2)?;
        let album: Option<String> = r.get(3)?;
        let year: Option<i64> = r.get(5)?;
        let genre: Option<String> = r.get(6)?;
        let faltando = [("artist", artist.is_none()), ("album", album.is_none()), ("year", year.is_none()), ("genre", genre.is_none())]
            .into_iter()
            .filter(|(_, f)| *f)
            .map(|(n, _)| n.to_string())
            .collect();
        Ok(Incompleta { id: r.get(0)?, title: r.get(1)?, artist, album, duration_ms: r.get(4)?, faltando })
    })?;
    linhas.collect()
}

/// Valor de um campo numa seleção: o comum a todas, ou `varios`.
#[derive(Serialize, Debug, PartialEq)]
pub struct ValorLote<T> {
    pub valor: Option<T>,
    pub varios: bool,
}

#[derive(Serialize, Debug)]
pub struct Lote {
    pub artist: ValorLote<String>,
    pub album: ValorLote<String>,
    pub genre: ValorLote<String>,
    pub year: ValorLote<i64>,
    pub musicas: usize,
}

fn juntar<T: PartialEq + Clone>(valores: &[Option<T>]) -> ValorLote<T> {
    let primeiro = valores.first().cloned().flatten();
    let varios = valores.iter().any(|v| *v != primeiro);
    ValorLote { valor: if varios { None } else { primeiro }, varios }
}

/// Edição em lote (seção 5): o que as músicas selecionadas têm em comum.
pub fn lote_valores(conn: &Connection, ids: &[i64]) -> Result<Lote, String> {
    let mut campos = Vec::with_capacity(ids.len());
    for &id in ids {
        campos.push(atual(conn, id)?.campos);
    }
    let pegar = |f: fn(&Campos) -> Option<String>| campos.iter().map(f).collect::<Vec<_>>();
    Ok(Lote {
        artist: juntar(&pegar(|c| c.artist.clone())),
        album: juntar(&pegar(|c| c.album.clone())),
        genre: juntar(&pegar(|c| c.genre.clone())),
        year: juntar(&campos.iter().map(|c| c.year).collect::<Vec<_>>()),
        musicas: ids.len(),
    })
}

#[derive(Serialize, Debug, Default)]
pub struct ResultadoLote {
    pub alteradas: usize,
    pub falhas: Vec<String>,
}

/// Aplica os mesmos campos (só artista, álbum, gênero e ano) a várias músicas.
/// Uma falha (ex.: arquivo somente leitura) não impede as demais.
pub fn lote_aplicar(conn_db: &std::sync::Mutex<Connection>, ids: &[i64], campos: &Campos, no_arquivo: bool) -> ResultadoLote {
    let campos = Campos {
        artist: campos.artist.clone(),
        album: campos.album.clone(),
        genre: campos.genre.clone(),
        year: campos.year,
        ..Default::default()
    };
    let mut r = ResultadoLote::default();
    for &id in ids {
        match aplicar(conn_db, id, &campos, no_arquivo, None) {
            Ok(()) => r.alteradas += 1,
            Err(e) => {
                let nome = conn_db
                    .lock()
                    .ok()
                    .and_then(|c| c.query_row("SELECT path FROM media WHERE id = ?1", [id], |x| x.get::<_, String>(0)).ok())
                    .unwrap_or_else(|| format!("música {id}"));
                r.falhas.push(format!("{nome}: {e}"));
            }
        }
    }
    r
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn lista_incompletas() {
        let conn = crate::db::open(Path::new(":memory:")).unwrap();
        conn.execute_batch(
            "INSERT INTO media (id, path, title, artist, album, year, genre, missing) VALUES
               (1, '/a.mp3', 'A', 'X', 'D', 2020, 'Rap', 0),
               (2, '/b.mp3', 'B', NULL, 'D', NULL, 'Rap', 0),
               (3, '/c.mp3', 'C', NULL, NULL, NULL, NULL, 1);",
        )
        .unwrap();
        let l = incompletas(&conn).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].id, l[0].faltando.clone()), (2, vec!["artist".to_string(), "year".to_string()]));
    }

    #[test]
    fn edicao_em_lote() {
        let conn = crate::db::open(Path::new(":memory:")).unwrap();
        conn.execute_batch(
            "INSERT INTO media (id, path, title, artist, album, year) VALUES
               (1, '/a.mp3', 'A', 'X', 'Disco', 2020), (2, '/b.mp3', 'B', 'Y', 'Disco', 2020);",
        )
        .unwrap();
        let l = lote_valores(&conn, &[1, 2]).unwrap();
        assert_eq!(l.artist, ValorLote { valor: None, varios: true });
        assert_eq!(l.album, ValorLote { valor: Some("Disco".into()), varios: false });
        assert_eq!(l.year.valor, Some(2020));
        assert_eq!(l.genre, ValorLote { valor: None, varios: false });
        let db = std::sync::Mutex::new(conn);
        // Título vem junto, mas o lote só mexe nos quatro campos permitidos.
        let campos = Campos { artist: Some("Z".into()), title: Some("Ignorado".into()), ..Default::default() };
        let r = lote_aplicar(&db, &[1, 2, 99], &campos, false);
        assert_eq!(r.alteradas, 2);
        assert_eq!(r.falhas.len(), 1);
        let conn = db.lock().unwrap();
        let a = atual(&conn, 1).unwrap().campos;
        assert_eq!((a.artist.as_deref(), a.title.as_deref()), (Some("Z"), Some("A")));
    }

    #[test]
    fn le_resposta_do_musicbrainz() {
        let json: Value = serde_json::from_str(
            r#"{"recordings":[
              {"id":"r1","score":100,"title":"Obsessão","length":238000,
               "artist-credit":[{"name":"Rodrigo Zin","joinphrase":" feat. "},{"name":"GEO"}],
               "tags":[{"name":"hip hop","count":3},{"name":"rap","count":1}],
               "releases":[{"id":"5b11f4ce-a62d-471e-81fc-a69a8278c7da","title":"Invasão","date":"2023-05-12",
                 "media":[{"position":1,"track":[{"number":"4","title":"Obsessão"}]}]}]},
              {"id":"r2","score":100,"title":"Obsessão","length":120000,
               "artist-credit":[{"name":"Outro"}]}
            ]}"#,
        )
        .unwrap();
        let c = candidatos(&json, Some(238_500));
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].release_id.as_deref(), Some("5b11f4ce-a62d-471e-81fc-a69a8278c7da"));
        assert_eq!(c[0].campos.artist.as_deref(), Some("Rodrigo Zin feat. GEO"));
        assert_eq!(c[0].campos.album.as_deref(), Some("Invasão"));
        assert_eq!((c[0].campos.year, c[0].campos.track_number, c[0].campos.disc_number), (Some(2023), Some(4), Some(1)));
        assert_eq!(c[0].campos.genre.as_deref(), Some("hip hop"));
        assert!(c[1].pontuacao < c[0].pontuacao); // duração bem diferente
    }

    #[test]
    fn valida_e_grava_so_os_campos_escolhidos() {
        let conn = crate::db::open(Path::new(":memory:")).unwrap();
        conn.execute("INSERT INTO media (id, path, title, artist) VALUES (1, '/x.mp3', 'Obsessão', 'Rodrigo Zin')", []).unwrap();
        let db = std::sync::Mutex::new(conn);
        let campos = Campos { album: Some(" Invasão ".into()), year: Some(2023), title: Some("  ".into()), ..Default::default() };
        aplicar(&db, 1, &campos, false, None).unwrap();
        let conn = db.lock().unwrap();
        let a = atual(&conn, 1).unwrap();
        assert_eq!(a.campos.title.as_deref(), Some("Obsessão")); // vazio não apaga
        assert_eq!(a.campos.album.as_deref(), Some("Invasão"));
        assert_eq!(a.campos.year, Some(2023));
        let busca: String = conn.query_row("SELECT search_text FROM media WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert!(busca.contains("invasao"));
        drop(conn);
        assert!(aplicar(&db, 1, &Campos { year: Some(20), ..Default::default() }, false, None).is_err());
        assert!(aplicar(&db, 1, &Campos::default(), false, Some("5b11f4ce-a62d-471e-81fc-a69a8278c7da")).is_err());
        assert!(!release_valido("../../etc"));
    }

    /// Consulta real (precisa de internet): cargo test ensaio_rede -- --ignored --nocapture
    #[test]
    #[ignore]
    fn ensaio_rede() {
        let c = buscar("Bohemian Rhapsody", Some("Queen"), Some(354_000)).unwrap();
        for x in c.iter().take(5) {
            println!("{} | {:?} | {:?} | {:?} | {:?}", x.pontuacao, x.campos.artist, x.campos.album, x.campos.year, x.release_id);
        }
        assert!(!c.is_empty());
        let capa = baixar_capa(c.iter().find_map(|x| x.release_id.as_deref()).unwrap());
        println!("capa: {:?}", capa.as_ref().map(Vec::len));
    }

    #[test]
    fn grava_tags_no_arquivo() {
        // WAV gerado pelo GStreamer (como no teste do player); sem ele, o teste é pulado.
        let dir = std::env::temp_dir().join(format!("skald-tags-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("t.wav");
        let ok = std::process::Command::new("gst-launch-1.0")
            .args(["-q", "audiotestsrc", "num-buffers=10", "!", "wavenc", "!", "filesink"])
            .arg(format!("location={}", wav.display()))
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            return;
        }
        let campos = Campos { title: Some("Teste".into()), album: Some("Álbum".into()), year: Some(2024), track_number: Some(3), ..Default::default() };
        gravar_no_arquivo(&wav, &campos, None).unwrap();
        let lido = lofty::read_from_path(&wav).unwrap();
        use lofty::prelude::*;
        let tag = lido.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Teste"));
        assert_eq!(tag.album().as_deref(), Some("Álbum"));
        assert_eq!(tag.track(), Some(3));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
