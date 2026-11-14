//! Créditos com vários artistas ("Rodrigo Zin/1LUM3/GEO", "X feat. Y", "X part. Y"):
//! cada participante conta como artista na coleção e nas estatísticas. O texto gravado
//! na música nunca muda; a divisão é só na hora de agrupar.

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

/// Registra `ARTISTAS(texto)`: JSON com os artistas, para usar com `json_each`.
/// Texto nulo ou vazio vira `[null]` (a música continua aparecendo como "Desconhecido").
pub fn registrar(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        "ARTISTAS",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let texto: Option<String> = ctx.get(0)?;
            let lista = texto.as_deref().map(separar).unwrap_or_default();
            Ok(if lista.is_empty() { "[null]".to_string() } else { serde_json::to_string(&lista).unwrap_or_default() })
        },
    )
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
