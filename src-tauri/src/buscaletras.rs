//! Busca de letras no LRCLIB (lrclib.net): banco aberto e gratuito, sem cadastro.
//! Só roda quando o usuário pede; envia apenas título, artista, álbum e duração.

use serde::Serialize;
use serde_json::Value;

use crate::enriquecer::{agente, erro_rede};

const LRCLIB: &str = "https://lrclib.net/api";
/// Diferença máxima de duração para aceitar um resultado da busca aberta.
const TOLERANCIA_S: f64 = 3.0;

#[derive(Serialize, Debug, PartialEq)]
pub struct Encontrada {
    /// Letra com tempos (LRC) ou texto simples.
    pub texto: String,
    pub sincronizada: bool,
    pub instrumental: bool,
    pub titulo: String,
    pub artista: String,
}

fn ler(v: &Value) -> Option<Encontrada> {
    let texto_de = |k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let instrumental = v.get("instrumental").and_then(Value::as_bool).unwrap_or(false);
    let (texto, sincronizada) = match (texto_de("syncedLyrics"), texto_de("plainLyrics")) {
        (Some(s), _) => (s, true),
        (None, Some(p)) => (p, false),
        (None, None) if instrumental => (String::new(), false),
        _ => return None,
    };
    Some(Encontrada {
        texto,
        sincronizada,
        instrumental,
        titulo: texto_de("trackName").unwrap_or_default(),
        artista: texto_de("artistName").unwrap_or_default(),
    })
}

/// Melhor resultado da busca aberta: duração compatível (se conhecida), sincronizada antes.
fn escolher(lista: &[Value], duracao_s: Option<f64>) -> Option<Encontrada> {
    let compativel = |v: &&Value| match (duracao_s, v.get("duration").and_then(Value::as_f64)) {
        (Some(d), Some(x)) => (d - x).abs() <= TOLERANCIA_S,
        (Some(_), None) => false,
        (None, _) => true,
    };
    let candidatas: Vec<Encontrada> = lista.iter().filter(compativel).filter_map(ler).collect();
    let mut candidatas = candidatas.into_iter();
    let primeira = candidatas.next()?;
    if primeira.sincronizada {
        return Some(primeira);
    }
    Some(candidatas.find(|c| c.sincronizada).unwrap_or(primeira))
}

fn pedir(caminho: &str, params: &[(&str, String)]) -> Result<Option<Value>, String> {
    let mut req = agente().get(format!("{LRCLIB}/{caminho}"));
    for (k, v) in params {
        req = req.query(*k, v);
    }
    let texto = match req.call() {
        Ok(mut r) => r.body_mut().read_to_string().map_err(|e| erro_rede(e, "LRCLIB"))?,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(erro_rede(e, "LRCLIB")),
    };
    serde_json::from_str(&texto).map(Some).map_err(|_| "Resposta inesperada do LRCLIB.".to_string())
}

/// Procura a letra: primeiro a correspondência exata (com álbum e duração), depois a busca aberta.
pub fn buscar(titulo: &str, artista: &str, album: Option<&str>, duracao_ms: Option<i64>) -> Result<Option<Encontrada>, String> {
    let duracao_s = duracao_ms.filter(|&d| d > 0).map(|d| d as f64 / 1000.0);
    if let Some(d) = duracao_s {
        let mut p = vec![("track_name", titulo.to_string()), ("artist_name", artista.to_string()), ("duration", format!("{}", d.round() as i64))];
        if let Some(a) = album {
            p.push(("album_name", a.to_string()));
        }
        if let Some(v) = pedir("get", &p)? {
            if let Some(e) = ler(&v) {
                return Ok(Some(e));
            }
        }
    }
    let p = [("track_name", titulo.to_string()), ("artist_name", artista.to_string())];
    Ok(pedir("search", &p)?.and_then(|v| v.as_array().and_then(|l| escolher(l, duracao_s))))
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    #[test]
    fn prefere_sincronizada_com_duracao_compativel() {
        let lista = vec![
            json!({"trackName": "A", "artistName": "X", "duration": 200.0, "plainLyrics": "só texto", "syncedLyrics": null}),
            json!({"trackName": "A", "artistName": "X", "duration": 260.0, "syncedLyrics": "[00:01.00]longe"}),
            json!({"trackName": "A", "artistName": "X", "duration": 201.5, "syncedLyrics": "[00:01.00]perto"}),
        ];
        let e = escolher(&lista, Some(200.0)).unwrap();
        assert!(e.sincronizada);
        assert_eq!(e.texto, "[00:01.00]perto");
        // Sem nenhuma compatível, nada.
        assert!(escolher(&lista, Some(100.0)).is_none());
        // Sem duração conhecida, a primeira sincronizada.
        assert_eq!(escolher(&lista, None).unwrap().texto, "[00:01.00]longe");
    }

    #[test]
    fn instrumental_e_vazio() {
        let i = ler(&json!({"trackName": "I", "artistName": "X", "instrumental": true})).unwrap();
        assert!(i.instrumental && i.texto.is_empty());
        assert!(ler(&json!({"trackName": "V", "artistName": "X", "plainLyrics": "  "})).is_none());
    }

    /// Consulta real (rede): cargo test ensaio_lrclib -- --ignored --nocapture
    #[test]
    #[ignore]
    fn ensaio_lrclib() {
        let e = buscar("Paranoid Android", "Radiohead", Some("OK Computer"), Some(387_000)).unwrap().unwrap();
        println!("{} — {} (sincronizada: {})\n{}", e.titulo, e.artista, e.sincronizada, &e.texto[..e.texto.len().min(200)]);
        assert!(!e.texto.is_empty());
    }
}
