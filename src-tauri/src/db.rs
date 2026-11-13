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
];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(&mut conn)?;
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
