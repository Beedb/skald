//! Integração com o sistema (seção 8.10 e 12): iniciar com a sessão e notificação de troca de música.

use std::path::{Path, PathBuf};

/// Argumento usado ao iniciar com o sistema: com a bandeja ligada, abre só na bandeja.
pub const ARG_MINIMIZADO: &str = "--minimizado";

/// `~/.config/autostart/skald.desktop` (padrão XDG, sem depender de serviço).
pub fn arquivo_autostart(config_dir: &Path) -> PathBuf {
    config_dir.join("autostart").join("skald.desktop")
}

pub fn autostart_ativo(config_dir: &Path) -> bool {
    arquivo_autostart(config_dir).is_file()
}

/// Cria ou remove a entrada de início automático (só o nosso `.desktop`).
pub fn definir_autostart(config_dir: &Path, executavel: &Path, ativo: bool) -> Result<(), String> {
    let arquivo = arquivo_autostart(config_dir);
    if !ativo {
        return match std::fs::remove_file(&arquivo) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let exe = executavel.to_string_lossy();
    // Aspas no Exec, conforme a especificação de .desktop (caminho com espaço).
    let exe = format!("\"{}\"", exe.replace('\\', "\\\\").replace('"', "\\\""));
    let conteudo = format!(
        "[Desktop Entry]\nType=Application\nName=Skáld\nComment=Reprodutor de música\nExec={exe} {ARG_MINIMIZADO}\nIcon=skald\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    );
    std::fs::create_dir_all(arquivo.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&arquivo, conteudo).map_err(|e| e.to_string())
}

/// Windows: valor `Skald` em `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
#[cfg(windows)]
pub mod registro {
    use std::path::Path;

    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};

    const CHAVE: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const NOME: &str = "Skald";

    pub fn ativo() -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(CHAVE, KEY_READ)
            .and_then(|k| k.get_value::<String, _>(NOME))
            .is_ok()
    }

    pub fn definir(executavel: &Path, ativo: bool) -> Result<(), String> {
        let chave = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(CHAVE, KEY_SET_VALUE)
            .map_err(|e| e.to_string())?;
        if !ativo {
            return match chave.delete_value(NOME) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        }
        // Aspas no caminho (pode ter espaço, ex.: C:\Users\Nome Sobrenome\...).
        let valor = format!("\"{}\" {}", executavel.display(), super::ARG_MINIMIZADO);
        chave.set_value(NOME, &valor).map_err(|e| e.to_string())
    }
}

/// Notificação de nova música (D-Bus). Falhas são ignoradas: o app segue sem ela.
pub fn notificar_musica(titulo: &str, artista: &str, capa: Option<&Path>) {
    let mut n = notify_rust::Notification::new();
    n.appname("Skáld").summary(titulo).body(artista).icon("skald").timeout(5000);
    if let Some(c) = capa {
        n.image_path(&c.to_string_lossy());
    }
    let _ = n.show();
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn autostart_cria_e_remove() {
        let dir = std::env::temp_dir().join(format!("skald-autostart-{}", std::process::id()));
        assert!(!autostart_ativo(&dir));
        definir_autostart(&dir, Path::new("/opt/Meu App/skald"), true).unwrap();
        assert!(autostart_ativo(&dir));
        let txt = std::fs::read_to_string(arquivo_autostart(&dir)).unwrap();
        assert!(txt.contains("Exec=\"/opt/Meu App/skald\" --minimizado"));
        definir_autostart(&dir, Path::new("/x"), false).unwrap();
        assert!(!autostart_ativo(&dir));
        definir_autostart(&dir, Path::new("/x"), false).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
