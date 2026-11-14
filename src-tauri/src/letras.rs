//! Letras (seção 9): sincronizadas de um `.lrc` ao lado da música; estáticas da tag do
//! arquivo. Uma letra na tag em formato LRC também conta como sincronizada.

use std::path::{Path, PathBuf};

use lofty::prelude::*;
use lofty::tag::ItemKey;
use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
pub struct Linha {
    pub ms: u64,
    pub texto: String,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum Letra {
    Sincronizada { linhas: Vec<Linha>, origem: String },
    Estatica { texto: String, origem: String },
}

/// "[mm:ss]", "[mm:ss.xx]", "[mm:ss.xxx]" ou "[mm:ss:xx]" → ms.
fn tempo(s: &str) -> Option<u64> {
    let (min, resto) = s.split_once(':')?;
    let min: u64 = min.trim().parse().ok()?;
    let (seg, fracao) = match resto.split_once(['.', ':']) {
        Some((s, f)) => (s, f),
        None => (resto, ""),
    };
    let seg: u64 = seg.trim().parse().ok()?;
    let fracao_ms = if fracao.is_empty() {
        0
    } else {
        let f: u64 = fracao.trim().parse().ok()?;
        match fracao.trim().len() {
            1 => f * 100,
            2 => f * 10,
            _ => f,
        }
    };
    (seg < 60).then_some(min * 60_000 + seg * 1000 + fracao_ms)
}

/// Tira marcas por palavra ("<00:12.34>") do LRC estendido.
fn sem_marcas_de_palavra(s: &str) -> String {
    let mut saida = String::new();
    let mut resto = s;
    while let Some(i) = resto.find('<') {
        match resto[i..].find('>') {
            Some(j) if tempo(&resto[i + 1..i + j]).is_some() => {
                saida.push_str(&resto[..i]);
                resto = &resto[i + j + 1..];
            }
            _ => {
                saida.push_str(&resto[..=i]);
                resto = &resto[i + 1..];
            }
        }
    }
    saida.push_str(resto);
    saida.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Interpreta texto LRC. `None` se não houver nenhuma linha com tempo.
pub fn ler_lrc(texto: &str) -> Option<Vec<Linha>> {
    let mut offset: i64 = 0;
    let mut linhas = Vec::new();
    for linha in texto.lines() {
        let mut resto = linha.trim();
        let mut tempos = Vec::new();
        while let Some(depois) = resto.strip_prefix('[') {
            let Some(fim) = depois.find(']') else { break };
            let dentro = &depois[..fim];
            if let Some(ms) = tempo(dentro) {
                tempos.push(ms);
            } else if let Some(v) = dentro.strip_prefix("offset:") {
                offset = v.trim().parse().unwrap_or(0);
            }
            resto = depois[fim + 1..].trim_start();
        }
        let texto = sem_marcas_de_palavra(resto);
        for ms in tempos {
            linhas.push(Linha { ms, texto: texto.clone() });
        }
    }
    if linhas.is_empty() {
        return None;
    }
    // Offset positivo adianta a letra.
    for l in &mut linhas {
        l.ms = (l.ms as i64 - offset).max(0) as u64;
    }
    linhas.sort_by_key(|l| l.ms);
    Some(linhas)
}

/// Texto em UTF-8 (sem BOM) ou, se inválido, Latin-1.
fn decodificar(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// `musica.lrc` (extensão sem diferenciar maiúsculas) na mesma pasta.
fn achar_lrc(musica: &Path) -> Option<PathBuf> {
    let nome = musica.file_stem()?;
    std::fs::read_dir(musica.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_stem() == Some(nome)
                && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lrc"))
                && p.is_file()
        })
}

fn da_tag(musica: &Path) -> Option<String> {
    let arquivo = lofty::read_from_path(musica).ok()?;
    arquivo.tags().iter().find_map(|t| {
        [ItemKey::UnsyncLyrics, ItemKey::Lyrics]
            .into_iter()
            .find_map(|k| t.get_string(k))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

pub fn carregar(musica: &Path) -> Option<Letra> {
    if let Some(lrc) = achar_lrc(musica) {
        if let Some(linhas) = std::fs::read(&lrc).ok().and_then(|b| ler_lrc(&decodificar(&b))) {
            return Some(Letra::Sincronizada { linhas, origem: "arquivo .lrc".into() });
        }
    }
    let texto = da_tag(musica)?;
    Some(match ler_lrc(&texto) {
        Some(linhas) => Letra::Sincronizada { linhas, origem: "tag do arquivo".into() },
        None => Letra::Estatica { texto: texto.replace("\r\n", "\n"), origem: "tag do arquivo".into() },
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn l(ms: u64, texto: &str) -> Linha {
        Linha { ms, texto: texto.into() }
    }

    #[test]
    fn le_tempos() {
        assert_eq!(tempo("01:02.5"), Some(62_500));
        assert_eq!(tempo("01:02.50"), Some(62_500));
        assert_eq!(tempo("01:02.500"), Some(62_500));
        assert_eq!(tempo("1:02:50"), Some(62_500));
        assert_eq!(tempo("00:07"), Some(7_000));
        assert_eq!(tempo("ar:Djavan"), None);
        assert_eq!(tempo("00:75"), None);
    }

    #[test]
    fn le_lrc_completo() {
        let lrc = "[ar:Djavan]\n[ti:Oceano]\n[offset:500]\n\n\
                   [00:12.00]Assim que o dia amanheceu\n\
                   [00:20.50][01:10.00]Lá no mar alto da paixão\n\
                   [00:30.00]<00:30.00>Dava <00:30.50>pra <00:31.00>ver\n\
                   [00:40.00]\n";
        assert_eq!(
            ler_lrc(lrc).unwrap(),
            vec![
                l(11_500, "Assim que o dia amanheceu"),
                l(20_000, "Lá no mar alto da paixão"),
                l(29_500, "Dava pra ver"),
                l(39_500, ""),
                l(69_500, "Lá no mar alto da paixão"),
            ]
        );
        assert_eq!(ler_lrc("Só texto\nsem tempo"), None);
    }

    #[test]
    fn acha_lrc_ao_lado_e_decodifica_latin1() {
        let dir = std::env::temp_dir().join(format!("skald-teste-letras-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let musica = dir.join("Canção.flac");
        std::fs::write(&musica, b"").unwrap();
        assert_eq!(carregar(&musica), None);
        // "[00:01.00]Coração" em Latin-1, com extensão em maiúsculas.
        std::fs::write(dir.join("Canção.LRC"), b"[00:01.00]Cora\xe7\xe3o").unwrap();
        assert_eq!(
            carregar(&musica),
            Some(Letra::Sincronizada { linhas: vec![l(1000, "Coração")], origem: "arquivo .lrc".into() })
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
