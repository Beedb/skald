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
    // v3: playlists guardam só referências a `media` (seção 7); a mesma música pode estar em várias.
    "CREATE TABLE playlist (
        id          INTEGER PRIMARY KEY,
        name        TEXT    NOT NULL,
        cover_path  TEXT,
        created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
        updated_at  INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE TABLE playlist_item (
        id          INTEGER PRIMARY KEY,
        playlist_id INTEGER NOT NULL REFERENCES playlist(id) ON DELETE CASCADE,
        media_id    INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
        position    INTEGER NOT NULL,
        added_at    INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE INDEX idx_playlist_item_ordem ON playlist_item(playlist_id, position);
    CREATE INDEX idx_playlist_item_media ON playlist_item(media_id);",
    // v4: playlist fixa "Curtidas" (favoritos), que não pode ser renomeada nem excluída.
    "ALTER TABLE playlist ADD COLUMN kind TEXT NOT NULL DEFAULT 'user' CHECK (kind IN ('user', 'curtidas'));
    CREATE UNIQUE INDEX idx_playlist_curtidas ON playlist(kind) WHERE kind = 'curtidas';
    INSERT INTO playlist (name, kind) VALUES ('Curtidas', 'curtidas');",
    // v5: histórico de reprodução (seção 11). Guarda cópia dos dados da música para sobreviver
    // à remoção da biblioteca (aí `media_id` vira nulo) e registra a categoria (seção 10).
    "CREATE TABLE play_history (
        id          INTEGER PRIMARY KEY,
        media_id    INTEGER REFERENCES media(id) ON DELETE SET NULL,
        category    TEXT    NOT NULL DEFAULT 'music',
        title       TEXT,
        artist      TEXT,
        album       TEXT,
        genre       TEXT,
        duration_ms INTEGER,
        started_at  INTEGER NOT NULL,
        listened_ms INTEGER NOT NULL DEFAULT 0,
        completed   INTEGER NOT NULL DEFAULT 0,
        counted     INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX idx_history_inicio ON play_history(started_at);
    CREATE INDEX idx_history_media ON play_history(media_id);",
    // v6: pastas monitoradas (seção 5). Toda pasta importada passa a ser vigiada.
    "CREATE TABLE watched_folder (
        id       INTEGER PRIMARY KEY,
        path     TEXT    NOT NULL UNIQUE,
        added_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
    );",
    // v7: configurações (seção 12), chave/valor.
    "CREATE TABLE setting (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );",
    // v8: grupos de duplicados (mesmo hash) que o usuário mandou parar de avisar (seção 5).
    "CREATE TABLE duplicate_ignored (
        file_hash TEXT PRIMARY KEY
    );",
    // v9: caminhos excluídos da biblioteca (seção 5). O arquivo continua no disco; a importação
    // e o monitoramento não o trazem de volta sozinhos.
    "CREATE TABLE excluded_path (
        path        TEXT PRIMARY KEY,
        excluded_at INTEGER NOT NULL DEFAULT (unixepoch())
    );",
    // v10: cada pasta monitorada guarda a categoria do conteúdo (seção 10); por ora só música.
    "ALTER TABLE watched_folder ADD COLUMN category TEXT NOT NULL DEFAULT 'music'
        CHECK (category IN ('music', 'podcast', 'audiobook', 'other'));",
    // v11: quando a playlist foi tocada pela última vez (tela inicial, seção 4.2).
    "ALTER TABLE playlist ADD COLUMN played_at INTEGER;",
    // v12: apelidos de artistas (seção 11.3): grafias alternativas contam como o nome principal.
    "CREATE TABLE artist_alias (
        alias_norm TEXT PRIMARY KEY,
        alias      TEXT NOT NULL,
        canonical  TEXT NOT NULL
    );",
    // v13: ReplayGain calculado pelo Skáld para arquivos sem as tags (seção 8.6).
    // `tem_tag = 1`: o arquivo já tem ReplayGain (não precisa analisar). Arquivos não mudam.
    "CREATE TABLE replaygain_analise (
        media_id   INTEGER PRIMARY KEY REFERENCES media(id) ON DELETE CASCADE,
        file_mtime INTEGER,
        tem_tag    INTEGER NOT NULL DEFAULT 0,
        track_gain REAL,
        track_peak REAL
    );",
    // v14: sincronização entre aparelhos (etapa 65). Cada playlist ganha um identificador
    // global (Curtidas é sempre 'curtidas'); playlists excluídas deixam uma marca para a
    // exclusão valer nos outros aparelhos.
    "ALTER TABLE playlist ADD COLUMN uuid TEXT;
    UPDATE playlist SET uuid = CASE kind WHEN 'curtidas' THEN 'curtidas' ELSE lower(hex(randomblob(16))) END;
    CREATE UNIQUE INDEX idx_playlist_uuid ON playlist(uuid);
    CREATE TABLE playlist_removida (
        uuid        TEXT PRIMARY KEY,
        removida_em INTEGER NOT NULL
    );",
    // v15: podcasts (Fase 10, etapa 54). O áudio de cada episódio é uma linha de `media` com
    // category = 'podcast' (nula enquanto não baixado, para a assinatura por RSS da etapa 59).
    // Episódio local: guid 'local:<media_id>'. Datas em segundos Unix; posições em ms.
    "CREATE TABLE podcast_show (
        id          INTEGER PRIMARY KEY,
        titulo      TEXT    NOT NULL,
        autor       TEXT,
        capa        TEXT,
        descricao   TEXT,
        pasta       TEXT    UNIQUE,
        feed_url    TEXT    UNIQUE,
        criado_em   INTEGER NOT NULL
    );
    CREATE TABLE podcast_episode (
        id            INTEGER PRIMARY KEY,
        show_id       INTEGER NOT NULL REFERENCES podcast_show(id) ON DELETE CASCADE,
        guid          TEXT    NOT NULL,
        titulo        TEXT    NOT NULL,
        publicado_em  INTEGER,
        descricao     TEXT,
        duracao_ms    INTEGER,
        media_id      INTEGER UNIQUE REFERENCES media(id) ON DELETE SET NULL,
        ouvido_em     INTEGER,
        posicao_ms    INTEGER,
        UNIQUE (show_id, guid)
    );
    CREATE INDEX idx_episodio_show ON podcast_episode(show_id, publicado_em);",
    // v16: assinatura por RSS (etapa 59): endereço, tamanho e tipo do áudio de cada episódio.
    "ALTER TABLE podcast_episode ADD COLUMN audio_url TEXT;
    ALTER TABLE podcast_episode ADD COLUMN audio_tamanho INTEGER;
    ALTER TABLE podcast_episode ADD COLUMN audio_tipo TEXT;",
];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    // Ordem alfabética sem diferenciar acentos nem maiúsculas: "Ágape" fica junto do "A".
    conn.create_collation("SEMACENTO", |a, b| {
        crate::search::normalizar(a).cmp(&crate::search::normalizar(b))
    })?;
    crate::artistas::registrar(&conn)?;
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

/// Versão do esquema que este código cria.
pub fn versao_atual() -> i64 {
    MIGRATIONS.len() as i64
}

/// Depois de restaurar um backup: migrações que faltarem e texto de busca pendente.
pub fn atualizar(conn: &mut Connection) -> rusqlite::Result<()> {
    migrate(conn)?;
    crate::search::preencher_pendentes(conn)
}

pub fn schema_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.pragma_query_value(None, "user_version", |r| r.get(0))
}

pub fn media_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM media WHERE category = 'music'", [], |r| r.get(0))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn migra_v1_para_v2_e_preenche_busca() {
        let dir = crate::teste_util::pasta("skald-teste-migracao");
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
