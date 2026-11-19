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
pub(crate) const CLIENTE: &str = "Skald/0.3.2 (reprodutor de musica local)";
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
    /// Achado só pelo título (sem artista): a tela mostra a sugestão desmarcada.
    pub incerto: bool,
    /// Tags da gravação (até 5, mais votadas primeiro), para as categorias.
    #[serde(skip)]
    pub tags: Vec<String>,
    /// Id do primeiro artista creditado (gêneros do artista, para as categorias).
    #[serde(skip)]
    pub artista_id: Option<String>,
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
    let tem_capa = crate::details::capa_salva(id).is_some()
        || (!missing && crate::details::capa_bruta(Path::new(&path)).is_some());
    Ok(Atual { campos, tem_capa, missing, duracao_ms })
}

pub(crate) fn agente() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .user_agent(CLIENTE)
        .build()
        .into()
}

pub(crate) fn erro_rede(e: ureq::Error, servico: &str) -> String {
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

/// Uma consulta ao MusicBrainz por vez, com 1,1 s entre elas (o serviço pede no máximo 1/s).
/// Com 503 (ocupado), tenta de novo mais 3 vezes, esperando 2, 4 e 8 s.
pub(crate) fn consultar_mb(url: &str, parametros: &[(&str, &str)]) -> Result<Value, String> {
    use std::sync::Mutex;
    use std::time::Instant;
    static ULTIMA: Mutex<Option<Instant>> = Mutex::new(None);
    let intervalo = Duration::from_millis(1100);
    for tentativa in 0..4u32 {
        {
            let mut ultima = ULTIMA.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(t) = *ultima {
                let passou = t.elapsed();
                if passou < intervalo {
                    std::thread::sleep(intervalo - passou);
                }
            }
            *ultima = Some(Instant::now());
        }
        let mut req = agente().get(url);
        for (k, v) in parametros {
            req = req.query(*k, *v);
        }
        match req.call() {
            Ok(mut r) => {
                let texto = r.body_mut().read_to_string().map_err(|e| erro_rede(e, "MusicBrainz"))?;
                return serde_json::from_str(&texto).map_err(|_| "Resposta inesperada do MusicBrainz.".to_string());
            }
            Err(ureq::Error::StatusCode(503)) if tentativa < 3 => std::thread::sleep(Duration::from_secs(2 << tentativa)),
            Err(e) => return Err(erro_rede(e, "MusicBrainz")),
        }
    }
    Err(erro_rede(ureq::Error::StatusCode(503), "MusicBrainz"))
}

/// Palavras de sobra entre parênteses/colchetes em nomes baixados ("(Official Video)", "[Lyrics]").
const SOBRAS: &[&str] = &[
    "official", "oficial", "video", "vídeo", "clipe", "audio", "áudio", "lyric", "letra", "legendado", "hd", "hq", "4k",
    "visualizer", "remaster",
];

/// Tira o número de faixa do começo ("02 - ", "11 ", "1-03. ", "07_") e sobras como "(Official Video)".
pub fn limpar_titulo(titulo: &str) -> String {
    let t = titulo.replace('_', " ");
    let mut t = t.trim();
    // Número de faixa (até 3 dígitos; "1-03" = disco-faixa) seguido de separador, ou de espaço
    // com até 2 dígitos. "1984" fica. A busca tenta o título original primeiro, então um número
    // que faz parte do nome ("99 Luftballons") ainda é achado.
    let n = t.chars().take_while(char::is_ascii_digit).count();
    if (1..=3).contains(&n) {
        let mut resto = &t[n..];
        if let Some(r) = resto.strip_prefix('-') {
            let m = r.chars().take_while(char::is_ascii_digit).count();
            if (1..=3).contains(&m) && r[m..].starts_with([' ', '.', '-']) {
                resto = &r[m..];
            }
        }
        let sem = resto.trim_start_matches(['.', ')', '-', '–', '—', ' ']);
        let separador = resto.trim_start().starts_with(['.', ')', '-', '–', '—']);
        let espaco = n <= 2 && resto.starts_with(' ');
        if !sem.is_empty() && (separador || espaco) {
            t = sem;
        }
    }
    // Parênteses/colchetes de sobra.
    let mut saida = String::new();
    let mut resto = t;
    while let Some(i) = resto.find(['(', '[']) {
        let fecha = if resto[i..].starts_with('(') { ')' } else { ']' };
        let Some(f) = resto[i..].find(fecha).map(|f| i + f) else { break };
        let dentro = crate::search::normalizar(&resto[i + 1..f]);
        let sobra = dentro.split(|c: char| !c.is_alphanumeric()).any(|p| SOBRAS.iter().any(|s| crate::search::normalizar(s) == p));
        saida.push_str(&resto[..i]);
        if !sobra {
            saida.push_str(&resto[i..=f]);
        }
        resto = &resto[f + 1..];
    }
    saida.push_str(resto);
    saida.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Formas de buscar uma música cujo título pode ser o nome do arquivo: a original; sem número
/// e sobras; e, se houver "A - B", artista A e música B (e o contrário).
pub fn variantes(titulo: &str, artista: Option<&str>) -> Vec<(String, Option<String>)> {
    let artista = artista.map(str::trim).filter(|a| !a.is_empty()).map(String::from);
    let mut v = vec![(titulo.trim().to_string(), artista.clone())];
    let limpo = limpar_titulo(titulo);
    let partes = ["-", "–", "—"].iter().find_map(|s| {
        let sep = format!(" {s} ");
        limpo.split_once(&sep).map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
    });
    match (&artista, partes) {
        (None, Some((a, b))) if !a.is_empty() && !b.is_empty() => {
            v.push((b.clone(), Some(a.clone())));
            v.push((a, Some(b)));
        }
        // Artista conhecido repetido no começo do título: "Queen - Bohemian Rhapsody".
        (Some(art), Some((a, b))) if crate::search::normalizar(&a) == crate::search::normalizar(art) => {
            v.push((b, artista.clone()));
        }
        _ => {}
    }
    if !limpo.is_empty() {
        v.push((limpo, artista));
    }
    let mut vistos = std::collections::HashSet::new();
    v.retain(|(t, a)| !t.is_empty() && vistos.insert((crate::search::normalizar(t), a.as_deref().map(crate::search::normalizar))));
    v
}

/// Candidato em que dá para confiar: semelhança ≥ 90 e duração a até 5 s (quando conhecida).
pub fn confiavel(c: &Candidato, duracao_ms: Option<i64>) -> bool {
    c.pontuacao >= 90
        && match (c.duracao_ms, duracao_ms) {
            (Some(a), Some(b)) => (a - b).abs() <= 5000,
            _ => true,
        }
}

/// Busca tentando as `variantes` até achar um candidato confiável. Sem nenhum confiável,
/// devolve a primeira lista não vazia.
pub fn buscar_flexivel(titulo: &str, artista: Option<&str>, duracao_ms: Option<i64>) -> Result<Vec<Candidato>, String> {
    let mut reserva: Option<Vec<Candidato>> = None;
    let mut erro = None;
    for (t, a) in variantes(titulo, artista) {
        let so_titulo = a.is_none();
        match buscar(&t, a.as_deref(), duracao_ms).map(|mut c| {
            c.iter_mut().for_each(|x| x.incerto = so_titulo);
            c
        }) {
            Ok(c) if c.iter().any(|x| confiavel(x, duracao_ms)) => return Ok(c),
            Ok(c) => {
                if reserva.is_none() && !c.is_empty() {
                    reserva = Some(c);
                }
            }
            Err(e) if e.starts_with("Sem conexão") => return Err(e),
            Err(e) => erro = erro.or(Some(e)),
        }
    }
    match (reserva, erro) {
        (Some(c), _) => Ok(c),
        (None, Some(e)) => Err(e),
        (None, None) => Ok(Vec::new()),
    }
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
    let json = consultar_mb(MUSICBRAINZ, &[("query", &consulta), ("fmt", "json"), ("limit", "10")])?;
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
        // Tags por votos; gênero = a mais votada.
        let mut votadas: Vec<(i64, String)> = rec["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| Some((t["count"].as_i64().unwrap_or(0), t["name"].as_str()?.to_string())))
            .collect();
        votadas.sort_by_key(|(n, _)| -n);
        let tags: Vec<String> = votadas.into_iter().take(5).map(|(_, t)| t).collect();
        let genero = tags.first().cloned();
        let artista_id = rec["artist-credit"][0]["artist"]["id"].as_str().map(String::from);
        let lancamentos: Vec<&Value> = rec["releases"].as_array().map(|r| r.iter().take(3).collect()).unwrap_or_default();
        let base = Campos { title: titulo.clone(), artist: artista.clone(), genre: genero.clone(), ..Default::default() };
        if lancamentos.is_empty() {
            lista.push(Candidato { campos: base, pontuacao, duracao_ms: duracao, release_id: None, incerto: false, tags: tags.clone(), artista_id: artista_id.clone() });
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
                incerto: false,
                tags: tags.clone(),
                artista_id: artista_id.clone(),
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
    let capa = release_capa.map(baixar_capa).transpose()?;
    if no_arquivo {
        if !Path::new(&path).is_file() {
            return Err("Arquivo não encontrado: as informações não podem ser gravadas nele.".into());
        }
        let embutiu = capa.is_some();
        gravar_no_arquivo(Path::new(&path), &c, capa)?;
        // A capa agora está no arquivo: a guardada no Skáld não precisa mais valer por cima.
        if embutiu {
            crate::details::remover_capa_salva(id);
        }
    } else if let Some(bytes) = capa {
        // Só no Skáld: a capa fica na pasta de dados do app e vale por cima da do arquivo.
        crate::details::salvar_capa(id, &bytes)?;
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

/// Nome de arquivo seguro em Linux, Windows e Android: sem `/\\:*?"<>|` nem controles, até 150 caracteres.
fn nome_seguro(texto: &str) -> String {
    let limpo: String = texto
        .chars()
        .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { ' ' } else { c })
        .collect();
    let limpo = limpo.split_whitespace().collect::<Vec<_>>().join(" ");
    let limpo = limpo.trim_matches(['.', ' ']);
    limpo.chars().take(150).collect()
}

/// Renomeia o arquivo da música para "Artista - Título.ext" (na mesma pasta), com o que está no
/// banco. Nunca sobrescreve; o `.lrc` de mesmo nome vai junto. O registro (playlists, histórico,
/// curtidas) segue o mesmo: só o caminho muda. Devolve o novo caminho (ou o atual, se já estava certo).
pub fn renomear_arquivo(db: &std::sync::Mutex<Connection>, id: i64) -> Result<String, String> {
    let conn = db.lock().map_err(|_| "Banco indisponível.".to_string())?;
    let (caminho, titulo, artista): (String, Option<String>, Option<String>) = conn
        .query_row("SELECT path, title, artist FROM media WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|_| "Música não encontrada.".to_string())?;
    let titulo = titulo.map(|t| nome_seguro(&t)).filter(|t| !t.is_empty()).ok_or("A música não tem título.")?;
    let base = match artista.map(|a| nome_seguro(&a)).filter(|a| !a.is_empty()) {
        Some(a) => format!("{a} - {titulo}"),
        None => titulo,
    };
    let antigo = Path::new(&caminho);
    let ext = antigo.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let novo = antigo.with_file_name(format!("{base}{ext}"));
    if novo == antigo {
        return Ok(caminho);
    }
    // Mesmo arquivo com outra caixa (Windows) pode; outro arquivo com esse nome, nunca.
    let mesmo = novo.to_string_lossy().to_lowercase() == caminho.to_lowercase();
    if novo.exists() && !mesmo {
        return Err(format!("Já existe um arquivo chamado \"{}\".", novo.file_name().unwrap_or_default().to_string_lossy()));
    }
    std::fs::rename(antigo, &novo).map_err(|e| format!("Não foi possível renomear: {e}"))?;
    let novo_txt = novo.to_string_lossy().into_owned();
    if let Err(e) = conn.execute("UPDATE media SET path = ?2 WHERE id = ?1", params![id, novo_txt]) {
        // Banco recusou (caminho já registrado?): desfaz para não perder a ligação.
        let _ = std::fs::rename(&novo, antigo);
        return Err(e.to_string());
    }
    let lrc = antigo.with_extension("lrc");
    let lrc_novo = novo.with_extension("lrc");
    if lrc.exists() && !lrc_novo.exists() {
        let _ = std::fs::rename(&lrc, &lrc_novo);
    }
    Ok(novo_txt)
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
    fn renomeia_sem_sobrescrever() {
        let pasta = crate::teste_util::pasta("skald-renomear");
        let velho = pasta.join("02 - x.mp3");
        std::fs::write(&velho, b"a").unwrap();
        std::fs::write(pasta.join("02 - x.lrc"), b"[00:01]oi").unwrap();
        std::fs::write(pasta.join("Outro - Y.mp3"), b"b").unwrap();
        let conn = crate::db::open(Path::new(":memory:")).unwrap();
        conn.execute("INSERT INTO media (id, path, title, artist) VALUES (1, ?1, 'Música: A/B', 'Fulano')", [velho.to_string_lossy()]).unwrap();
        conn.execute("INSERT INTO media (id, path, title, artist) VALUES (2, ?1, 'Y', 'Outro')", [pasta.join("y.mp3").to_string_lossy()]).unwrap();
        std::fs::write(pasta.join("y.mp3"), b"c").unwrap();
        let db = std::sync::Mutex::new(conn);
        let novo = renomear_arquivo(&db, 1).unwrap();
        assert!(novo.ends_with("Fulano - Música A B.mp3"), "{novo}");
        assert!(!velho.exists() && Path::new(&novo).exists());
        assert!(pasta.join("Fulano - Música A B.lrc").exists());
        assert!(renomear_arquivo(&db, 2).is_err()); // "Outro - Y.mp3" já existe
        assert!(pasta.join("y.mp3").exists());
        let p: String = db.lock().unwrap().query_row("SELECT path FROM media WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(p, novo);
    }

    #[test]
    fn limpa_nomes_de_arquivo() {
        assert_eq!(limpar_titulo("02 - I'm Just a Prisoner"), "I'm Just a Prisoner");
        assert_eq!(limpar_titulo("11 Def Leppard - Armageddon"), "Def Leppard - Armageddon");
        assert_eq!(limpar_titulo("1-03. Faixa"), "Faixa");
        assert_eq!(limpar_titulo("07_Garota_de_Ipanema"), "Garota de Ipanema");
        assert_eq!(limpar_titulo("1984"), "1984");
        assert_eq!(limpar_titulo("Igris - Requiém das Rosas (Official Video) [HD]"), "Igris - Requiém das Rosas");
        assert_eq!(limpar_titulo("Deixe Me Ir (Acústico)"), "Deixe Me Ir (Acústico)");
        let v = variantes("11 Def Leppard - Armageddon", None);
        assert_eq!(v[0], ("11 Def Leppard - Armageddon".into(), None));
        assert_eq!(v[1], ("Armageddon".into(), Some("Def Leppard".into())));
        assert_eq!(v[2], ("Def Leppard".into(), Some("Armageddon".into())));
        assert_eq!(v[3], ("Def Leppard - Armageddon".into(), None));
        let v = variantes("Queen - Bohemian Rhapsody", Some("Queen"));
        assert_eq!(v[1], ("Bohemian Rhapsody".into(), Some("Queen".into())));
        assert_eq!(variantes("Garota de Ipanema", Some("Tom Jobim")).len(), 1);
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
        for nome in ["11 Def Leppard - Armageddon It", "02 - I'm Just a Prisoner", "Igris - Requiém das Rosas"] {
            let c = buscar_flexivel(nome, None, None).unwrap_or_default();
            println!("{nome} => {:?}", c.first().map(|x| (x.pontuacao, &x.campos.title, &x.campos.artist)));
        }
    }

    fn confere_tags(wav: &Path) {
        let lido = lofty::read_from_path(wav).unwrap();
        use lofty::prelude::*;
        let tag = lido.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Teste"));
        assert_eq!(tag.album().as_deref(), Some("Álbum"));
        assert_eq!(tag.track(), Some(3));
    }

    fn campos_teste() -> Campos {
        Campos { title: Some("Teste".into()), album: Some("Álbum".into()), year: Some(2024), track_number: Some(3), ..Default::default() }
    }

    #[test]
    fn grava_tags_no_arquivo() {
        let dir = crate::teste_util::pasta("skald tags ção");
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("t.wav");
        crate::teste_util::wav(&wav, 4410, 0);
        gravar_no_arquivo(&wav, &campos_teste(), None).unwrap();
        confere_tags(&wav);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No Windows o arquivo aberto pelo player pode ficar travado para escrita.
    #[test]
    fn grava_tags_enquanto_toca() {
        let dir = crate::teste_util::pasta("skald tags tocando");
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("tocando.wav");
        crate::teste_util::wav(&wav, 44_100 * 6, 3);
        let (tx, rx) = std::sync::mpsc::channel();
        let p = crate::player::Player::new(move |e| { let _ = tx.send(e); }, || None).unwrap();
        p.silenciar_para_teste();
        p.tocar(1, &wav.to_string_lossy(), crate::ganho::Ganho::default()).unwrap();
        loop {
            let e = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("o player não começou");
            assert!(e.erro.is_none(), "{:?}", e.erro);
            if e.estado == crate::player::Estado::Tocando && e.posicao_ms > 500 {
                break;
            }
        }
        let gravou = gravar_no_arquivo(&wav, &campos_teste(), None);
        drop(p);
        gravou.expect("gravar tags com a música tocando");
        confere_tags(&wav);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
