//! Pastas de dados e de cache do app.
//!
//! No desktop vêm do Tauri (`app.path()`). No Android o plugin de caminhos do Tauri só
//! responde com a atividade aberta, e o motor pode subir sem ela (Android Auto): o Kotlin
//! informa as pastas (`Context.dataDir` e `cacheDir`, as mesmas do Tauri) por
//! `Midia.iniciarNativo` antes de o Rust iniciar.

use std::path::PathBuf;

use tauri::{AppHandle, Runtime};

#[cfg(target_os = "android")]
static PASTAS: std::sync::OnceLock<(PathBuf, PathBuf, PathBuf)> = std::sync::OnceLock::new();

/// Android: pastas de dados, de cache e Música (pública) vindas do Kotlin (só a primeira chamada vale).
#[cfg(target_os = "android")]
pub fn definir_android(dados: PathBuf, cache: PathBuf, musica: PathBuf) {
    let _ = PASTAS.set((dados, cache, musica));
}

/// Pasta de dados do app (banco, capas e letras guardadas no Skáld).
pub fn dados<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    if let Some((dados, _, _)) = PASTAS.get() {
        return Ok(dados.clone());
    }
    use tauri::Manager;
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Pasta de cache do app (miniaturas, capas enviadas ao sistema).
pub fn cache<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    if let Some((_, cache, _)) = PASTAS.get() {
        return Ok(cache.clone());
    }
    use tauri::Manager;
    app.path().app_cache_dir().map_err(|e| e.to_string())
}

/// Pasta Música da pessoa (destino das músicas baixadas do Drive). No Android é a pública
/// (`/storage/emulated/0/Music`): a do Tauri é privada do app e some ao desinstalar.
pub fn musica<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    if let Some((_, _, musica)) = PASTAS.get() {
        return Ok(musica.clone());
    }
    use tauri::Manager;
    app.path().audio_dir().map_err(|_| "Pasta de música do sistema não encontrada.".to_string())
}
