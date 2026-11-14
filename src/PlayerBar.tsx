import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { MediaRow } from "./Biblioteca";
import { BotaoCurtir, formatarDuracao } from "./Biblioteca";
import type { Repetir } from "./Fila";
import Temporizador from "./Temporizador";

export type EstadoPlayer = {
  id: number | null;
  estado: "parado" | "tocando" | "pausado";
  posicao_ms: number;
  duracao_ms: number;
  erro: string | null;
};

/** Estado de reprodução compartilhado pelo mini-player e pelo player completo. */
export type Reproducao = {
  musica: MediaRow | null;
  estado: EstadoPlayer | null;
  capa: string | null;
  volume: number;
  onVolume: (v: number) => void;
  onErro: (mensagem: string) => void;
  curtida: boolean;
  onCurtir: () => void;
  aleatorio: boolean;
  repetir: Repetir;
};

const PROXIMO_REPETIR: Record<Repetir, Repetir> = { desligado: "fila", fila: "faixa", faixa: "desligado" };
const ROTULO_REPETIR: Record<Repetir, string> = {
  desligado: "Repetir: desligado",
  fila: "Repetir: fila",
  faixa: "Repetir: faixa",
};

const ativo = (r: Reproducao) => r.estado !== null && r.estado.estado !== "parado" && r.musica !== null;

export function Capa({ src, className }: { src: string | null; className: string }) {
  return src ? (
    <img className={"capa " + className} src={src} alt="Capa do álbum" draggable={false} />
  ) : (
    <div className={"capa sem-capa " + className} aria-label="Sem capa">
      ♪
    </div>
  );
}

export function Controles({ r }: { r: Reproducao }) {
  const tocando = r.estado?.estado === "tocando";
  const comando = (nome: string, args?: Record<string, unknown>) =>
    invoke(nome, args).catch((e) => r.onErro(String(e)));
  return (
    <div className="controles">
      <button
        className={"modo" + (r.aleatorio ? " ligado" : "")}
        onClick={() => comando("set_shuffle", { ativo: !r.aleatorio })}
        aria-pressed={r.aleatorio}
        aria-label="Aleatório"
        title={r.aleatorio ? "Aleatório: ligado" : "Aleatório: desligado"}
      >
        🔀
      </button>
      <button onClick={() => comando("previous_track")} disabled={!r.musica} aria-label="Anterior" title="Anterior">
        ⏮
      </button>
      <button
        className="play"
        onClick={() => comando("toggle_pause")}
        disabled={!ativo(r)}
        aria-label={tocando ? "Pausar" : "Tocar"}
        title={tocando ? "Pausar" : "Tocar"}
      >
        {tocando ? "❚❚" : "▶"}
      </button>
      <button onClick={() => comando("next_track")} disabled={!r.musica} aria-label="Próxima" title="Próxima">
        ⏭
      </button>
      <button
        className={"modo" + (r.repetir !== "desligado" ? " ligado" : "")}
        onClick={() => comando("set_repeat", { modo: PROXIMO_REPETIR[r.repetir] })}
        aria-pressed={r.repetir !== "desligado"}
        aria-label={ROTULO_REPETIR[r.repetir]}
        title={ROTULO_REPETIR[r.repetir]}
      >
        {r.repetir === "faixa" ? "🔂" : "🔁"}
      </button>
    </div>
  );
}

export function Progresso({ r }: { r: Reproducao }) {
  const [arrastando, setArrastando] = useState<number | null>(null);
  const duracao = r.estado?.duracao_ms || r.musica?.duration_ms || 0;
  const posicao = arrastando ?? (ativo(r) ? r.estado!.posicao_ms : 0);

  function soltar() {
    if (arrastando !== null) invoke("seek", { posicaoMs: Math.round(arrastando) });
    setArrastando(null);
  }

  return (
    <div className="progresso">
      <span className="tempo">{formatarDuracao(posicao) || "0:00"}</span>
      <input
        type="range"
        min={0}
        max={duracao || 1}
        step={1000}
        value={Math.min(posicao, duracao || 1)}
        disabled={!ativo(r) || !duracao}
        aria-label="Posição"
        onChange={(e) => setArrastando(Number(e.target.value))}
        onPointerUp={soltar}
        onKeyUp={soltar}
      />
      <span className="tempo">{formatarDuracao(duracao) || "0:00"}</span>
    </div>
  );
}

export function Volume({ r }: { r: Reproducao }) {
  return (
    <label className="volume" title={`Volume: ${Math.round(r.volume * 100)}%`}>
      <span aria-hidden>🔊</span>
      <input
        type="range"
        min={0}
        max={1}
        step={0.01}
        value={r.volume}
        aria-label="Volume"
        onChange={(e) => r.onVolume(Number(e.target.value))}
      />
    </label>
  );
}

type Props = {
  r: Reproducao;
  filaAberta: boolean;
  onAlternarFila: () => void;
  onAbrirPlayer: () => void;
};

/** Mini-player fixo no rodapé. Clicar na capa ou no título abre o player completo. */
export default function PlayerBar({ r, filaAberta, onAlternarFila, onAbrirPlayer }: Props) {
  const m = r.musica;
  return (
    <footer className="player">
      <Controles r={r} />

      {ativo(r) && m ? (
        <button className="faixa-mini" onClick={onAbrirPlayer} title="Abrir o player completo">
          <Capa src={r.capa} className="capa-mini" />
          <span className="faixa">
            <strong>{m.title ?? "Sem título"}</strong>
            <span>{m.artist ?? "Artista desconhecido"}</span>
          </span>
        </button>
      ) : (
        <span className="dica">Dê dois cliques numa música para tocar</span>
      )}
      {ativo(r) && m ? <BotaoCurtir curtida={r.curtida} onClick={r.onCurtir} /> : <span />}

      <Progresso r={r} />
      <Volume r={r} />

      <div className="extras">
        <Temporizador onErro={r.onErro} />
        <button
          className={"botao-fila" + (filaAberta ? " ativo" : "")}
          onClick={onAlternarFila}
          aria-pressed={filaAberta}
          title="Mostrar ou esconder a fila"
        >
          Fila
        </button>
      </div>
    </footer>
  );
}
