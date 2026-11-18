//! Assinatura de podcasts por RSS (etapa 59): ler o feed e baixar episódios.
//! A internet só é usada quando a pessoa assina, atualiza ou baixa.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Programa lido do feed.
#[derive(Debug, Default)]
pub struct Feed {
    pub titulo: String,
    pub autor: Option<String>,
    pub descricao: Option<String>,
    pub imagem: Option<String>,
    pub itens: Vec<Item>,
}

/// Episódio do feed (só os que têm áudio anexado).
#[derive(Debug, Default, Clone)]
pub struct Item {
    pub guid: String,
    pub titulo: String,
    pub publicado_em: Option<i64>,
    pub descricao: Option<String>,
    pub url: String,
    pub tipo: Option<String>,
    pub tamanho: Option<i64>,
    pub duracao_ms: Option<i64>,
}

const ITUNES: &str = "http://www.itunes.com/dtds/podcast-1.0.dtd";
const ATOM: &str = "http://www.w3.org/2005/Atom";

fn texto(n: roxmltree::Node) -> Option<String> {
    let t: String = n.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect();
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn filho<'a, 'i>(n: roxmltree::Node<'a, 'i>, nome: &str, ns: Option<&str>) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == nome && c.tag_name().namespace() == ns)
}

/// Tira marcação HTML simples das descrições (muitos feeds mandam HTML).
fn sem_html(s: &str) -> String {
    let mut saida = String::with_capacity(s.len());
    let mut dentro = false;
    for c in s.chars() {
        match c {
            '<' => dentro = true,
            '>' if dentro => {
                dentro = false;
                saida.push(' ');
            }
            _ if !dentro => saida.push(c),
            _ => {}
        }
    }
    let s = saida
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Lê um feed RSS 2.0 (com as marcas do iTunes) ou Atom.
pub fn ler(xml: &str) -> Result<Feed, String> {
    let doc = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() },
    )
    .map_err(|e| format!("O endereço não trouxe um feed válido ({e})."))?;
    let raiz = doc.root_element();
    if raiz.tag_name().name() == "feed" {
        return ler_atom(raiz);
    }
    let canal = filho(raiz, "channel", None).ok_or("O endereço não é um feed de podcast (RSS).")?;
    let mut feed = Feed {
        titulo: filho(canal, "title", None).and_then(texto).unwrap_or_else(|| "Podcast".into()),
        autor: filho(canal, "author", Some(ITUNES)).and_then(texto),
        descricao: filho(canal, "summary", Some(ITUNES))
            .or_else(|| filho(canal, "description", None))
            .and_then(texto)
            .map(|d| sem_html(&d)),
        imagem: filho(canal, "image", Some(ITUNES))
            .and_then(|i| i.attribute("href").map(str::to_string))
            .or_else(|| filho(canal, "image", None).and_then(|i| filho(i, "url", None)).and_then(texto)),
        itens: Vec::new(),
    };
    for item in canal.children().filter(|c| c.is_element() && c.tag_name().name() == "item") {
        let Some(anexo) = filho(item, "enclosure", None) else { continue };
        let Some(url) = anexo.attribute("url").map(|u| u.trim().to_string()).filter(|u| !u.is_empty()) else { continue };
        let tipo = anexo.attribute("type").map(str::to_string);
        if tipo.as_deref().is_some_and(|t| t.starts_with("video/")) {
            continue;
        }
        feed.itens.push(Item {
            guid: filho(item, "guid", None).and_then(texto).unwrap_or_else(|| url.clone()),
            titulo: filho(item, "title", None).and_then(texto).unwrap_or_else(|| "Episódio".into()),
            publicado_em: filho(item, "pubDate", None).and_then(texto).and_then(|d| data_rfc2822(&d)),
            descricao: filho(item, "summary", Some(ITUNES))
                .or_else(|| filho(item, "description", None))
                .and_then(texto)
                .map(|d| sem_html(&d)),
            tamanho: anexo.attribute("length").and_then(|l| l.trim().parse().ok()).filter(|&n: &i64| n > 0),
            duracao_ms: filho(item, "duration", Some(ITUNES)).and_then(texto).and_then(|d| duracao(&d)),
            url,
            tipo,
        });
    }
    Ok(feed)
}

fn ler_atom(raiz: roxmltree::Node) -> Result<Feed, String> {
    let ns = Some(ATOM);
    let mut feed = Feed {
        titulo: filho(raiz, "title", ns).and_then(texto).unwrap_or_else(|| "Podcast".into()),
        autor: filho(raiz, "author", ns).and_then(|a| filho(a, "name", ns)).and_then(texto),
        descricao: filho(raiz, "subtitle", ns).and_then(texto),
        imagem: filho(raiz, "logo", ns).or_else(|| filho(raiz, "icon", ns)).and_then(texto),
        itens: Vec::new(),
    };
    for e in raiz.children().filter(|c| c.is_element() && c.tag_name().name() == "entry") {
        let Some(anexo) = e
            .children()
            .find(|c| c.tag_name().name() == "link" && c.attribute("rel") == Some("enclosure"))
        else {
            continue;
        };
        let Some(url) = anexo.attribute("href").map(str::to_string) else { continue };
        feed.itens.push(Item {
            guid: filho(e, "id", ns).and_then(texto).unwrap_or_else(|| url.clone()),
            titulo: filho(e, "title", ns).and_then(texto).unwrap_or_else(|| "Episódio".into()),
            publicado_em: filho(e, "published", ns)
                .or_else(|| filho(e, "updated", ns))
                .and_then(texto)
                .and_then(|d| data_iso(&d)),
            descricao: filho(e, "summary", ns).and_then(texto).map(|d| sem_html(&d)),
            tipo: anexo.attribute("type").map(str::to_string),
            tamanho: anexo.attribute("length").and_then(|l| l.parse().ok()),
            duracao_ms: None,
            url,
        });
    }
    Ok(feed)
}

/// "01:02:03", "62:03" ou "3723" (segundos) → ms.
fn duracao(s: &str) -> Option<i64> {
    let partes: Vec<f64> = s.trim().split(':').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let seg = partes.iter().fold(0.0, |t, p| t * 60.0 + p);
    (seg > 0.0 && partes.len() <= 3).then_some((seg * 1000.0) as i64)
}

fn dias(ano: i64, mes: i64, dia: i64) -> i64 {
    crate::podcasts::dias_desde_1970(ano, mes, dia)
}

/// "Tue, 17 Nov 2026 10:00:00 +0000" (pubDate do RSS) → segundos Unix.
fn data_rfc2822(s: &str) -> Option<i64> {
    let s = s.split_once(',').map_or(s, |(_, r)| r).trim();
    let mut p = s.split_whitespace();
    let dia: i64 = p.next()?.parse().ok()?;
    let mes = match p.next()?.get(..3)?.to_ascii_lowercase().as_str() {
        "jan" => 1, "feb" => 2, "mar" => 3, "apr" => 4, "may" => 5, "jun" => 6,
        "jul" => 7, "aug" => 8, "sep" => 9, "oct" => 10, "nov" => 11, "dec" => 12,
        _ => return None,
    };
    let mut ano: i64 = p.next()?.parse().ok()?;
    if ano < 100 {
        ano += 2000;
    }
    let hora = p.next().unwrap_or("00:00:00");
    let hms: Vec<i64> = hora.split(':').map(|x| x.parse().unwrap_or(0)).collect();
    let segs = hms.first().unwrap_or(&0) * 3600 + hms.get(1).unwrap_or(&0) * 60 + hms.get(2).unwrap_or(&0);
    let fuso = match p.next().unwrap_or("+0000") {
        "GMT" | "UT" | "UTC" | "Z" => 0,
        "EST" => -5 * 3600, "EDT" => -4 * 3600, "CST" => -6 * 3600, "CDT" => -5 * 3600,
        "MST" => -7 * 3600, "MDT" => -6 * 3600, "PST" => -8 * 3600, "PDT" => -7 * 3600,
        z if z.len() == 5 && (z.starts_with('+') || z.starts_with('-')) => {
            let n: i64 = z[1..].parse().ok()?;
            let s = (n / 100) * 3600 + (n % 100) * 60;
            if z.starts_with('-') { -s } else { s }
        }
        _ => 0,
    };
    Some(dias(ano, mes, dia) * 86_400 + segs - fuso)
}

/// "2026-11-17T10:00:00Z" ou com fuso "-03:00" (Atom) → segundos Unix.
fn data_iso(s: &str) -> Option<i64> {
    let s = s.trim();
    let ano = s.get(0..4)?.parse().ok()?;
    let mes = s.get(5..7)?.parse().ok()?;
    let dia = s.get(8..10)?.parse().ok()?;
    let mut segs = 0;
    let mut fuso = 0;
    if let Some(h) = s.get(11..19) {
        let hms: Vec<i64> = h.split(':').map(|x| x.parse().unwrap_or(0)).collect();
        segs = hms[0] * 3600 + hms.get(1).unwrap_or(&0) * 60 + hms.get(2).unwrap_or(&0);
        let resto = s[19..].trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
        if resto.len() >= 6 && (resto.starts_with('+') || resto.starts_with('-')) {
            let h: i64 = resto[1..3].parse().ok()?;
            let m: i64 = resto[4..6].parse().ok()?;
            fuso = (h * 3600 + m * 60) * if resto.starts_with('-') { -1 } else { 1 };
        }
    }
    Some(dias(ano, mes, dia) * 86_400 + segs - fuso)
}

fn agente(timeout_global: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(timeout_global)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .user_agent(crate::enriquecer::CLIENTE)
        .build()
        .into()
}

fn erro(e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(404) => "O endereço não foi encontrado (404).".into(),
        e => crate::enriquecer::erro_rede(e, "servidor do podcast"),
    }
}

/// Baixa e lê o feed.
pub fn buscar(url: &str) -> Result<Feed, String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("Cole o endereço completo do feed (começando com https://).".into());
    }
    let mut r = agente(Some(Duration::from_secs(60))).get(url).call().map_err(erro)?;
    let xml = r
        .body_mut()
        .with_config()
        .limit(50 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| format!("Não foi possível ler o feed: {e}"))?;
    ler(&xml)
}

/// Baixa uma imagem (capa do programa), até 10 MB.
pub fn baixar_imagem(url: &str) -> Result<Vec<u8>, String> {
    let mut r = agente(Some(Duration::from_secs(60))).get(url).call().map_err(erro)?;
    r.body_mut().with_config().limit(10 * 1024 * 1024).read_to_vec().map_err(|e| e.to_string())
}

/// Extensão do arquivo de áudio pela URL ou pelo tipo.
pub fn extensao(url: &str, tipo: Option<&str>) -> &'static str {
    let caminho = url.split(['?', '#']).next().unwrap_or(url).to_ascii_lowercase();
    for e in ["mp3", "m4a", "aac", "ogg", "opus", "oga", "flac", "wav"] {
        if caminho.ends_with(&format!(".{e}")) {
            return e;
        }
    }
    match tipo.unwrap_or("") {
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => "m4a",
        "audio/aac" => "aac",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/flac" => "flac",
        "audio/wav" | "audio/x-wav" => "wav",
        _ => "mp3",
    }
}

/// Baixa `url` para `destino` (via `.parte`, renomeado no fim). `progresso(feito, total)`;
/// devolver `false` cancela. Nada sobrescreve um arquivo que já exista.
pub fn baixar(
    url: &str,
    destino: &Path,
    mut progresso: impl FnMut(u64, Option<u64>) -> bool,
) -> Result<PathBuf, String> {
    if let Some(pasta) = destino.parent() {
        std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
    }
    let parte = destino.with_extension(format!(
        "{}.parte",
        destino.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    let r = (|| {
        let mut resposta = agente(None).get(url).call().map_err(erro)?;
        let total = resposta.body().content_length();
        let mut leitor = resposta.body_mut().with_config().limit(u64::MAX).reader();
        let mut arquivo = std::fs::File::create(&parte).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 256 * 1024];
        let mut feito = 0u64;
        loop {
            let n = leitor.read(&mut buf).map_err(|e| format!("A conexão caiu: {e}"))?;
            if n == 0 {
                break;
            }
            arquivo.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            feito += n as u64;
            if !progresso(feito, total) {
                return Err("Download cancelado.".to_string());
            }
        }
        arquivo.sync_all().map_err(|e| e.to_string())?;
        drop(arquivo);
        if feito == 0 {
            return Err("O servidor mandou um arquivo vazio.".into());
        }
        if destino.exists() {
            return Err("Já existe um arquivo com esse nome.".into());
        }
        std::fs::rename(&parte, destino).map_err(|e| e.to_string())
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&parte);
    }
    r.map(|()| destino.to_path_buf())
}

#[cfg(test)]
mod testes {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
  <channel>
    <title>Café com Código</title>
    <itunes:author>Fulana</itunes:author>
    <description><![CDATA[<p>Um <b>podcast</b> &amp; tal</p>]]></description>
    <itunes:image href="https://exemplo.com/capa.jpg"/>
    <item>
      <title>Ep. 2 — Rust</title>
      <guid isPermaLink="false">abc-2</guid>
      <pubDate>Tue, 17 Nov 2026 10:00:00 -0300</pubDate>
      <enclosure url="https://cdn.exemplo.com/ep2.mp3?x=1" length="1234" type="audio/mpeg"/>
      <itunes:duration>01:02:03</itunes:duration>
    </item>
    <item>
      <title>Ep. 1</title>
      <pubDate>Mon, 02 Nov 2026 08:30:00 GMT</pubDate>
      <enclosure url="https://cdn.exemplo.com/ep1" type="audio/x-m4a"/>
      <itunes:duration>754</itunes:duration>
    </item>
    <item><title>Sem áudio</title></item>
  </channel>
</rss>"#;

    #[test]
    fn le_rss_com_itunes() {
        let f = ler(RSS).unwrap();
        assert_eq!(f.titulo, "Café com Código");
        assert_eq!(f.autor.as_deref(), Some("Fulana"));
        assert_eq!(f.descricao.as_deref(), Some("Um podcast & tal"));
        assert_eq!(f.imagem.as_deref(), Some("https://exemplo.com/capa.jpg"));
        assert_eq!(f.itens.len(), 2);
        let e = &f.itens[0];
        assert_eq!((e.guid.as_str(), e.titulo.as_str()), ("abc-2", "Ep. 2 — Rust"));
        assert_eq!(e.duracao_ms, Some(3_723_000));
        assert_eq!(e.tamanho, Some(1234));
        // 17/11/2026 13:00 UTC.
        assert_eq!(e.publicado_em, Some(20_774 * 86_400 + 13 * 3600));
        // Sem guid: a URL do áudio.
        assert_eq!(f.itens[1].guid, "https://cdn.exemplo.com/ep1");
        assert_eq!(f.itens[1].duracao_ms, Some(754_000));
        assert_eq!(extensao(&f.itens[0].url, f.itens[0].tipo.as_deref()), "mp3");
        assert_eq!(extensao(&f.itens[1].url, f.itens[1].tipo.as_deref()), "m4a");
    }

    #[test]
    fn le_atom_e_datas() {
        let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>A</title>
            <entry><id>t1</id><title>E</title><published>2026-11-17T10:00:00-03:00</published>
            <link rel="enclosure" href="https://x/e.ogg" type="audio/ogg"/></entry></feed>"#;
        let f = ler(atom).unwrap();
        assert_eq!(f.itens[0].publicado_em, Some(20_774 * 86_400 + 13 * 3600));
        assert_eq!(data_rfc2822("17 Nov 2026 13:00:00 Z"), Some(20_774 * 86_400 + 13 * 3600));
        assert!(ler("<html><body>oi</body></html>").is_err());
        assert!(ler("não é xml").is_err());
    }

    /// Rede de verdade: `cargo test ensaio_rss -- --ignored --nocapture` (SKALD_FEED troca o feed).
    #[test]
    #[ignore]
    fn ensaio_rss() {
        let url = std::env::var("SKALD_FEED").unwrap_or_else(|_| "https://feeds.npr.org/510289/podcast.xml".into());
        let f = buscar(&url).unwrap();
        println!("{} · {:?} · {} episódios · capa {:?}", f.titulo, f.autor, f.itens.len(), f.imagem);
        let e = &f.itens[0];
        println!("1º: {} · {:?} · {:?} ms · {}", e.titulo, e.publicado_em, e.duracao_ms, e.url);
        let dir = crate::teste_util::pasta("skald-ensaio-rss");
        let destino = dir.join("ep.mp3");
        let r = baixar(&e.url, &destino, |feito, total| {
            if feito >= 1 << 20 {
                println!("1 MB de {total:?} bytes: cancela");
                return false;
            }
            true
        });
        assert_eq!(r.unwrap_err(), "Download cancelado.");
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none(), ".parte apagado");
        if let Some(img) = &f.imagem {
            println!("capa: {} bytes", baixar_imagem(img).unwrap().len());
        }
    }
}
