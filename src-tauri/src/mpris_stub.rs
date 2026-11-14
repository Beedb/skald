//! Fora do Linux não há MPRIS. Mesma API pública de `mpris.rs`, sem efeito.
//! Os controles de mídia do Windows (SMTC) ficam para a etapa 40.

use tauri::AppHandle;

use crate::player::EstadoPlayer;

pub struct Mpris;

impl Mpris {
    pub fn iniciar(_app: AppHandle) -> Result<Self, String> {
        Ok(Self)
    }

    pub fn atualizar(&self, _app: &AppHandle, _e: &EstadoPlayer) {}
}
