import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useVirtualizer } from "@tanstack/react-virtual";
import { formatarDuracao } from "./Biblioteca";

type Entrada = {
  id: number;
  media_id: number | null;
  title: string | null;
  artist: string | null;
  album: string | null;
  started_at: number;
  listened_ms: number;
  completed: boolean;
  counted: boolean;
  missing: boolean;
};

const PAGINA = 200;
const ALTURA_LINHA = 36;

function quando(unix: number) {
  const d = new Date(unix * 1000);
  const hora = d.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
  const hoje = new Date();
  const ontem = new Date(hoje.getFullYear(), hoje.getMonth(), hoje.getDate() - 1);
  if (d.toDateString() === hoje.toDateString()) return `hoje, ${hora}`;
  if (d.toDateString() === ontem.toDateString()) return `ontem, ${hora}`;
  return `${d.toLocaleDateString("pt-BR")} ${hora}`;
}

type Props = {
  /** Muda quando o histórico recebe uma reprodução nova ou encerrada. */
  versao: number;
  visivel: boolean;
  onTocar: (mediaId: number) => void;
};

/** Histórico de reprodução, da mais recente para a mais antiga. */
export default function Historico({ versao, visivel, onTocar }: Props) {
  const [total, setTotal] = useState(0);
  const scrollRef = useRef<HTMLDivElement>(null);
  const paginas = useRef(new Map<number, Entrada[]>());
  const pedidas = useRef(new Set<number>());
  const versaoAtual = useRef(versao);
  const [, setTick] = useState(0);

  useEffect(() => {
    if (!visivel) return;
    versaoAtual.current = versao;
    paginas.current.clear();
    pedidas.current.clear();
    invoke<number>("history_count").then(setTotal).catch(() => setTotal(0));
    setTick((t) => t + 1);
  }, [versao, visivel]);

  const virtualizer = useVirtualizer({
    count: total,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ALTURA_LINHA,
    overscan: 20,
  });

  const itens = virtualizer.getVirtualItems();
  const primeira = itens.length ? Math.floor(itens[0].index / PAGINA) : 0;
  const ultima = itens.length ? Math.floor(itens[itens.length - 1].index / PAGINA) : 0;

  useEffect(() => {
    if (!visivel) return;
    for (let p = primeira; p <= ultima; p++) {
      if (pedidas.current.has(p)) continue;
      pedidas.current.add(p);
      const pedida = versaoAtual.current;
      invoke<Entrada[]>("history_list", { offset: p * PAGINA, limit: PAGINA })
        .then((linhas) => {
          if (pedida !== versaoAtual.current) return;
          paginas.current.set(p, linhas);
          setTick((t) => t + 1);
        })
        .catch(() => pedidas.current.delete(p));
    }
  }, [primeira, ultima, versao, visivel, total]);

  return (
    <section className="historico" aria-label="Histórico de reprodução">
      <header className="pl-cabecalho historico-cabecalho">
        <div className="pl-titulos">
          <h2>Histórico</h2>
          <span className="pl-meta">
            {total} {total === 1 ? "reprodução" : "reproduções"} · ✓ = escutada (mais de 60% e no mínimo 20 s)
          </span>
        </div>
      </header>

      {total === 0 ? (
        <p className="vazio">Nada tocado ainda. As músicas que você ouvir aparecem aqui.</p>
      ) : (
        <>
          <div className="linha linha-hist cabecalho" role="row">
            <span>Quando</span>
            <span>Título</span>
            <span>Artista</span>
            <span className="duracao">Ouvido</span>
            <span className="escutada" title="Escutada">
              ✓
            </span>
          </div>
          <div ref={scrollRef} className="lista" role="table">
            <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
              {itens.map((v) => {
                const e = paginas.current.get(Math.floor(v.index / PAGINA))?.[v.index % PAGINA];
                const tocavel = e && e.media_id !== null && !e.missing;
                return (
                  <div
                    key={v.key}
                    role="row"
                    className={"linha linha-hist" + (e && !tocavel ? " ausente" : "")}
                    title={
                      e && !tocavel
                        ? e.media_id === null
                          ? "Música removida da biblioteca"
                          : "Arquivo não encontrado"
                        : undefined
                    }
                    style={{ height: ALTURA_LINHA, transform: `translateY(${v.start}px)` }}
                    onDoubleClick={() => tocavel && onTocar(e.media_id!)}
                  >
                    {e ? (
                      <>
                        <span className="quando">{quando(e.started_at)}</span>
                        <span>{e.title ?? "Sem título"}</span>
                        <span>{e.artist ?? "Artista desconhecido"}</span>
                        <span className="duracao">{formatarDuracao(e.listened_ms) || "0:00"}</span>
                        <span className="escutada">{e.counted ? "✓" : ""}</span>
                      </>
                    ) : (
                      <span className="carregando">…</span>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        </>
      )}
    </section>
  );
}
