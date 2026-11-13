import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { MediaRow } from "./Biblioteca";
import { formatarDuracao } from "./Biblioteca";

export type EstadoPlayer = {
  id: number | null;
  estado: "parado" | "tocando" | "pausado";
  posicao_ms: number;
  duracao_ms: number;
  erro: string | null;
};

type Props = {
  musica: MediaRow | null;
  estado: EstadoPlayer | null;
};

export default function PlayerBar({ musica, estado }: Props) {
  const [volume, setVolume] = useState(1);
  const [arrastando, setArrastando] = useState<number | null>(null);

  const ativo = estado !== null && estado.estado !== "parado" && musica !== null;
  const duracao = estado?.duracao_ms || musica?.duration_ms || 0;
  const posicao = arrastando ?? (ativo ? estado.posicao_ms : 0);

  function mudarVolume(v: number) {
    setVolume(v);
    invoke("set_volume", { volume: v });
  }

  function soltarProgresso() {
    if (arrastando !== null) invoke("seek", { posicaoMs: Math.round(arrastando) });
    setArrastando(null);
  }

  return (
    <footer className="player">
      <button
        className="play"
        onClick={() => invoke("toggle_pause")}
        disabled={!ativo}
        aria-label={estado?.estado === "tocando" ? "Pausar" : "Tocar"}
        title={estado?.estado === "tocando" ? "Pausar" : "Tocar"}
      >
        {estado?.estado === "tocando" ? "❚❚" : "▶"}
      </button>

      <div className="faixa">
        {ativo ? (
          <>
            <strong>{musica.title ?? "Sem título"}</strong>
            <span>{musica.artist ?? "Artista desconhecido"}</span>
          </>
        ) : (
          <span className="dica">Dê dois cliques numa música para tocar</span>
        )}
      </div>

      <div className="progresso">
        <span className="tempo">{formatarDuracao(posicao) || "0:00"}</span>
        <input
          type="range"
          min={0}
          max={duracao || 1}
          step={1000}
          value={Math.min(posicao, duracao || 1)}
          disabled={!ativo || !duracao}
          aria-label="Posição"
          onChange={(e) => setArrastando(Number(e.target.value))}
          onPointerUp={soltarProgresso}
          onKeyUp={soltarProgresso}
        />
        <span className="tempo">{formatarDuracao(duracao) || "0:00"}</span>
      </div>

      <label className="volume" title={`Volume: ${Math.round(volume * 100)}%`}>
        <span aria-hidden>🔊</span>
        <input
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={volume}
          aria-label="Volume"
          onChange={(e) => mudarVolume(Number(e.target.value))}
        />
      </label>
    </footer>
  );
}
