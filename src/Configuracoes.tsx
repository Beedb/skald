import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { aplicarTema, ModoTema, temDuasVariantes, TEMAS } from "./tema";

type Normalizacao = "desligada" | "faixa" | "album";
type Preset = "desligado" | "grave" | "agudo" | "bateria" | "voz" | "instrumental";
type Config = {
  crossfade_ms: number;
  normalizacao: Normalizacao;
  equalizador: Preset;
  tema: string;
  modo_tema: ModoTema;
  cores_dinamicas: boolean;
  bandeja: boolean;
  notificar: boolean;
};

const PRESETS: [Preset, string][] = [
  ["desligado", "Desligado"],
  ["grave", "Grave"],
  ["agudo", "Agudo"],
  ["bateria", "Bateria"],
  ["voz", "Voz"],
  ["instrumental", "Instrumental"],
];

/** Configurações (seção 12): aparência e reprodução. */
export default function Configuracoes({ onErro }: { onErro: (m: string) => void }) {
  const [crossfade, setCrossfade] = useState<number | null>(null);
  const [normalizacao, setNormalizacao] = useState<Normalizacao>("faixa");
  const [equalizador, setEqualizador] = useState<Preset>("desligado");
  const [tema, setTema] = useState("brasa");
  const [modo, setModo] = useState<ModoTema>("sistema");
  const [coresDinamicas, setCoresDinamicas] = useState(true);
  const [bandeja, setBandeja] = useState(false);
  const [notificar, setNotificar] = useState(true);
  const [autostart, setAutostart] = useState(false);

  useEffect(() => {
    invoke<Config>("get_settings")
      .then((c) => {
        setCrossfade(c.crossfade_ms / 1000);
        setNormalizacao(c.normalizacao);
        setEqualizador(c.equalizador);
        setTema(c.tema);
        setModo(c.modo_tema);
        setCoresDinamicas(c.cores_dinamicas);
        setBandeja(c.bandeja);
        setNotificar(c.notificar);
      })
      .catch((e) => onErro(String(e)));
    invoke<boolean>("autostart_get")
      .then(setAutostart)
      .catch(() => {});
  }, []);

  function gravarBandeja(ativo: boolean) {
    setBandeja(ativo);
    invoke("set_tray", { ativo }).catch((e) => {
      setBandeja(false);
      onErro(String(e));
    });
  }

  function gravarNotificar(ativo: boolean) {
    setNotificar(ativo);
    invoke("set_notifications", { ativo }).catch((e) => onErro(String(e)));
  }

  function gravarAutostart(ativo: boolean) {
    setAutostart(ativo);
    invoke("set_autostart", { ativo }).catch((e) => {
      setAutostart(!ativo);
      onErro(String(e));
    });
  }

  function gravarAparencia(novoTema: string, novoModo: ModoTema) {
    setTema(novoTema);
    setModo(novoModo);
    aplicarTema(novoTema, novoModo);
    invoke("set_appearance", { tema: novoTema, modo: novoModo }).catch((e) => onErro(String(e)));
  }

  function gravarCoresDinamicas(ativo: boolean) {
    setCoresDinamicas(ativo);
    window.dispatchEvent(new CustomEvent("cores-dinamicas", { detail: ativo }));
    invoke("set_dynamic_colors", { ativo }).catch((e) => onErro(String(e)));
  }

  function gravarEqualizador(preset: Preset) {
    setEqualizador(preset);
    invoke("set_equalizer", { preset }).catch((e) => onErro(String(e)));
  }

  function gravarNormalizacao(modo: Normalizacao) {
    setNormalizacao(modo);
    invoke("set_normalization", { modo }).catch((e) => onErro(String(e)));
  }

  function gravar(segundos: number) {
    invoke<number>("set_crossfade", { ms: Math.round(segundos * 1000) })
      .then((ms) => setCrossfade(ms / 1000))
      .catch((e) => onErro(String(e)));
  }

  if (crossfade === null) return null;
  return (
    <section className="configuracoes">
      <h2>Configurações</h2>
      <h3>Aparência</h3>
      <label className="opcao">
        <span>Tema</span>
        <select value={tema} onChange={(e) => gravarAparencia(e.target.value, modo)}>
          {TEMAS.map((t) => (
            <option key={t.id} value={t.id}>
              {t.nome}
            </option>
          ))}
        </select>
      </label>
      <label className="opcao">
        <span>Claro ou escuro</span>
        <select
          value={modo}
          disabled={!temDuasVariantes(tema)}
          onChange={(e) => gravarAparencia(tema, e.target.value as ModoTema)}
        >
          <option value="sistema">Seguir o sistema</option>
          <option value="claro">Claro</option>
          <option value="escuro">Escuro</option>
        </select>
      </label>
      {!temDuasVariantes(tema) && <p className="nota">Este tema tem uma variante só.</p>}
      <label className="opcao">
        <span>Cores da capa</span>
        <input type="checkbox" checked={coresDinamicas} onChange={(e) => gravarCoresDinamicas(e.target.checked)} />
      </label>
      <p className="nota">
        O destaque da interface e o fundo do player acompanham a capa da música atual, sempre com contraste legível.
      </p>
      <h3>Reprodução</h3>
      <label className="opcao">
        <span>
          Crossfade: <strong>{crossfade === 0 ? "desligado" : `${crossfade} s`}</strong>
        </span>
        <input
          type="range"
          min={0}
          max={12}
          step={1}
          value={crossfade}
          aria-label="Duração do crossfade em segundos"
          onChange={(e) => setCrossfade(Number(e.target.value))}
          onPointerUp={(e) => gravar(Number((e.target as HTMLInputElement).value))}
          onKeyUp={(e) => gravar(Number((e.target as HTMLInputElement).value))}
        />
      </label>
      <p className="nota">
        Sobrepõe o fim de uma música ao início da seguinte. Entre faixas do mesmo álbum não há crossfade: elas são
        emendadas sem pausa (gapless). Vale a partir da próxima troca de música.
      </p>
      <label className="opcao">
        <span>Normalização de volume (ReplayGain)</span>
        <select value={normalizacao} onChange={(e) => gravarNormalizacao(e.target.value as Normalizacao)}>
          <option value="desligada">Desligada</option>
          <option value="faixa">Por faixa</option>
          <option value="album">Por álbum</option>
        </select>
      </label>
      <p className="nota">
        Usa os valores de ReplayGain gravados nos arquivos para equilibrar o volume. "Por faixa" iguala todas as
        músicas; "por álbum" mantém a diferença de volume entre as faixas de um mesmo álbum. Arquivos sem esses
        valores tocam no volume original.
      </p>
      <label className="opcao">
        <span>Equalizador</span>
        <select value={equalizador} onChange={(e) => gravarEqualizador(e.target.value as Preset)}>
          {PRESETS.map(([valor, nome]) => (
            <option key={valor} value={valor}>
              {nome}
            </option>
          ))}
        </select>
      </label>
      <p className="nota">
        Grave e agudo reforçam as pontas; bateria destaca o bumbo e a caixa; voz realça os médios; instrumental dá
        mais clareza e corpo. O volume é compensado para não distorcer. Vale na hora.
      </p>
      <h3>Sistema</h3>
      <label className="opcao">
        <span>Fechar a janela envia para a bandeja</span>
        <input type="checkbox" checked={bandeja} onChange={(e) => gravarBandeja(e.target.checked)} />
      </label>
      <p className="nota">
        Mostra um ícone na bandeja com tocar/pausar, próxima, anterior e sair. Fechar a janela só a esconde e a música
        continua. Desligado, fechar a janela encerra o Skáld.
      </p>
      <label className="opcao">
        <span>Iniciar com o sistema</span>
        <input type="checkbox" checked={autostart} onChange={(e) => gravarAutostart(e.target.checked)} />
      </label>
      <p className="nota">Abre o Skáld ao entrar na sessão. Com a bandeja ligada, ele começa escondido nela.</p>
      <label className="opcao">
        <span>Notificar ao trocar de música</span>
        <input type="checkbox" checked={notificar} onChange={(e) => gravarNotificar(e.target.checked)} />
      </label>
      <p className="nota">Só aparece quando a janela do Skáld não está em primeiro plano.</p>
    </section>
  );
}
