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

fn dispositivos() -> Vec<gst::Device> {
    let monitor = gst::DeviceMonitor::new();
    monitor.add_filter(Some("Audio/Sink"), None);
    if monitor.start().is_err() {
        return Vec::new();
    }
    let lista = monitor.devices().into_iter().collect();
    monitor.stop();
    lista
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
