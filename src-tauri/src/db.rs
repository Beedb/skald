use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

/// Conexão SQLite compartilhada pelo app.
pub struct Db(pub Mutex<Connection>);

/// Migrações em ordem. O índice + 1 é a versão gravada em `PRAGMA user_version`.
/// Nunca editar uma migração já publicada; sempre acrescentar uma nova.
const MIGRATIONS: &[&str] = &[
    // v1: tabela de mídia, já preparada para categorias futuras (seção 10).
    "CREATE TABLE media (
        id              INTEGER PRIMARY KEY,
        path            TEXT    NOT NULL UNIQUE,
        title           TEXT,
        artist          TEXT,
        album           TEXT,
        album_artist    TEXT,
        genre           TEXT,
        year            INTEGER,
        track_number    INTEGER,
        disc_number     INTEGER,
        duration_ms     INTEGER,
        file_size       INTEGER,
        file_mtime      INTEGER,
        file_hash       TEXT,
        missing         INTEGER NOT NULL DEFAULT 0,
        category        TEXT    NOT NULL DEFAULT 'music'
                        CHECK (category IN ('music', 'podcast', 'audiobook', 'other')),
        resume_position REAL,
        added_at        INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE INDEX idx_media_hash ON media(file_hash);",
    // v2: texto normalizado (minúsculas, sem acentos) para a busca; preenchido em Rust.
    "ALTER TABLE media ADD COLUMN search_text TEXT;",
];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(&mut conn)?;
    crate::search::preencher_pendentes(&mut conn)?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let current: usize = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
    }
    Ok(())
}

pub fn schema_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.pragma_query_value(None, "user_version", |r| r.get(0))
}

pub fn media_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn migra_v1_para_v2_e_preenche_busca() {
        let dir = std::env::temp_dir().join(format!("skald-teste-migracao-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let caminho = dir.join("v1.db");
        let _ = std::fs::remove_file(&caminho);
        {
            let conn = Connection::open(&caminho).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute("INSERT INTO media (path, title, artist) VALUES ('/x.mp3', 'Canção', 'Ánirap')", []).unwrap();
        }
        let conn = open(&caminho).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), MIGRATIONS.len() as i64);
        let texto: String = conn.query_row("SELECT search_text FROM media", [], |r| r.get(0)).unwrap();
        assert_eq!(texto, "cancao anirap");
        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
