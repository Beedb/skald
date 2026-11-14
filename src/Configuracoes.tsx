import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Normalizacao = "desligada" | "faixa" | "album";
type Config = { crossfade_ms: number; normalizacao: Normalizacao };

/** Configurações (seção 12). Por enquanto, só a reprodução. */
export default function Configuracoes({ onErro }: { onErro: (m: string) => void }) {
  const [crossfade, setCrossfade] = useState<number | null>(null);
  const [normalizacao, setNormalizacao] = useState<Normalizacao>("faixa");

  useEffect(() => {
    invoke<Config>("get_settings")
      .then((c) => {
        setCrossfade(c.crossfade_ms / 1000);
        setNormalizacao(c.normalizacao);
      })
      .catch((e) => onErro(String(e)));
  }, []);

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
    </section>
  );
}
