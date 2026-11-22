//! Equalizador (seção 8.7): presets sobre o `equalizer-10bands` do GStreamer
//! (29, 59, 119, 237, 474, 947, 1889, 3770, 7523 e 15011 Hz) e o personalizado (ajuste manual).

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    #[default]
    Desligado,
    Grave,
    Agudo,
    Bateria,
    Voz,
    Instrumental,
    /// Bandas escolhidas pela pessoa (configuração `equalizador_bandas`).
    Personalizado,
}

/// Ganho de cada uma das 10 bandas, em dB.
pub type Bandas = [f64; 10];

/// Limite do ajuste manual, em dB.
pub const LIMITE_DB: f64 = 12.0;

/// Ajuste manual dentro de ±`LIMITE_DB` (valores inválidos viram 0).
pub fn limitar(b: Bandas) -> Bandas {
    b.map(|g| if g.is_finite() { g.clamp(-LIMITE_DB, LIMITE_DB) } else { 0.0 })
}

/// Fator de volume que compensa o maior reforço (evita clipar).
pub fn pre_atenuacao_de(b: &Bandas) -> f64 {
    let maximo = b.iter().copied().fold(0.0, f64::max);
    10f64.powf(-maximo / 20.0)
}

impl Preset {
    pub fn de_texto(s: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
    }

    pub fn texto(self) -> String {
        serde_json::to_value(self).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
    }

    /// Ganho de cada banda, em dB (o personalizado vem de `personalizadas`).
    pub fn efetivas(self, personalizadas: &Bandas) -> Bandas {
        match self {
            Preset::Personalizado => limitar(*personalizadas),
            p => p.bandas(),
        }
    }

    /// Ganho de cada banda dos presets fixos, em dB (o personalizado aqui é plano).
    pub fn bandas(self) -> Bandas {
        match self {
            Preset::Desligado | Preset::Personalizado => [0.0; 10],
            Preset::Grave => [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Preset::Agudo => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 4.0, 5.0, 6.0],
            Preset::Bateria => [5.0, 4.0, 1.0, -1.0, -2.0, 0.0, 2.0, 3.0, 3.0, 2.0],
            Preset::Voz => [-3.0, -2.0, -1.0, 1.0, 3.0, 4.0, 4.0, 2.0, 0.0, -1.0],
            Preset::Instrumental => [3.0, 2.0, 0.0, -1.0, -1.0, 0.0, 1.0, 2.0, 3.0, 3.0],
        }
    }

    /// Fator de volume que compensa o maior reforço do preset fixo.
    #[cfg(test)]
    pub fn pre_atenuacao(self) -> f64 {
        pre_atenuacao_de(&self.bandas())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn presets_validos() {
        for p in [Preset::Desligado, Preset::Grave, Preset::Agudo, Preset::Bateria, Preset::Voz, Preset::Instrumental] {
            assert!(p.bandas().iter().all(|g| (-24.0..=12.0).contains(g)), "{p:?}");
            assert_eq!(Preset::de_texto(&p.texto()), Some(p));
        }
        assert_eq!(Preset::Desligado.pre_atenuacao(), 1.0);
        assert!((Preset::Grave.pre_atenuacao() - 0.501).abs() < 0.001); // -6 dB
        assert_eq!(Preset::de_texto("inexistente"), None);
        assert_eq!(Preset::de_texto("personalizado"), Some(Preset::Personalizado));
        let mut b = [0.0; 10];
        b[0] = 30.0;
        b[1] = f64::NAN;
        b[9] = -5.0;
        let e = Preset::Personalizado.efetivas(&b);
        assert_eq!((e[0], e[1], e[9]), (12.0, 0.0, -5.0));
        assert_eq!(Preset::Grave.efetivas(&b), Preset::Grave.bandas());
        assert!((pre_atenuacao_de(&e) - 0.251).abs() < 0.001); // -12 dB
    }
}
