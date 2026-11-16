//! Monitoramento de pastas (seção 5): arquivos novos entram na biblioteca; arquivos que
//! somem são marcados como não encontrados (registro, playlists e histórico ficam).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use notify::event::{EventKind, ModifyKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use crate::db::Db;
use crate::import::{self, ImportResumo};

/// Depois do último aviso do sistema, espera este tempo antes de processar (cópias em rajada).
const ESPERA: Duration = Duration::from_secs(2);

pub struct Vigia(Mutex<RecommendedWatcher>);

impl Vigia {
    /// Cria o vigia e a thread que processa os avisos do sistema.
    pub fn iniciar(app: AppHandle) -> notify::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(tx)?;
        let h = app.clone();
        std::thread::spawn(move || {
            while let Ok(evento) = rx.recv() {
                let mut caminhos = HashSet::new();
                juntar(&mut caminhos, evento);
                while let Ok(evento) = rx.recv_timeout(ESPERA) {
                    juntar(&mut caminhos, evento);
                }
                let resumo = processar(&h.state::<Db>().0, caminhos);
                crate::biblioteca_mudou(&h, resumo);
            }
        });
        Ok(Vigia(Mutex::new(watcher)))
    }

    /// Sincroniza as pastas salvas (o que mudou com o app fechado) e passa a vigiá-las.
    /// Chamar depois de `app.manage(vigia)`.
    pub fn retomar(app: AppHandle) {
        std::thread::spawn(move || {
            let pastas = {
                let db = app.state::<Db>();
                let Ok(conn) = db.0.lock() else { return };
                listar(&conn).unwrap_or_default()
            };
            let mut resumo = ImportResumo::default();
            for pasta in pastas {
                app.state::<Vigia>().vigiar(Path::new(&pasta));
                resumo.somar(sincronizar(&app, Path::new(&pasta)));
            }
            crate::biblioteca_mudou(&app, resumo);
        });
    }

    pub fn vigiar(&self, pasta: &Path) {
        if let Ok(mut w) = self.0.lock() {
            if let Err(e) = w.watch(pasta, RecursiveMode::Recursive) {
                eprintln!("Não foi possível monitorar {}: {e}", pasta.display());
            }
        }
    }

    pub fn parar(&self, pasta: &Path) {
        if let Ok(mut w) = self.0.lock() {
            let _ = w.unwatch(pasta);
        }
    }
}

fn juntar(caminhos: &mut HashSet<PathBuf>, evento: notify::Result<notify::Event>) {
    let Ok(evento) = evento else { return };
    match evento.kind {
        EventKind::Create(_) | EventKind::Remove(_) => {}
        EventKind::Modify(ModifyKind::Name(_) | ModifyKind::Data(_) | ModifyKind::Any) => {}
        _ => return,
    }
    caminhos.extend(evento.paths);
}

fn processar(db: &Mutex<Connection>, caminhos: HashSet<PathBuf>) -> ImportResumo {
    let mut resumo = ImportResumo::default();
    let mut arquivos = Vec::new();
    for p in caminhos {
        if p.is_dir() {
            // Pasta copiada ou movida para dentro de uma pasta monitorada.
            resumo.somar(import::importar(db, &p).unwrap_or_default());
        } else if p.is_file() {
            if import::eh_audio(&p) {
                arquivos.push(p);
            }
        } else if let Ok(conn) = db.lock() {
            resumo.ausentes += import::marcar_sumidos(&conn, &p).unwrap_or(0);
        }
    }
    if !arquivos.is_empty() {
        resumo.somar(import::importar_arquivos(db, &arquivos).unwrap_or_default());
    }
    resumo
}

/// Importa o que é novo em `pasta` e marca o que sumiu. Pasta inexistente (disco
/// desmontado, por exemplo): todas as músicas dela ficam como não encontradas.
pub fn sincronizar(app: &AppHandle, pasta: &Path) -> ImportResumo {
    let db = app.state::<Db>();
    let mut resumo = if pasta.is_dir() { import::importar(&db.0, pasta).unwrap_or_default() } else { ImportResumo::default() };
    if let Ok(conn) = db.0.lock() {
        resumo.ausentes += import::marcar_sumidos(&conn, pasta).unwrap_or(0);
    }
    resumo
}

pub fn listar(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT path FROM watched_folder ORDER BY path")?;
    let pastas = stmt.query_map([], |r| r.get(0))?;
    pastas.collect()
}

/// Pasta nova entra como música (única categoria ativa; seção 10).
pub fn adicionar(conn: &Connection, pasta: &str) -> rusqlite::Result<()> {
    conn.execute("INSERT OR IGNORE INTO watched_folder (path, category) VALUES (?1, 'music')", [pasta])?;
    Ok(())
}

/// Para de monitorar. As músicas da pasta continuam na biblioteca.
pub fn remover(conn: &Connection, pasta: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM watched_folder WHERE path = ?1", [pasta])?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn wav(destino: &Path, segundos: u32) {
        let amostras = 44_100 * segundos;
        let bytes = amostras * 2;
        let mut w = Vec::new();
        for (rotulo, valor) in [(b"RIFF", 36 + bytes)] {
            w.extend_from_slice(rotulo);
            w.extend_from_slice(&valor.to_le_bytes());
        }
        w.extend_from_slice(b"WAVEfmt ");
        for v in [16u32] {
            w.extend_from_slice(&v.to_le_bytes());
        }
        for v in [1u16, 1] {
            w.extend_from_slice(&v.to_le_bytes());
        }
        for v in [44_100u32, 88_200] {
            w.extend_from_slice(&v.to_le_bytes());
        }
        for v in [2u16, 16] {
            w.extend_from_slice(&v.to_le_bytes());
        }
        w.extend_from_slice(b"data");
        w.extend_from_slice(&bytes.to_le_bytes());
        // Conteúdo diferente por arquivo (hash único).
        let semente = destino.to_string_lossy().len() as u32;
        w.extend((0..amostras).flat_map(|i| (((i * (semente + 7)) % 4000) as i16).to_le_bytes()));
        std::fs::write(destino, w).unwrap();
    }

    /// O mesmo caminho da thread de `Vigia::iniciar` + o que `biblioteca_mudou` faz depois.
    fn esperar_lote(rx: &mpsc::Receiver<notify::Result<notify::Event>>, db: &Mutex<Connection>) {
        let mut caminhos = HashSet::new();
        juntar(&mut caminhos, rx.recv_timeout(Duration::from_secs(10)).expect("o sistema não avisou"));
        while let Ok(e) = rx.recv_timeout(Duration::from_millis(700)) {
            juntar(&mut caminhos, e);
        }
        processar(db, caminhos);
        import::preencher_hashes(db).unwrap();
        import::reconciliar_movidos(&mut db.lock().unwrap()).unwrap();
    }

    fn musicas(db: &Mutex<Connection>) -> Vec<(String, bool)> {
        let conn = db.lock().unwrap();
        let mut stmt = conn.prepare("SELECT path, missing FROM media ORDER BY id").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }

    /// Monitoramento com avisos reais do sistema (inotify / ReadDirectoryChangesW), numa
    /// pasta com espaço e acento: arquivo novo, renomeado, movido para subpasta e apagado.
    #[test]
    fn monitora_pasta_de_verdade() {
        let base = std::env::temp_dir().join(format!("skald monitor ção-{}", std::process::id()));
        let pasta = base.join("Música");
        std::fs::create_dir_all(&pasta).unwrap();
        let db = Mutex::new(crate::db::open(&base.join("teste.db")).unwrap());
        adicionar(&db.lock().unwrap(), &pasta.to_string_lossy()).unwrap();

        let (tx, rx) = mpsc::channel();
        let mut vigia = notify::recommended_watcher(tx).unwrap();
        vigia.watch(&pasta, RecursiveMode::Recursive).unwrap();

        // Novo arquivo (escrito fora e movido para dentro, como uma cópia terminada).
        let fora = base.join("fora.wav");
        wav(&fora, 2);
        let a = pasta.join("Faixa Um.wav");
        std::fs::rename(&fora, &a).unwrap();
        esperar_lote(&rx, &db);
        assert_eq!(musicas(&db), vec![(a.to_string_lossy().into_owned(), false)]);

        // Renomeado: continua o mesmo registro, no caminho novo.
        let b = pasta.join("Faixa Dois ção.wav");
        std::fs::rename(&a, &b).unwrap();
        esperar_lote(&rx, &db);
        assert_eq!(musicas(&db), vec![(b.to_string_lossy().into_owned(), false)]);

        // Movido para uma subpasta.
        let sub = pasta.join("Álbum de Teste");
        std::fs::create_dir(&sub).unwrap();
        let c = sub.join("Faixa Dois ção.wav");
        std::fs::rename(&b, &c).unwrap();
        esperar_lote(&rx, &db);
        assert_eq!(musicas(&db), vec![(c.to_string_lossy().into_owned(), false)]);
        let categoria: String =
            db.lock().unwrap().query_row("SELECT category FROM media", [], |r| r.get(0)).unwrap();
        assert_eq!(categoria, "music");

        // Apagado: o registro fica, marcado como não encontrado.
        std::fs::remove_file(&c).unwrap();
        esperar_lote(&rx, &db);
        assert_eq!(musicas(&db), vec![(c.to_string_lossy().into_owned(), true)]);

        drop(vigia);
        drop(db);
        let _ = std::fs::remove_dir_all(&base);
    }
}
