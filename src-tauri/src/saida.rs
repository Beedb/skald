//! Seletor de saída de áudio (seção 8.8): lista as saídas pelo `DeviceMonitor` do GStreamer
//! e cria o elemento de saída da escolhida. Sem escolha (ou se ela sumiu), o `playbin` usa a
//! saída padrão do sistema. No Android quem escolhe é o sistema (fone, Bluetooth, alto-falante).

use gstreamer as gst;
use gst::prelude::*;
use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Saida {
    /// Identificação estável entre aberturas do app (o nome do nó no PipeWire, o id do
    /// dispositivo no Windows); senão o nome mostrado.
    pub chave: String,
    pub nome: String,
}

/// Propriedades que identificam o dispositivo sem mudar a cada boot, na ordem de preferência.
const CHAVES: &[&str] = &["node.name", "device.strid", "device.id", "device.string", "udev.id"];

fn chave_de(d: &gst::Device) -> String {
    let props = d.properties();
    CHAVES
        .iter()
        .find_map(|k| {
            let s = props.as_ref()?;
            // "device.id" do PipeWire é um número que muda; só vale como texto não numérico.
            s.get::<String>(*k).ok().filter(|v| !v.is_empty() && !v.chars().all(|c| c.is_ascii_digit()))
        })
        .unwrap_or_else(|| d.display_name().to_string())
}

/// Um monitor só, ligado na primeira consulta e nunca parado: os elementos criados a partir
/// de um dispositivo (o `pipewiresink`, por exemplo) seguem usando o provedor do monitor, e
/// parar o monitor com um deles em uso derruba o app (SIGSEGV em
/// `gst_device_provider_device_remove` ao trocar de saída).
fn monitor() -> Option<&'static gst::DeviceMonitor> {
    struct Monitor(gst::DeviceMonitor);
    // O DeviceMonitor do GStreamer pode ser usado de qualquer thread.
    unsafe impl Sync for Monitor {}
    unsafe impl Send for Monitor {}
    static MONITOR: std::sync::OnceLock<Option<Monitor>> = std::sync::OnceLock::new();
    MONITOR
        .get_or_init(|| {
            let m = gst::DeviceMonitor::new();
            m.add_filter(Some("Audio/Sink"), None);
            m.start().ok().map(|_| Monitor(m))
        })
        .as_ref()
        .map(|m| &m.0)
}

fn dispositivos() -> Vec<gst::Device> {
    monitor().map(|m| m.devices().into_iter().collect()).unwrap_or_default()
}

/// Saídas disponíveis agora (sem repetir o mesmo nome, que alguns sistemas listam por dois
/// caminhos, como PipeWire e PulseAudio).
pub fn listar() -> Vec<Saida> {
    let mut vistas = std::collections::HashSet::new();
    dispositivos()
        .iter()
        .map(|d| Saida { chave: chave_de(d), nome: d.display_name().to_string() })
        .filter(|s| vistas.insert(s.nome.clone()))
        .collect()
}

/// Dois elementos de saída (um por deck) para a saída `chave`, se ela existir agora.
pub fn criar(chave: &str) -> Option<[gst::Element; 2]> {
    let d = dispositivos().into_iter().find(|d| chave_de(d) == chave)?;
    Some([d.create_element(None).ok()?, d.create_element(None).ok()?])
}

#[cfg(test)]
mod testes {
    /// Só confere que listar não trava nem quebra (a máquina de teste pode não ter som).
    #[test]
    fn lista_sem_travar() {
        if crate::player::gst_iniciar().is_err() {
            return;
        }
        let lista = super::listar();
        for s in &lista {
            assert!(!s.chave.is_empty() && !s.nome.is_empty());
            assert!(super::criar(&s.chave).is_some(), "{s:?}");
        }
        assert!(super::criar("saida-que-nao-existe").is_none());
    }
}
