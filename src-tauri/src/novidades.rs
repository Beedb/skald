//! Verificação de nova versão (opcional, desligada por padrão): consulta a
//! última release do GitHub. Só roda com a configuração `verificar_versao` ligada.

use serde::{Deserialize, Serialize};

const URL: &str = "https://api.github.com/repos/Beedb/skald/releases/latest";

#[derive(Serialize, Debug, PartialEq)]
pub struct NovaVersao {
    /// Sem o "v" (ex.: "0.3.0").
    pub versao: String,
    /// Página da release no GitHub.
    pub url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
}

/// "v0.10.2" → [0, 10, 2]; sufixos como "-beta" são ignorados.
fn partes(v: &str) -> Option<Vec<u64>> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let v = v.split(['-', '+']).next()?;
    v.split('.').map(|p| p.parse().ok()).collect()
}

/// `nova` é maior que `atual`? Versões ilegíveis contam como "não".
pub fn maior(nova: &str, atual: &str) -> bool {
    match (partes(nova), partes(atual)) {
        (Some(mut a), Some(mut b)) => {
            let n = a.len().max(b.len());
            a.resize(n, 0);
            b.resize(n, 0);
            a > b
        }
        _ => false,
    }
}

/// Versão do app; em desenvolvimento, `SKALD_VERSAO_FINGIDA` simula outra (para testar o aviso).
pub fn versao_atual() -> String {
    if cfg!(debug_assertions) {
        if let Ok(v) = std::env::var("SKALD_VERSAO_FINGIDA") {
            return v;
        }
    }
    env!("CARGO_PKG_VERSION").to_string()
}

/// Consulta o GitHub; `Ok(None)` quando já está na última versão.
pub fn verificar() -> Result<Option<NovaVersao>, String> {
    let texto = crate::enriquecer::agente()
        .get(URL)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| crate::enriquecer::erro_rede(e, "GitHub"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let release: Release = serde_json::from_str(&texto).map_err(|e| e.to_string())?;
    let atual = versao_atual();
    if !maior(&release.tag_name, &atual) || !release.html_url.starts_with("https://github.com/Beedb/skald/") {
        return Ok(None);
    }
    Ok(Some(NovaVersao {
        versao: release.tag_name.trim_start_matches(['v', 'V']).to_string(),
        url: release.html_url,
    }))
}

#[cfg(test)]
mod testes {
    use super::maior;

    #[test]
    fn compara_versoes() {
        assert!(maior("v0.3.0", "0.2.0"));
        assert!(maior("0.10.0", "0.9.9"));
        assert!(maior("1.0", "0.99.1"));
        assert!(maior("v0.2.1", "0.2"));
        assert!(!maior("v0.2.0", "0.2.0"));
        assert!(!maior("0.2", "0.2.0"));
        assert!(!maior("v0.1.9", "0.2.0"));
        assert!(!maior("v0.3.0-beta", "0.3.0"));
        assert!(!maior("lixo", "0.2.0"));
    }

    /// Consulta de verdade: `cargo test ensaio_novidades -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn ensaio_novidades() {
        println!("{:?}", super::verificar());
    }
}
