//! Utilidades só dos testes.

/// Pasta temporária vazia para um teste: `<temp>/<nome>-<pid>`. Apaga antes o que tiver
/// sobrado: o Windows reaproveita PIDs, e uma pasta que não pôde ser apagada no fim de uma
/// rodada antiga (arquivo ainda aberto) faria o teste achar dados velhos.
pub fn pasta(nome: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// WAV PCM 16 bits mono, 44,1 kHz, com `amostras` amostras. Escrito em Rust: não depende
/// do `gst-launch-1.0` nem do `audiotestsrc` (que não vão no GStreamer embutido do Windows).
/// `semente` muda o conteúdo (arquivos com hash diferente).
pub fn wav(destino: &std::path::Path, amostras: u32, semente: u32) {
    let bytes = amostras * 2;
    let mut w = Vec::with_capacity(44 + bytes as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + bytes).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&44_100u32.to_le_bytes());
    w.extend_from_slice(&88_200u32.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&bytes.to_le_bytes());
    let freq = 440.0 + semente as f64;
    for i in 0..amostras {
        let v = (i as f64 * freq * std::f64::consts::TAU / 44_100.0).sin() * 16_000.0;
        w.extend_from_slice(&(v as i16).to_le_bytes());
    }
    std::fs::write(destino, w).unwrap();
}

/// Roda o `gst-launch-1.0` do GStreamer instalado. Tira do ambiente o GStreamer embutido
/// (no Windows o `cargo test` põe no PATH a cópia de `target\debug`, sem os plugins de
/// codificação) e as variáveis que `player::gst_iniciar` define para os testes.
/// `None`: `gst-launch-1.0` não existe nesta máquina.
pub fn gst_launch(args: &[&str]) -> Option<bool> {
    let caminhos = std::env::var_os("PATH").unwrap_or_default();
    let limpo = std::env::join_paths(
        std::env::split_paths(&caminhos)
            .filter(|p| !(p.join("gstreamer-1.0-0.dll").is_file() && p.join("gstreamer-1.0").is_dir())),
    )
    .ok()?;
    // No Windows o Rust só acrescenta ".exe" a nomes sem extensão, e "1.0" conta como uma.
    let programa = if cfg!(windows) { "gst-launch-1.0.exe" } else { "gst-launch-1.0" };
    let status = std::process::Command::new(programa)
        .arg("-q")
        .args(args)
        .env("PATH", limpo)
        .env_remove("GST_PLUGIN_SYSTEM_PATH_1_0")
        .env_remove("GST_REGISTRY_1_0")
        .env_remove("GST_REGISTRY_FORK")
        .status();
    match status {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => panic!("gst-launch-1.0: {e}"),
        Ok(s) => Some(s.success()),
    }
}

/// "/m/a.mp3" com o separador do sistema ("\m\a.mp3" no Windows).
pub fn nativo(p: &str) -> String {
    p.replace('/', std::path::MAIN_SEPARATOR_STR)
}
