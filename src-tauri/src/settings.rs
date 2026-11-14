//! Configurações (seção 12), guardadas na tabela `setting` como chave/valor.

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

const CROSSFADE: &str = "crossfade_ms";
const NORMALIZACAO: &str = "normalizacao";
/// Crossfade padrão: ligado, 5 s (desligado entre faixas do mesmo álbum).
const CROSSFADE_PADRAO: u64 = 5000;
pub const CROSSFADE_MAX: u64 = 12_000;

#[derive(Serialize)]
pub struct Configuracoes {
    pub crossfade_ms: u64,
    /// ReplayGain: desligada, faixa (padrão) ou álbum.
    pub normalizacao: crate::ganho::Modo,
}

fn ler(conn: &Connection, chave: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM setting WHERE key = ?1", [chave], |r| r.get(0)).optional()
}

fn gravar(conn: &Connection, chave: &str, valor: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO setting (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [chave, valor],
    )?;
    Ok(())
}

pub fn carregar(conn: &Connection) -> rusqlite::Result<Configuracoes> {
    let crossfade_ms = ler(conn, CROSSFADE)?
        .and_then(|v| v.parse().ok())
        .unwrap_or(CROSSFADE_PADRAO)
        .min(CROSSFADE_MAX);
    let normalizacao = ler(conn, NORMALIZACAO)?
        .and_then(|v| crate::ganho::Modo::de_texto(&v))
        .unwrap_or_default();
    Ok(Configuracoes { crossfade_ms, normalizacao })
}

pub fn definir_normalizacao(conn: &Connection, modo: crate::ganho::Modo) -> rusqlite::Result<()> {
    gravar(conn, NORMALIZACAO, modo.texto())
}

/// Grava e devolve o valor efetivo (limitado a `CROSSFADE_MAX`).
pub fn definir_crossfade(conn: &Connection, ms: u64) -> rusqlite::Result<u64> {
    let ms = ms.min(CROSSFADE_MAX);
    gravar(conn, CROSSFADE, &ms.to_string())?;
    Ok(ms)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn crossfade_padrao_e_gravado() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(carregar(&conn).unwrap().crossfade_ms, CROSSFADE_PADRAO);
        assert_eq!(definir_crossfade(&conn, 0).unwrap(), 0);
        assert_eq!(carregar(&conn).unwrap().crossfade_ms, 0);
        assert_eq!(definir_crossfade(&conn, 60_000).unwrap(), CROSSFADE_MAX);
        assert_eq!(carregar(&conn).unwrap().crossfade_ms, CROSSFADE_MAX);
    }

    #[test]
    fn normalizacao_padrao_e_gravada() {
        use crate::ganho::Modo;
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(carregar(&conn).unwrap().normalizacao, Modo::Faixa);
        definir_normalizacao(&conn, Modo::Album).unwrap();
        assert_eq!(carregar(&conn).unwrap().normalizacao, Modo::Album);
        definir_normalizacao(&conn, Modo::Desligada).unwrap();
        assert_eq!(carregar(&conn).unwrap().normalizacao, Modo::Desligada);
    }
}
