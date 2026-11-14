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

  useEffect(() => {
    invoke<Config>("get_settings")
      .then((c) => {
        setCrossfade(c.crossfade_ms / 1000);
        setNormalizacao(c.normalizacao);
        setEqualizador(c.equalizador);
        setTema(c.tema);
        setModo(c.modo_tema);
      })
      .catch((e) => onErro(String(e)));
  }, []);

  function gravarAparencia(novoTema: string, novoModo: ModoTema) {
    setTema(novoTema);
    setModo(novoModo);
    aplicarTema(novoTema, novoModo);
    invoke("set_appearance", { tema: novoTema, modo: novoModo }).catch((e) => onErro(String(e)));
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
    </section>
  );
}
