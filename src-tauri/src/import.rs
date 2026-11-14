use std::path::Path;
use std::time::UNIX_EPOCH;

use lofty::prelude::*;
use lofty::tag::ItemKey;
use rusqlite::{Connection, params};
use serde::Serialize;
use walkdir::WalkDir;

const EXTENSOES: &[&str] = &["mp3", "flac", "ogg", "opus", "oga", "m4a", "aac", "wav"];

#[derive(Serialize, Default)]
pub struct ImportResumo {
    pub encontrados: u32,
    pub adicionados: u32,
    pub existentes: u32,
    pub falhas: Vec<String>,
    /// Músicas que reapareceram (estavam marcadas como não encontradas).
    pub reencontrados: u32,
    /// Músicas cujo arquivo sumiu e foram marcadas como não encontradas.
    pub ausentes: u32,
    /// Arquivos renomeados ou movidos reconhecidos (registro antigo ficou com o caminho novo).
    pub movidos: u32,
    /// Grupos de arquivos idênticos (mesmo hash) que ganharam um membro novo.
    pub duplicados: u32,
}

impl ImportResumo {
    pub fn somar(&mut self, outro: ImportResumo) {
        self.encontrados += outro.encontrados;
        self.adicionados += outro.adicionados;
        self.existentes += outro.existentes;
        self.reencontrados += outro.reencontrados;
        self.ausentes += outro.ausentes;
        self.movidos += outro.movidos;
        self.duplicados += outro.duplicados;
        self.falhas.extend(outro.falhas);
    }

    pub fn mudou_algo(&self) -> bool {
        self.adicionados + self.reencontrados + self.ausentes + self.movidos + self.duplicados > 0
    }
}

struct Faixa {
    path: String,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    album_artist: Option<String>,
    genre: Option<String>,
    year: Option<u32>,
    track_number: Option<u32>,
    disc_number: Option<u32>,
    duration_ms: i64,
    file_size: i64,
    file_mtime: Option<i64>,
}

pub fn eh_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSOES.contains(&e.to_ascii_lowercase().as_str()))
}

fn ler_faixa(path: &Path) -> Result<Faixa, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let arquivo = lofty::read_from_path(path).map_err(|e| e.to_string())?;
    let tag = arquivo.primary_tag().or_else(|| arquivo.first_tag());
    let texto = |s: Option<std::borrow::Cow<'_, str>>| {
        s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
    };

    // Sem título nas tags: usar o nome do arquivo (seção 6, informações ausentes).
    let title = texto(tag.and_then(|t| t.title())).or_else(|| {
        path.file_stem().map(|s| s.to_string_lossy().into_owned())
    });

    Ok(Faixa {
        path: path.to_string_lossy().into_owned(),
        title,
        artist: texto(tag.and_then(|t| t.artist())),
        album: texto(tag.and_then(|t| t.album())),
        album_artist: tag
            .and_then(|t| t.get_string(ItemKey::AlbumArtist))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        genre: texto(tag.and_then(|t| t.genre())),
        year: tag.and_then(|t| t.date()).map(|d| d.year as u32),
        track_number: tag.and_then(|t| t.track()),
        disc_number: tag.and_then(|t| t.disk()),
        duration_ms: arquivo.properties().duration().as_millis() as i64,
        file_size: meta.len() as i64,
        file_mtime: meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64),
    })
}

/// Lê a pasta (recursivamente) sem tocar no banco.
fn ler_pasta(raiz: &Path, resumo: &mut ImportResumo) -> Vec<Faixa> {
    let mut faixas = Vec::new();
    for entrada in WalkDir::new(raiz).follow_links(true).into_iter().filter_map(Result::ok) {
        let path = entrada.path();
        if !entrada.file_type().is_file() || !eh_audio(path) {
            continue;
        }
        resumo.encontrados += 1;
        match ler_faixa(path) {
            Ok(f) => faixas.push(f),
            Err(e) => resumo.falhas.push(format!("{}: {e}", path.display())),
        }
    }
    faixas
}

/// Grava as faixas. Caminhos já presentes na biblioteca são mantidos como estão,
/// para nunca sobrescrever metadados silenciosamente.
fn gravar(conn: &mut Connection, faixas: &[Faixa], resumo: &mut ImportResumo) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO media
             (path, title, artist, album, album_artist, genre, year,
              track_number, disc_number, duration_ms, file_size, file_mtime, search_text, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                 -- Categoria da pasta monitorada mais interna que contém o arquivo (seção 10).
                 COALESCE((SELECT category FROM watched_folder
                           WHERE substr(?1, 1, length(path) + 1) = path || '/'
                           ORDER BY length(path) DESC LIMIT 1), 'music'))",
        )?;
        let mut excluido = tx.prepare("SELECT 1 FROM excluded_path WHERE path = ?1")?;
        let mut reencontrar = tx.prepare("UPDATE media SET missing = 0 WHERE path = ?1 AND missing = 1")?;
        // Arquivo alterado no mesmo caminho: o hash fica para recalcular (as tags não mudam).
        let mut alterado = tx.prepare(
            "UPDATE media SET file_hash = NULL, file_size = ?2, file_mtime = ?3
             WHERE path = ?1 AND file_mtime IS NOT ?3",
        )?;
        for f in faixas {
            // Excluída da biblioteca pelo usuário: o arquivo existe, mas não volta sozinho.
            if excluido.exists([&f.path])? {
                continue;
            }
            let n = stmt.execute(params![
                f.path, f.title, f.artist, f.album, f.album_artist, f.genre, f.year,
                f.track_number, f.disc_number, f.duration_ms, f.file_size, f.file_mtime,
                crate::search::texto_busca(&[
                    f.title.as_deref(), f.artist.as_deref(), f.album.as_deref(),
                    f.album_artist.as_deref(), f.genre.as_deref(),
                ])
            ])?;
            if n == 1 {
                resumo.adicionados += 1;
            } else {
                resumo.existentes += 1;
                // Só desmarca "não encontrado"; os metadados gravados ficam como estão.
                resumo.reencontrados += reencontrar.execute([&f.path])? as u32;
                alterado.execute(params![f.path, f.file_size, f.file_mtime])?;
            }
        }
    }
    tx.commit()
}

pub fn importar(conn_db: &std::sync::Mutex<Connection>, raiz: &Path) -> Result<ImportResumo, String> {
    if !raiz.is_dir() {
        return Err("A pasta selecionada não existe.".into());
    }
    let mut resumo = ImportResumo::default();
    // A leitura dos arquivos acontece antes de travar o banco.
    let faixas = ler_pasta(raiz, &mut resumo);
    let mut conn = conn_db.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
    gravar(&mut conn, &faixas, &mut resumo).map_err(|e| e.to_string())?;
    Ok(resumo)
}

/// Importação pedida pelo usuário: os caminhos voltam a ser aceitos mesmo se tinham sido
/// excluídos da biblioteca antes.
pub fn readmitir(conn: &Connection, arquivos: &[std::path::PathBuf]) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare_cached("DELETE FROM excluded_path WHERE path = ?1")?;
    for p in arquivos {
        stmt.execute([p.to_string_lossy()])?;
    }
    Ok(())
}

/// Importa arquivos soltos (escolhidos pelo usuário ou avisados pelo monitoramento).
pub fn importar_arquivos(conn_db: &std::sync::Mutex<Connection>, arquivos: &[std::path::PathBuf]) -> Result<ImportResumo, String> {
    let mut resumo = ImportResumo::default();
    let mut faixas = Vec::new();
    for path in arquivos.iter().filter(|p| p.is_file() && eh_audio(p)) {
        resumo.encontrados += 1;
        match ler_faixa(path) {
            Ok(f) => faixas.push(f),
            Err(e) => resumo.falhas.push(format!("{}: {e}", path.display())),
        }
    }
    let mut conn = conn_db.lock().map_err(|_| "Banco de dados indisponível.".to_string())?;
    gravar(&mut conn, &faixas, &mut resumo).map_err(|e| e.to_string())?;
    Ok(resumo)
}

/// Hash (BLAKE3) do conteúdo inteiro do arquivo.
pub fn hash_arquivo(path: &Path) -> std::io::Result<String> {
    let mut h = blake3::Hasher::new();
    h.update_reader(std::fs::File::open(path)?)?;
    Ok(h.finalize().to_hex().to_string())
}

/// Calcula o hash das músicas presentes que ainda não têm (importadas agora, anteriores à
/// etapa 20 ou alteradas). A leitura dos arquivos acontece sem travar o banco.
/// Devolve quantos grupos de duplicados (não ignorados) ganharam um membro novo.
pub fn preencher_hashes(conn_db: &std::sync::Mutex<Connection>) -> Result<u32, String> {
    let erro = |_| "Banco de dados indisponível.".to_string();
    let pendentes: Vec<(i64, String)> = {
        let conn = conn_db.lock().map_err(erro)?;
        let mut stmt = conn
            .prepare("SELECT id, path FROM media WHERE file_hash IS NULL AND missing = 0")
            .map_err(|e| e.to_string())?;
        let linhas = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())?;
        linhas.filter_map(Result::ok).collect()
    };
    let mut novos = std::collections::HashSet::new();
    // Em lotes, para não segurar o banco durante a leitura de muitos arquivos.
    for lote in pendentes.chunks(32) {
        let hashes: Vec<(i64, String)> = lote
            .iter()
            .filter_map(|(id, path)| hash_arquivo(Path::new(path)).ok().map(|h| (*id, h)))
            .collect();
        let conn = conn_db.lock().map_err(erro)?;
        for (id, h) in hashes {
            conn.execute("UPDATE media SET file_hash = ?2 WHERE id = ?1", params![id, h])
                .map_err(|e| e.to_string())?;
            novos.insert(h);
        }
    }
    if novos.is_empty() {
        return Ok(0);
    }
    let conn = conn_db.lock().map_err(erro)?;
    let grupos = duplicados(&conn).map_err(|e| e.to_string())?;
    Ok(grupos.iter().filter(|g| novos.contains(&g.hash)).count() as u32)
}

#[derive(Serialize)]
pub struct Duplicado {
    pub id: i64,
    pub path: String,
    pub title: Option<String>,
    pub artist: Option<String>,
}

#[derive(Serialize)]
pub struct GrupoDuplicados {
    pub hash: String,
    pub musicas: Vec<Duplicado>,
}

/// Arquivos idênticos (mesmo hash) presentes na biblioteca, exceto os grupos ignorados.
/// Só avisa: nada é apagado (seção 5).
pub fn duplicados(conn: &Connection) -> rusqlite::Result<Vec<GrupoDuplicados>> {
    let mut stmt = conn.prepare_cached(
        "SELECT file_hash, id, path, title, artist FROM media
         WHERE missing = 0 AND file_hash IN (
             SELECT file_hash FROM media WHERE missing = 0 AND file_hash IS NOT NULL
             GROUP BY file_hash HAVING COUNT(*) > 1)
           AND file_hash NOT IN (SELECT file_hash FROM duplicate_ignored)
         ORDER BY file_hash, path",
    )?;
    let mut grupos: Vec<GrupoDuplicados> = Vec::new();
    let linhas = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, Duplicado { id: r.get(1)?, path: r.get(2)?, title: r.get(3)?, artist: r.get(4)? }))
    })?;
    for linha in linhas {
        let (hash, m) = linha?;
        match grupos.last_mut() {
            Some(g) if g.hash == hash => g.musicas.push(m),
            _ => grupos.push(GrupoDuplicados { hash, musicas: vec![m] }),
        }
    }
    Ok(grupos)
}

/// O usuário decidiu manter as cópias: o grupo deixa de ser avisado.
pub fn ignorar_duplicado(conn: &Connection, hash: &str) -> rusqlite::Result<()> {
    conn.execute("INSERT OR IGNORE INTO duplicate_ignored (file_hash) VALUES (?1)", [hash])?;
    Ok(())
}

/// Marca como não encontradas as músicas em `raiz` (arquivo ou pasta) cujo arquivo sumiu.
/// O registro, as playlists e o histórico são mantidos. Devolve quantas foram marcadas.
pub fn marcar_sumidos(conn: &Connection, raiz: &Path) -> rusqlite::Result<u32> {
    let mut stmt = conn.prepare_cached("SELECT id, path FROM media WHERE missing = 0")?;
    let sumidos: Vec<i64> = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .filter_map(Result::ok)
        .filter(|(_, p)| Path::new(p).starts_with(raiz) && !Path::new(p).is_file())
        .map(|(id, _)| id)
        .collect();
    for &id in &sumidos {
        crate::library::marcar_ausente(conn, id, true)?;
    }
    Ok(sumidos.len() as u32)
}

/// Arquivos renomeados ou movidos (seção 5): um registro ausente e um presente são o mesmo
/// arquivo se têm o mesmo hash; ou, com a mesma duração (±1 s), se além disso
/// - têm o mesmo tamanho em bytes e o mesmo título e artista (renomeado), ou o título dos
///   dois veio do nome do arquivo (sem tag de título, o nome mudou junto); ou
/// - têm título, artista e álbum iguais e preenchidos (tags regravadas mudam o tamanho);
/// e o par é único dos dois lados. O registro antigo fica com o caminho novo
/// (mantendo playlists, curtidas e histórico) e o duplicado sai, passando as referências
/// dele para o antigo. Nenhum arquivo é tocado. Devolve os pares (duplicado, antigo).
pub fn reconciliar_movidos(conn: &mut Connection) -> rusqlite::Result<Vec<(i64, i64)>> {
    let tx = conn.transaction()?;
    let pares: Vec<(i64, i64, String, Option<i64>, Option<String>, Option<i64>)> = {
        let mut stmt = tx.prepare(
            "WITH candidatos AS (
                SELECT a.id AS antigo, b.id AS novo, b.path AS path, b.file_mtime AS mtime,
                       b.file_hash AS hash, b.file_size AS tamanho
                FROM media a JOIN media b
                  ON a.missing = 1 AND b.missing = 0
                 AND a.category = b.category
                 AND ((a.file_hash IS NOT NULL AND a.file_hash = b.file_hash)
                 OR (ABS(COALESCE(a.duration_ms, 0) - COALESCE(b.duration_ms, 0)) <= 1000
                 AND (
                    (a.file_size IS NOT NULL AND a.file_size = b.file_size AND (
                        (COALESCE(a.title, '') = COALESCE(b.title, '') AND COALESCE(a.artist, '') = COALESCE(b.artist, ''))
                        OR (a.path LIKE '%/' || a.title || '.%' AND b.path LIKE '%/' || b.title || '.%')))
                    OR (a.title IS NOT NULL AND a.artist IS NOT NULL
                        AND a.title = b.title AND a.artist = b.artist
                        AND COALESCE(a.album, '') = COALESCE(b.album, ''))
                 )))
            )
            SELECT novo, antigo, path, mtime, hash, tamanho FROM candidatos c
            WHERE (SELECT COUNT(*) FROM candidatos x WHERE x.antigo = c.antigo) = 1
              AND (SELECT COUNT(*) FROM candidatos x WHERE x.novo = c.novo) = 1",
        )?;
        let linhas = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?;
        linhas.collect::<rusqlite::Result<_>>()?
    };
    for (novo, antigo, path, mtime, hash, tamanho) in &pares {
        // Curtida duplicada (curtiu de novo depois de renomear): fica uma só.
        tx.execute(
            "DELETE FROM playlist_item WHERE media_id = ?1 AND playlist_id IN (
                SELECT pi.playlist_id FROM playlist_item pi JOIN playlist p ON p.id = pi.playlist_id
                WHERE pi.media_id = ?2 AND p.kind = 'curtidas')",
            params![novo, antigo],
        )?;
        tx.execute("UPDATE playlist_item SET media_id = ?2 WHERE media_id = ?1", params![novo, antigo])?;
        tx.execute("UPDATE play_history SET media_id = ?2 WHERE media_id = ?1", params![novo, antigo])?;
        tx.execute("DELETE FROM media WHERE id = ?1", [novo])?;
        tx.execute(
            "UPDATE media SET path = ?2, file_mtime = ?3, file_hash = ?4, file_size = ?5, missing = 0 WHERE id = ?1",
            params![antigo, path, mtime, hash, tamanho],
        )?;
    }
    tx.commit()?;
    Ok(pares.into_iter().map(|(novo, antigo, ..)| (novo, antigo)).collect())
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Rodar com: SKALD_TEST_DIR=/pasta cargo test -- --nocapture
    #[test]
    fn importa_pasta_de_teste() {
        let Ok(dir) = std::env::var("SKALD_TEST_DIR") else { return };
        let db = std::sync::Mutex::new(crate::db::open(Path::new(":memory:")).unwrap());
        let r = importar(&db, Path::new(&dir)).unwrap();
        println!("encontrados={} adicionados={} falhas={:?}", r.encontrados, r.adicionados, r.falhas);
        let conn = db.lock().unwrap();
        let mut st = conn.prepare("SELECT title, artist, album, album_artist, year, track_number, duration_ms FROM media ORDER BY path").unwrap();
        let linhas = st.query_map([], |r| Ok(format!("{:?} {:?} {:?} {:?} {:?} {:?} {:?}",
            r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?, r.get::<_, Option<i64>>(4)?, r.get::<_, Option<i64>>(5)?, r.get::<_, i64>(6)?))).unwrap();
        for l in linhas { println!("{}", l.unwrap()); }
        drop(st); drop(conn);
        let r2 = importar(&db, Path::new(&dir)).unwrap();
        assert_eq!(r2.adicionados, 0);
        assert_eq!(r2.existentes, r.adicionados);
    }

    #[test]
    fn arquivo_sumido_fica_marcado_e_registro_mantido() {
        let dir = std::env::temp_dir().join(format!("skald-teste-sumidos-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let fica = dir.join("fica.mp3");
        let some = dir.join("sub/some.mp3");
        std::fs::write(&fica, b"").unwrap();
        let conn = crate::db::open(Path::new(":memory:")).unwrap();
        for p in [&fica, &some, &std::path::PathBuf::from("/fora/outra.mp3")] {
            conn.execute("INSERT INTO media (path) VALUES (?1)", [p.to_string_lossy()]).unwrap();
        }
        assert_eq!(marcar_sumidos(&conn, &dir).unwrap(), 1);
        let ausentes: Vec<String> = conn
            .prepare("SELECT path FROM media WHERE missing = 1").unwrap()
            .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(ausentes, [some.to_string_lossy().into_owned()]);
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0)).unwrap();
        assert_eq!(total, 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn renomeado_mantem_playlists_curtidas_e_historico() {
        let mut conn = crate::db::open(Path::new(":memory:")).unwrap();
        let ins = |c: &Connection, id: i64, path: &str, titulo: &str, tam: i64, dur: i64, ausente: bool| {
            c.execute(
                "INSERT INTO media (id, path, title, artist, file_size, duration_ms, missing) VALUES (?1, ?2, ?3, 'Djavan', ?4, ?5, ?6)",
                params![id, path, titulo, tam, dur, ausente],
            )
            .unwrap();
        };
        ins(&conn, 1, "/m/oceano.{ext}.mp3", "Oceano", 5000, 200_000, true);
        ins(&conn, 2, "/m/Djavan - Oceano.mp3", "Oceano", 5000, 200_400, false);
        // Ambíguo: dois ausentes iguais para um presente → não mexe.
        ins(&conn, 3, "/m/a1.mp3", "Lilás", 7000, 100_000, true);
        ins(&conn, 4, "/m/a2.mp3", "Lilás", 7000, 100_000, true);
        ins(&conn, 5, "/m/Lilás.mp3", "Lilás", 7000, 100_000, false);
        // Tags regravadas: tamanho mudou, título/artista/álbum/duração iguais → mesmo arquivo.
        ins(&conn, 6, "/m/b.mp3", "Sina", 9000, 100_000, true);
        ins(&conn, 7, "/m/Sina.mp3", "Sina", 9300, 100_000, false);
        // Sem tag de título (título = nome do arquivo), mesmo tamanho → mesmo arquivo.
        ins(&conn, 8, "/m/faixa 01.{ext}.mp3", "faixa 01.{ext}", 4000, 90_000, true);
        ins(&conn, 9, "/m/Faixa 1.mp3", "Faixa 1", 4000, 90_000, false);
        // Duração diferente → outro arquivo, mesmo com o mesmo título.
        ins(&conn, 10, "/m/c.mp3", "Velha", 3000, 100_000, true);
        ins(&conn, 11, "/m/Velha.mp3", "Velha", 3000, 120_000, false);

        let curtidas: i64 = conn.query_row("SELECT id FROM playlist WHERE kind = 'curtidas'", [], |r| r.get(0)).unwrap();
        conn.execute("INSERT INTO playlist (id, name) VALUES (50, 'Minha')", []).unwrap();
        for (pl, m, pos) in [(curtidas, 1, 0), (curtidas, 2, 1), (50, 1, 0), (50, 2, 1)] {
            conn.execute("INSERT INTO playlist_item (playlist_id, media_id, position) VALUES (?1, ?2, ?3)", params![pl, m, pos]).unwrap();
        }
        for m in [1, 2] {
            conn.execute("INSERT INTO play_history (media_id, title, started_at) VALUES (?1, 'Oceano', 0)", [m]).unwrap();
        }

        let mut pares = reconciliar_movidos(&mut conn).unwrap();
        pares.sort();
        assert_eq!(pares, [(2, 1), (7, 6), (9, 8)]);
        let (path, ausente): (String, bool) =
            conn.query_row("SELECT path, missing FROM media WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((path.as_str(), ausente), ("/m/Djavan - Oceano.mp3", false));
        let conta = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(conta("SELECT COUNT(*) FROM media WHERE id = 2"), 0);
        assert_eq!(conta(&format!("SELECT COUNT(*) FROM playlist_item WHERE playlist_id = {curtidas}")), 1);
        assert_eq!(conta("SELECT COUNT(*) FROM playlist_item WHERE playlist_id = 50 AND media_id = 1"), 2);
        assert_eq!(conta("SELECT COUNT(*) FROM play_history WHERE media_id = 1"), 2);
        assert_eq!(conta("SELECT COUNT(*) FROM media"), 8);
        assert_eq!(conta("SELECT COUNT(*) FROM media WHERE missing = 1"), 3); // Lilás (2) e Velha
        assert_eq!(reconciliar_movidos(&mut conn).unwrap(), []);
    }

    #[test]
    fn excluida_nao_volta_na_importacao() {
        let mut conn = crate::db::open(Path::new(":memory:")).unwrap();
        let faixa = |path: &str| Faixa {
            path: path.into(), title: None, artist: None, album: None, album_artist: None, genre: None,
            year: None, track_number: None, disc_number: None, duration_ms: 1000, file_size: 1, file_mtime: None,
        };
        conn.execute("INSERT INTO excluded_path (path) VALUES ('/fora.mp3')", []).unwrap();
        let mut resumo = ImportResumo::default();
        gravar(&mut conn, &[faixa("/fora.mp3"), faixa("/dentro.mp3")], &mut resumo).unwrap();
        assert_eq!((resumo.adicionados, resumo.existentes), (1, 0));
        let paths: Vec<String> = conn.prepare("SELECT path FROM media").unwrap()
            .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(paths, ["/dentro.mp3"]);
        readmitir(&conn, &["/fora.mp3".into()]).unwrap();
        let mut resumo = ImportResumo::default();
        gravar(&mut conn, &[faixa("/fora.mp3")], &mut resumo).unwrap();
        assert_eq!(resumo.adicionados, 1);
        // Categoria vem da pasta monitorada mais interna.
        conn.execute_batch(
            "INSERT INTO watched_folder (path, category) VALUES ('/p', 'podcast'), ('/p/m', 'music');",
        )
        .unwrap();
        let mut resumo = ImportResumo::default();
        gravar(&mut conn, &[faixa("/p/ep.mp3"), faixa("/p/m/x.mp3"), faixa("/px/y.mp3")], &mut resumo).unwrap();
        let cat = |p: &str| conn.query_row("SELECT category FROM media WHERE path = ?1", [p], |r| r.get::<_, String>(0)).unwrap();
        assert_eq!((cat("/p/ep.mp3").as_str(), cat("/p/m/x.mp3").as_str(), cat("/px/y.mp3").as_str()), ("podcast", "music", "music"));
    }

    #[test]
    fn hash_detecta_duplicados_e_movidos() {
        let dir = std::env::temp_dir().join(format!("skald-teste-hash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let arq = |nome: &str, conteudo: &[u8]| {
            let p = dir.join(nome);
            std::fs::write(&p, conteudo).unwrap();
            p.to_string_lossy().into_owned()
        };
        let (a, b, c, d) = (arq("a.mp3", b"igual"), arq("b.mp3", b"igual"), arq("c.mp3", b"outra"), arq("d.mp3", b"movida"));
        let db = std::sync::Mutex::new(crate::db::open(Path::new(":memory:")).unwrap());
        {
            let conn = db.lock().unwrap();
            // Ausente com metadados totalmente diferentes do arquivo novo: só o hash os liga.
            conn.execute(
                "INSERT INTO media (id, path, title, duration_ms, missing, file_hash) VALUES (1, '/velho/x.mp3', 'X', 1000, 1, ?1)",
                [hash_arquivo(Path::new(&d)).unwrap()],
            )
            .unwrap();
            for (id, p) in [(2, &a), (3, &b), (4, &c), (5, &d)] {
                conn.execute("INSERT INTO media (id, path, title, duration_ms) VALUES (?1, ?2, 'Y', 99000)", params![id, p]).unwrap();
            }
        }
        assert_eq!(preencher_hashes(&db).unwrap(), 1);
        assert_eq!(preencher_hashes(&db).unwrap(), 0);
        let mut conn = db.lock().unwrap();
        let grupos = duplicados(&conn).unwrap();
        assert_eq!(grupos.len(), 1);
        assert_eq!(grupos[0].musicas.iter().map(|m| m.id).collect::<Vec<_>>(), [2, 3]);

        assert_eq!(reconciliar_movidos(&mut conn).unwrap(), [(5, 1)]);
        let path: String = conn.query_row("SELECT path FROM media WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(path, d);

        ignorar_duplicado(&conn, &grupos[0].hash).unwrap();
        assert!(duplicados(&conn).unwrap().is_empty());
        // Nada foi apagado do disco.
        assert!([&a, &b, &c, &d].iter().all(|p| Path::new(p).is_file()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Ensaio sobre uma CÓPIA do banco real (altera o arquivo indicado).
    /// Rodar com: SKALD_DB_ENSAIO=/copia.db cargo test ensaio -- --nocapture
    #[test]
    fn ensaio_reconciliacao() {
        let Ok(caminho) = std::env::var("SKALD_DB_ENSAIO") else { return };
        let db = std::sync::Mutex::new(crate::db::open(Path::new(&caminho)).unwrap());
        let inicio = std::time::Instant::now();
        let novos = preencher_hashes(&db).unwrap();
        println!("hashes em {:?}; grupos de duplicados novos: {novos}", inicio.elapsed());
        let mut conn = db.into_inner().unwrap();
        for g in duplicados(&conn).unwrap() {
            println!("duplicado: {:?}", g.musicas.iter().map(|m| &m.path).collect::<Vec<_>>());
        }
        let conta = |c: &Connection, sql: &str| c.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        println!("antes: {} músicas, {} ausentes", conta(&conn, "SELECT COUNT(*) FROM media"), conta(&conn, "SELECT COUNT(*) FROM media WHERE missing = 1"));
        let pares = reconciliar_movidos(&mut conn).unwrap();
        println!("unidos: {}", pares.len());
        println!("depois: {} músicas, {} ausentes", conta(&conn, "SELECT COUNT(*) FROM media"), conta(&conn, "SELECT COUNT(*) FROM media WHERE missing = 1"));
        println!("playlists/curtidas em ausentes: {}", conta(&conn, "SELECT COUNT(*) FROM playlist_item pi JOIN media m ON m.id = pi.media_id WHERE m.missing = 1"));
        println!("histórico em ausentes: {}", conta(&conn, "SELECT COUNT(*) FROM play_history h JOIN media m ON m.id = h.media_id WHERE m.missing = 1"));
    }
}
