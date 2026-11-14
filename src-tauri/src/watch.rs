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
                let resumo = processar(&h, caminhos);
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

fn processar(app: &AppHandle, caminhos: HashSet<PathBuf>) -> ImportResumo {
    let db = app.state::<Db>();
    let mut resumo = ImportResumo::default();
    let mut arquivos = Vec::new();
    for p in caminhos {
        if p.is_dir() {
            // Pasta copiada ou movida para dentro de uma pasta monitorada.
            resumo.somar(import::importar(&db.0, &p).unwrap_or_default());
        } else if p.is_file() {
            if import::eh_audio(&p) {
                arquivos.push(p);
            }
        } else if let Ok(conn) = db.0.lock() {
            resumo.ausentes += import::marcar_sumidos(&conn, &p).unwrap_or(0);
        }
    }
    if !arquivos.is_empty() {
        resumo.somar(import::importar_arquivos(&db.0, &arquivos).unwrap_or_default());
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

pub fn adicionar(conn: &Connection, pasta: &str) -> rusqlite::Result<()> {
    conn.execute("INSERT OR IGNORE INTO watched_folder (path) VALUES (?1)", [pasta])?;
    Ok(())
}

/// Para de monitorar. As músicas da pasta continuam na biblioteca.
pub fn remover(conn: &Connection, pasta: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM watched_folder WHERE path = ?1", [pasta])?;
    Ok(())
}
