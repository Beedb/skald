import { useEffect, useRef, useState } from "react";
import {
  ListMusic,
  Maximize2,
  Pause,
  Play,
  Repeat,
  Repeat1,
  Shuffle,
  SkipBack,
  SkipForward,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
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

export const ativo = (r: Reproducao) => r.estado !== null && r.estado.estado !== "parado" && r.musica !== null;

export function Capa({ src, className }: { src: string | null; className: string }) {
  return src ? (
    <img className={"capa " + className} src={src} alt="Capa do álbum" draggable={false} />
  ) : (
    <div className={"capa sem-capa " + className} aria-label="Sem capa">
      ♪
    </div>
  );
}

export function Controles({ r, grande = false }: { r: Reproducao; grande?: boolean }) {
  const tocando = r.estado?.estado === "tocando";
  const comando = (nome: string, args?: Record<string, unknown>) =>
    invoke(nome, args).catch((e) => r.onErro(String(e)));
  const t = grande ? 22 : 18;
  return (
    <div className={"controles" + (grande ? " grandes" : "")}>
      <button
        className={"modo" + (r.aleatorio ? " ligado" : "")}
        onClick={() => comando("set_shuffle", { ativo: !r.aleatorio })}
        aria-pressed={r.aleatorio}
        aria-label="Aleatório"
        title={r.aleatorio ? "Aleatório: ligado" : "Aleatório: desligado"}
      >
        <Shuffle size={t} />
      </button>
      <button onClick={() => comando("previous_track")} disabled={!r.musica} aria-label="Anterior" title="Anterior">
        <SkipBack size={t} fill="currentColor" />
      </button>
      <button
        className="play"
        onClick={() => comando("toggle_pause")}
        disabled={!ativo(r)}
        aria-label={tocando ? "Pausar" : "Tocar"}
        title={tocando ? "Pausar" : "Tocar"}
      >
        {tocando ? <Pause size={t} fill="currentColor" /> : <Play size={t} fill="currentColor" />}
      </button>
      <button onClick={() => comando("next_track")} disabled={!r.musica} aria-label="Próxima" title="Próxima">
        <SkipForward size={t} fill="currentColor" />
      </button>
      <button
        className={"modo" + (r.repetir !== "desligado" ? " ligado" : "")}
        onClick={() => comando("set_repeat", { modo: PROXIMO_REPETIR[r.repetir] })}
        aria-pressed={r.repetir !== "desligado"}
        aria-label={ROTULO_REPETIR[r.repetir]}
        title={ROTULO_REPETIR[r.repetir]}
      >
        {r.repetir === "faixa" ? <Repeat1 size={t} /> : <Repeat size={t} />}
      </button>
    </div>
  );
}

/** Preenchimento da trilha (a parte já tocada) para os controles deslizantes. */
const trilha = (valor: number, max: number) =>
  ({ "--p": `${max > 0 ? Math.min(100, (valor / max) * 100) : 0}%` }) as React.CSSProperties;

/** Barra de progresso: clicar vai direto ao ponto; arrastar mostra o tempo e busca ao soltar. */
export function Progresso({ r }: { r: Reproducao }) {
  const [arrastando, setArrastando] = useState<number | null>(null);
  // Depois de buscar, mostra o ponto escolhido até o player confirmar (ou 2 s).
  const [pendente, setPendente] = useState<{ ms: number; em: number } | null>(null);
  const [sobre, setSobre] = useState<{ ms: number; x: number } | null>(null);
  const barra = useRef<HTMLDivElement>(null);
  const duracao = r.estado?.duracao_ms || r.musica?.duration_ms || 0;
  const real = ativo(r) ? r.estado!.posicao_ms : 0;
  const habilitada = ativo(r) && duracao > 0;

  useEffect(() => {
    if (pendente && (Math.abs(real - pendente.ms) < 1500 || Date.now() - pendente.em > 2000)) setPendente(null);
  }, [real, pendente]);

  const posicao = arrastando ?? pendente?.ms ?? real;

  const ponto = (clientX: number) => {
    const caixa = barra.current!.getBoundingClientRect();
    const f = Math.min(1, Math.max(0, (clientX - caixa.left) / caixa.width));
    return { ms: f * duracao, x: f * caixa.width };
  };

  function buscar(ms: number) {
    const alvo = Math.round(Math.min(Math.max(0, ms), duracao));
    setPendente({ ms: alvo, em: Date.now() });
    invoke("seek", { posicaoMs: alvo });
  }

  function apertar(e: React.PointerEvent<HTMLDivElement>) {
    if (!habilitada || e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    setArrastando(ponto(e.clientX).ms);
  }

  function mover(e: React.PointerEvent<HTMLDivElement>) {
    if (!habilitada) return;
    const p = ponto(e.clientX);
    setSobre(p);
    if (arrastando !== null) setArrastando(p.ms);
  }

  function soltar(e: React.PointerEvent<HTMLDivElement>) {
    if (arrastando === null) return;
    buscar(ponto(e.clientX).ms);
    setArrastando(null);
  }

  function tecla(e: React.KeyboardEvent<HTMLDivElement>) {
    if (!habilitada) return;
    const passo = { ArrowLeft: -5000, ArrowRight: 5000, ArrowDown: -5000, ArrowUp: 5000 }[e.key];
    if (passo !== undefined) buscar(posicao + passo);
    else if (e.key === "Home") buscar(0);
    else if (e.key === "End") buscar(duracao - 1000);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  const fracao = duracao > 0 ? Math.min(100, (posicao / duracao) * 100) : 0;
  return (
    <div className="progresso">
      <span className="tempo">{formatarDuracao(posicao) || "0:00"}</span>
      <div
        ref={barra}
        className={"barra-progresso" + (habilitada ? "" : " desabilitada") + (arrastando !== null ? " arrastando" : "")}
        role="slider"
        tabIndex={habilitada ? 0 : -1}
        aria-label="Posição"
        aria-valuemin={0}
        aria-valuemax={Math.round(duracao / 1000)}
        aria-valuenow={Math.round(posicao / 1000)}
        aria-valuetext={formatarDuracao(posicao) || "0:00"}
        aria-disabled={!habilitada}
        onPointerDown={apertar}
        onPointerMove={mover}
        onPointerUp={soltar}
        onPointerCancel={() => setArrastando(null)}
        onPointerLeave={() => setSobre(null)}
        onKeyDown={tecla}
      >
        <div className="bp-trilha">
          <div className="bp-tocado" style={{ width: `${fracao}%` }} />
        </div>
        <div className="bp-pino" style={{ left: `${fracao}%` }} />
        {sobre && habilitada && (
          <span className="bp-dica" style={{ left: sobre.x }}>
            {formatarDuracao(arrastando ?? sobre.ms) || "0:00"}
          </span>
        )}
      </div>
      <span className="tempo">{formatarDuracao(duracao) || "0:00"}</span>
    </div>
  );
}

export function Volume({ r }: { r: Reproducao }) {
  const anterior = useRef(1);
  const mudo = r.volume === 0;
  return (
    <div className="volume" title={`Volume: ${Math.round(r.volume * 100)}%`}>
      <button
        className="icone-botao"
        onClick={() => {
          if (mudo) r.onVolume(anterior.current || 1);
          else {
            anterior.current = r.volume;
            r.onVolume(0);
          }
        }}
        aria-label={mudo ? "Ativar som" : "Silenciar"}
        title={mudo ? "Ativar som" : "Silenciar"}
      >
        {mudo ? <VolumeX size={18} /> : r.volume < 0.5 ? <Volume1 size={18} /> : <Volume2 size={18} />}
      </button>
      <input
        type="range"
        className="trilha"
        style={trilha(r.volume, 1)}
        min={0}
        max={1}
        step={0.01}
        value={r.volume}
        aria-label="Volume"
        onChange={(e) => r.onVolume(Number(e.target.value))}
      />
    </div>
  );
}

type Props = {
  r: Reproducao;
  filaAberta: boolean;
  onAlternarFila: () => void;
  onAbrirPlayer: () => void;
};

/** Player fixo no rodapé: faixa à esquerda, controles e progresso no centro, volume e atalhos à direita. */
export default function PlayerBar({ r, filaAberta, onAlternarFila, onAbrirPlayer }: Props) {
  const m = r.musica;
  return (
    <footer className="player">
      <div className="player-faixa">
        {ativo(r) && m ? (
          <>
            <button className="faixa-mini" onClick={onAbrirPlayer} title="Abrir o player completo">
              <Capa src={r.capa} className="capa-mini" />
              <span className="faixa">
                <strong>{m.title ?? "Sem título"}</strong>
                <span>{m.artist ?? "Artista desconhecido"}</span>
              </span>
            </button>
            <BotaoCurtir curtida={r.curtida} onClick={r.onCurtir} />
          </>
        ) : (
          <span className="dica">Dê dois cliques numa música para tocar</span>
        )}
      </div>

      <div className="player-centro">
        <Controles r={r} />
        <Progresso r={r} />
      </div>

      <div className="extras">
        <Volume r={r} />
        <Temporizador onErro={r.onErro} />
        <button
          className={"icone-botao botao-fila" + (filaAberta ? " ativo" : "")}
          onClick={onAlternarFila}
          aria-pressed={filaAberta}
          aria-label="Fila"
          title="Mostrar ou esconder a fila"
        >
          <ListMusic size={18} />
        </button>
        <button className="icone-botao" onClick={onAbrirPlayer} aria-label="Player completo" title="Abrir o player completo">
          <Maximize2 size={18} />
        </button>
      </div>
    </footer>
  );
}
