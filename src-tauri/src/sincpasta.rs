//! Playlists, curtidas, histórico e apelidos por uma pasta comum (Syncthing, pendrive, rede).
//!
//! Mesmo formato do Drive (`sincdados`): cada aparelho grava `aparelho-<id>.json` na pasta e lê
//! os de todos. Quem leva os arquivos de um aparelho ao outro é outro programa; o Skáld só lê e
//! grava arquivos locais, sem internet. Cópias de conflito do Syncthing
//! (`aparelho-x.sync-conflict-….json`) também são lidas: juntar a mais é inofensivo.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

/// Uma sincronização por vez (botão, abertura e a repetição a cada poucos minutos).
static EM_ANDAMENTO: Mutex<()> = Mutex::new(());

/// Lê os arquivos dos aparelhos em `pasta`, chama `trocar` (junta e devolve o deste aparelho) e
/// grava o deste aparelho, só se mudou (evita que o Syncthing reenvie o mesmo arquivo).
pub fn sincronizar(
    pasta: &Path,
    aparelho: &str,
    trocar: impl FnOnce(Vec<String>) -> Result<String, String>,
) -> Result<(), String> {
    let _vez = EM_ANDAMENTO.lock().unwrap_or_else(|e| e.into_inner());
    fs::create_dir_all(pasta).map_err(|e| format!("Não foi possível usar a pasta {}: {e}", pasta.display()))?;
    let meu_nome = format!("aparelho-{aparelho}.json");
    let meu_caminho = pasta.join(&meu_nome);
    let mut textos = Vec::new();
    let mut atual = None;
    for entrada in fs::read_dir(pasta).map_err(|e| e.to_string())?.flatten() {
        let nome = entrada.file_name().to_string_lossy().into_owned();
        if !(nome.starts_with("aparelho-") && nome.ends_with(".json")) {
            continue;
        }
        // Arquivo ilegível (ainda chegando, por exemplo): fica para a próxima vez.
        let Ok(texto) = fs::read_to_string(entrada.path()) else { continue };
        if nome == meu_nome {
            atual = Some(texto.clone());
        }
        textos.push(texto);
    }
    let meu = trocar(textos)?;
    if atual.as_deref() != Some(meu.as_str()) {
        // Grava ao lado com outro nome e renomeia: quem lê nunca vê o arquivo pela metade.
        let temporario = pasta.join(format!(".{meu_nome}.tmp"));
        fs::write(&temporario, &meu).map_err(|e| e.to_string())?;
        fs::rename(&temporario, &meu_caminho).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_os_outros_e_grava_o_seu_so_se_mudou() {
        let pasta = crate::teste_util::pasta("skald-sincpasta");
        fs::create_dir_all(&pasta).unwrap();
        fs::write(pasta.join("aparelho-b.json"), "B").unwrap();
        fs::write(pasta.join("aparelho-b.sync-conflict-1.json"), "B2").unwrap();
        fs::write(pasta.join("outra-coisa.json"), "X").unwrap();

        let mut lidos = Vec::new();
        sincronizar(&pasta, "a", |t| {
            lidos = t;
            Ok("A".into())
        })
        .unwrap();
        lidos.sort();
        assert_eq!(lidos, ["B", "B2"]);
        assert_eq!(fs::read_to_string(pasta.join("aparelho-a.json")).unwrap(), "A");

        // Igual ao que já estava: não regrava.
        let antes = fs::metadata(pasta.join("aparelho-a.json")).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        sincronizar(&pasta, "a", |t| {
            assert_eq!(t.len(), 3);
            Ok("A".into())
        })
        .unwrap();
        let depois = fs::metadata(pasta.join("aparelho-a.json")).unwrap().modified().unwrap();
        assert_eq!(antes, depois);
        assert!(!pasta.join(".aparelho-a.json.tmp").exists());
        let _ = fs::remove_dir_all(&pasta);
    }
}
