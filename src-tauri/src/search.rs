use rusqlite::{Connection, params};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Minúsculas e sem acentos: "Canção" -> "cancao".
pub fn normalizar(s: &str) -> String {
    s.nfd().filter(|c| !is_combining_mark(*c)).flat_map(char::to_lowercase).collect()
}

/// Texto pesquisável de uma música: título, artista, álbum, artista do álbum e gênero.
pub fn texto_busca(campos: &[Option<&str>]) -> String {
    campos.iter().flatten().map(|c| normalizar(c)).collect::<Vec<_>>().join(" ")
}

/// Preenche `search_text` dos registros antigos (anteriores à coluna). Rápido se não houver nada.
pub fn preencher_pendentes(conn: &mut Connection) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut ler = tx.prepare(
            "SELECT id, title, artist, album, album_artist, genre FROM media WHERE search_text IS NULL",
        )?;
        let mut gravar = tx.prepare("UPDATE media SET search_text = ?2 WHERE id = ?1")?;
        let linhas = ler.query_map([], |r| {
            let campos: Vec<Option<String>> = (1..=5).map(|i| r.get(i)).collect::<Result<_, _>>()?;
            Ok((r.get::<_, i64>(0)?, campos))
        })?;
        for linha in linhas {
            let (id, campos) = linha?;
            let refs: Vec<Option<&str>> = campos.iter().map(Option::as_deref).collect();
            gravar.execute(params![id, texto_busca(&refs)])?;
        }
    }
    tx.commit()
}

/// Condição SQL para a busca: cada palavra precisa aparecer em algum campo.
/// Devolve `(condição, parâmetros)`; busca vazia vira `("1", [])`.
pub fn filtro(busca: &str) -> (String, Vec<String>) {
    let termos: Vec<String> = normalizar(busca)
        .split_whitespace()
        .map(|t| format!("%{}%", t.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")))
        .collect();
    if termos.is_empty() {
        return ("1".into(), termos);
    }
    let cond = vec!["search_text LIKE ? ESCAPE '\\'"; termos.len()].join(" AND ");
    (cond, termos)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn normaliza_acentos_e_caixa() {
        assert_eq!(normalizar("Canção ÁGUA Über"), "cancao agua uber");
        assert_eq!(texto_busca(&[Some("Faixa"), None, Some("Pé")]), "faixa pe");
    }

    #[test]
    fn filtro_por_palavras_e_escape() {
        assert_eq!(filtro("  ").0, "1");
        let (cond, p) = filtro("Ani 50%");
        assert_eq!(cond.matches("LIKE").count(), 2);
        assert_eq!(p, ["%ani%", "%50\\%%"]);
    }
}
