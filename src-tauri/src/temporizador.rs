//! Temporizador (seção 8.9): pausa a reprodução depois de um tempo ou no fim da música atual.

use serde::Serialize;
use std::time::{Duration, Instant};

/// Nos últimos segundos o volume desce aos poucos até a pausa.
pub const ATENUAR: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Modo {
    Ate(Instant),
    FimDaFaixa,
}

#[derive(Default, Debug)]
pub struct Temporizador {
    modo: Option<Modo>,
    /// Muda a cada troca, para a contagem antiga saber que foi cancelada.
    geracao: u64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Visao {
    /// Quanto falta, em ms (modo por tempo).
    pub restante_ms: Option<u64>,
    pub fim_da_faixa: bool,
}

impl Temporizador {
    /// Troca o modo (ou desliga com `None`) e devolve a nova geração.
    pub fn definir(&mut self, modo: Option<Modo>) -> u64 {
        self.modo = modo;
        self.geracao += 1;
        self.geracao
    }

    pub fn geracao(&self) -> u64 {
        self.geracao
    }

    pub fn fim_da_faixa(&self) -> bool {
        self.modo == Some(Modo::FimDaFaixa)
    }

    pub fn visao(&self, agora: Instant) -> Visao {
        Visao {
            restante_ms: match self.modo {
                Some(Modo::Ate(fim)) => Some(fim.saturating_duration_since(agora).as_millis() as u64),
                _ => None,
            },
            fim_da_faixa: self.fim_da_faixa(),
        }
    }
}

/// Fator de volume para o tempo que falta: 1 até os últimos `ATENUAR`, depois desce até 0.
pub fn atenuacao(restante: Duration) -> f64 {
    (restante.as_secs_f64() / ATENUAR.as_secs_f64()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn modos_e_geracao() {
        let mut t = Temporizador::default();
        let agora = Instant::now();
        assert_eq!(t.visao(agora), Visao { restante_ms: None, fim_da_faixa: false });
        let g1 = t.definir(Some(Modo::Ate(agora + Duration::from_secs(60))));
        assert_eq!(t.visao(agora).restante_ms, Some(60_000));
        assert_eq!(t.visao(agora + Duration::from_secs(90)).restante_ms, Some(0));
        let g2 = t.definir(Some(Modo::FimDaFaixa));
        assert_ne!(g1, g2);
        assert!(t.fim_da_faixa());
        assert_eq!(t.visao(agora), Visao { restante_ms: None, fim_da_faixa: true });
        t.definir(None);
        assert!(!t.fim_da_faixa());
        assert_eq!(t.geracao(), 3);
    }

    #[test]
    fn atenua_so_no_fim() {
        assert_eq!(atenuacao(Duration::from_secs(60)), 1.0);
        assert_eq!(atenuacao(Duration::from_secs(5)), 0.5);
        assert_eq!(atenuacao(Duration::ZERO), 0.0);
    }
}
