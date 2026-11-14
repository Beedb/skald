//! Créditos com vários artistas ("Rodrigo Zin/1LUM3/GEO", "X feat. Y", "X part. Y"):
//! cada participante conta como artista na coleção e nas estatísticas. O texto gravado
//! na música nunca muda; a divisão é só na hora de agrupar.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;

/// Separadores entre artistas, sem diferenciar maiúsculas (os com espaço vêm cercados de espaço).
const SEPARADORES_PALAVRA: [&str; 6] = ["feat.", "feat", "ft.", "featuring", "part.", "participação"];

/// Divide um crédito nos artistas que o compõem, sem repetir (comparação sem acento).
/// "AC/DC" e afins (todas as partes com até 3 letras) não são divididos na barra.
pub fn separar(texto: &str) -> Vec<String> {
    let mut partes: Vec<String> = Vec::new();
    for pedaco in texto.split(';') {
        for p in separar_barra(pedaco) {
            for nome in separar_palavras(&p) {
                let nome = nome.trim().trim_matches(|c| c == '(' || c == ')' || c == '[' || c == ']').trim();
                if nome.is_empty() {
                    continue;
                }
                let chave = crate::search::normalizar(nome);
                if !partes.iter().any(|x| crate::search::normalizar(x) == chave) {
                    partes.push(nome.to_string());
                }
            }
        }
    }
    partes
}

fn separar_barra(texto: &str) -> Vec<String> {
    let partes: Vec<&str> = texto.split('/').map(str::trim).collect();
    if partes.len() > 1 && partes.iter().all(|p| p.chars().count() <= 3) {
        return vec![texto.to_string()];
    }
    partes.into_iter().map(String::from).collect()
}

/// Divide em " feat. ", " ft. ", " part. "... (também "(feat. X)").
fn separar_palavras(texto: &str) -> Vec<String> {
    let mut saida = Vec::new();
    let mut atual = String::new();
    let palavras: Vec<&str> = texto.split(' ').collect();
    for p in palavras {
        let limpa = p.trim_start_matches(['(', '[']).to_lowercase();
        if !atual.trim().is_empty() && SEPARADORES_PALAVRA.contains(&limpa.as_str()) {
            saida.push(std::mem::take(&mut atual));
            continue;
        }
        if !atual.is_empty() {
            atual.push(' ');
        }
        atual.push_str(p);
    }
    saida.push(atual);
    saida
}

/// Apelido normalizado → nome principal (seção 11.3).
pub type Mapa = HashMap<String, String>;

/// Muda sempre que os apelidos mudam: a função SQL recarrega o mapa em memória.
static GERACAO: AtomicU64 = AtomicU64::new(1);

pub fn apelidos_mudaram() {
    GERACAO.fetch_add(1, Ordering::Relaxed);
}

pub fn mapa(conn: &Connection) -> Mapa {
    let Ok(mut stmt) = conn.prepare("SELECT alias_norm, canonical FROM artist_alias") else { return Mapa::new() };
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map(|l| l.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

/// `separar` com os apelidos trocados pelo nome principal (sem repetir).
pub fn separar_mapeado(texto: &str, mapa: &Mapa) -> Vec<String> {
    let mut saida: Vec<String> = Vec::new();
    for nome in separar(texto) {
        let nome = mapa.get(&crate::search::normalizar(&nome)).cloned().unwrap_or(nome);
        let chave = crate::search::normalizar(&nome);
        if !saida.iter().any(|x| crate::search::normalizar(x) == chave) {
            saida.push(nome);
        }
    }
    saida
}

/// Registra `ARTISTAS(texto)`: JSON com os artistas (apelidos já trocados), para usar com
/// `json_each`. Texto nulo ou vazio vira `[null]` (a música aparece como "Desconhecido").
/// Não é determinística: depende da tabela de apelidos (mapa em cache por geração).
pub fn registrar(conn: &Connection) -> rusqlite::Result<()> {
    let cache: Mutex<(u64, Mapa)> = Mutex::new((0, Mapa::new()));
    conn.create_scalar_function("ARTISTAS", 1, FunctionFlags::SQLITE_UTF8, move |ctx| {
        let texto: Option<String> = ctx.get(0)?;
        let Some(texto) = texto else { return Ok("[null]".to_string()) };
        let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
        let geracao = GERACAO.load(Ordering::Relaxed);
        if c.0 != geracao {
            // SAFETY: a conexão só é usada aqui dentro, na mesma thread da consulta.
            let conn = unsafe { ctx.get_connection()? };
            *c = (geracao, mapa(&conn));
        }
        let lista = separar_mapeado(&texto, &c.1);
        Ok(if lista.is_empty() { "[null]".to_string() } else { serde_json::to_string(&lista).unwrap_or_default() })
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn separa_creditos() {
        assert_eq!(separar("Rodrigo Zin/1LUM3/GEO"), ["Rodrigo Zin", "1LUM3", "GEO"]);
        assert_eq!(separar("4KGuto / ÉoLt"), ["4KGuto", "ÉoLt"]);
        assert_eq!(separar("Akashi Cruz feat. Barmilly"), ["Akashi Cruz", "Barmilly"]);
        assert_eq!(separar("Akashi Cruz (Feat. Barmilly)"), ["Akashi Cruz", "Barmilly"]);
        assert_eq!(separar("Akashi Cruz part. Cunamata; ÉoDan"), ["Akashi Cruz", "Cunamata", "ÉoDan"]);
        assert_eq!(separar("X ft. Y/x"), ["X", "Y"]);
        assert_eq!(separar("AC/DC"), ["AC/DC"]);
        assert_eq!(separar("Simon & Garfunkel"), ["Simon & Garfunkel"]);
        assert_eq!(separar("Feat Lisboa"), ["Feat Lisboa"]);
        assert!(separar("  ").is_empty());
    }

    #[test]
    fn funcao_sql() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM json_each(ARTISTAS('A/B feat. C'))", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 3);
        let nulo: Option<String> =
            conn.query_row("SELECT value FROM json_each(ARTISTAS(NULL))", [], |r| r.get(0)).unwrap();
        assert_eq!(nulo, None);
    }
}
