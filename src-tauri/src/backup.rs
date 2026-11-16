//! Backup do banco (seção 12, Dados): exportar uma cópia `.db` e restaurar uma.
//! Restaurar troca só o banco do Skáld; arquivos de música nunca são tocados.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

/// Cópia consistente do banco aberto (funciona com o app em uso).
pub fn exportar(conn: &Connection, destino: &Path) -> Result<(), String> {
    if destino.exists() {
        // VACUUM INTO não sobrescreve; o diálogo de salvar já pediu confirmação.
        std::fs::remove_file(destino).map_err(|e| format!("Não foi possível substituir o arquivo: {e}"))?;
    }
    conn.execute("VACUUM INTO ?1", [destino.to_string_lossy()])
        .map_err(|e| format!("Não foi possível exportar o backup: {e}"))?;
    Ok(())
}

/// Confere se o arquivo é um banco do Skáld que esta versão consegue abrir.
pub fn validar(origem: &Path) -> Result<(), String> {
    let c = Connection::open_with_flags(origem, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| "Não foi possível abrir o arquivo escolhido.".to_string())?;
    let versao: i64 = c
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|_| "O arquivo escolhido não é um banco de dados válido.".to_string())?;
    let tem_media: bool = c
        .query_row("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'media'", [], |_| Ok(true))
        .unwrap_or(false);
    if versao < 1 || !tem_media {
        return Err("O arquivo escolhido não é um backup do Skáld.".into());
    }
    if versao > crate::db::versao_atual() {
        return Err("Este backup é de uma versão mais nova do Skáld.".into());
    }
    Ok(())
}

/// Nome da cópia de segurança feita antes de restaurar.
pub fn copia_de_seguranca(pasta: &Path, agora_unix: i64) -> PathBuf {
    pasta.join(format!("skald-antes-de-restaurar-{agora_unix}.db"))
}

/// Restaura `origem` por cima do banco aberto e atualiza o esquema, se for antigo.
/// Antes, salva o banco atual em `seguranca`.
pub fn restaurar(conn: &mut Connection, origem: &Path, seguranca: &Path) -> Result<(), String> {
    validar(origem)?;
    exportar(conn, seguranca)?;
    conn.restore(rusqlite::MAIN_DB, origem, None::<fn(rusqlite::backup::Progress)>)
        .map_err(|e| format!("Não foi possível restaurar o backup: {e}"))?;
    crate::artistas::apelidos_mudaram(); // o backup traz a sua própria tabela de apelidos
    crate::db::atualizar(conn).map_err(|e| format!("Backup restaurado, mas a atualização falhou: {e}"))
}

/// Tamanho total (bytes) das pastas de cache dadas.
pub fn tamanho(pastas: &[PathBuf]) -> u64 {
    pastas
        .iter()
        .flat_map(|p| walkdir::WalkDir::new(p).into_iter().filter_map(Result::ok))
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Apaga as pastas de cache (miniaturas e capas do MPRIS); são refeitas quando preciso.
pub fn limpar(pastas: &[PathBuf]) -> Result<u64, String> {
    let liberado = tamanho(pastas);
    for p in pastas {
        match std::fs::remove_dir_all(p) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
    }
    Ok(liberado)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn exporta_valida_e_restaura() {
        let dir = crate::teste_util::pasta("skald-backup");
        std::fs::create_dir_all(&dir).unwrap();
        let mut conn = crate::db::open(&dir.join("atual.db")).unwrap();
        conn.execute("INSERT INTO media (path, title) VALUES ('/a.mp3', 'A')", []).unwrap();
        let backup = dir.join("backup.db");
        exportar(&conn, &backup).unwrap();
        exportar(&conn, &backup).unwrap(); // substitui
        validar(&backup).unwrap();

        conn.execute("INSERT INTO media (path, title) VALUES ('/b.mp3', 'B')", []).unwrap();
        let seg = copia_de_seguranca(&dir, 1);
        restaurar(&mut conn, &backup, &seg).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        let c = Connection::open(&seg).unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2); // a cópia de segurança tem o estado de antes

        let lixo = dir.join("lixo.db");
        std::fs::write(&lixo, b"nao sou banco").unwrap();
        assert!(validar(&lixo).is_err());
        let vazio = dir.join("vazio.db");
        Connection::open(&vazio).unwrap().execute_batch("CREATE TABLE x (a)").unwrap();
        assert!(validar(&vazio).is_err());

        let cache = dir.join("cache");
        std::fs::create_dir_all(cache.join("sub")).unwrap();
        std::fs::write(cache.join("sub/a.jpg"), [0u8; 100]).unwrap();
        assert_eq!(tamanho(&[cache.clone()]), 100);
        assert_eq!(limpar(&[cache.clone(), dir.join("nao-existe")]).unwrap(), 100);
        assert!(!cache.exists());
        drop((conn, c)); // no Windows, arquivo aberto não sai da pasta
        let _ = std::fs::remove_dir_all(&dir);
    }
}
