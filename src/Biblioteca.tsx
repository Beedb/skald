import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useVirtualizer } from "@tanstack/react-virtual";

type MediaRow = {
  id: number;
  title: string | null;
  artist: string | null;
  album: string | null;
  duration_ms: number | null;
  missing: boolean;
};

const PAGINA = 500;
const ALTURA_LINHA = 36;

function formatarDuracao(ms: number | null) {
  if (!ms) return "";
  const s = Math.round(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

type Props = {
  total: number;
  /** Muda quando a biblioteca é alterada (ex.: após importar), descartando o cache. */
  versao: number;
};

export default function Biblioteca({ total, versao }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const paginas = useRef(new Map<number, MediaRow[]>());
  const pedidas = useRef(new Set<number>());
  const versaoAtual = useRef(versao);
  const [, setTick] = useState(0);

  const virtualizer = useVirtualizer({
    count: total,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ALTURA_LINHA,
    overscan: 20,
  });

  useEffect(() => {
    versaoAtual.current = versao;
    paginas.current.clear();
    pedidas.current.clear();
    setTick((t) => t + 1);
  }, [versao]);

  const itens = virtualizer.getVirtualItems();
  const primeira = itens.length ? Math.floor(itens[0].index / PAGINA) : 0;
  const ultima = itens.length ? Math.floor(itens[itens.length - 1].index / PAGINA) : 0;

  // Busca só as páginas que estão (ou estão quase) na tela.
  useEffect(() => {
    for (let p = primeira; p <= ultima; p++) {
      if (pedidas.current.has(p)) continue;
      pedidas.current.add(p);
      const versaoPedida = versao;
      invoke<MediaRow[]>("list_media", { offset: p * PAGINA, limit: PAGINA })
        .then((linhas) => {
          if (versaoPedida !== versaoAtual.current) return;
          paginas.current.set(p, linhas);
          setTick((t) => t + 1);
        })
        .catch(() => pedidas.current.delete(p));
    }
  }, [primeira, ultima, versao]);

  if (total === 0) {
    return <p className="vazio">Nenhuma música na biblioteca. Use "Importar pasta" para começar.</p>;
  }

  return (
    <div className="biblioteca">
      <div className="linha cabecalho" role="row">
        <span>Título</span>
        <span>Artista</span>
        <span>Álbum</span>
        <span className="duracao">Duração</span>
      </div>
      <div ref={scrollRef} className="lista" role="table">
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {itens.map((item) => {
            const m = paginas.current.get(Math.floor(item.index / PAGINA))?.[item.index % PAGINA];
            return (
              <div
                key={item.key}
                role="row"
                className={"linha" + (m?.missing ? " ausente" : "")}
                title={m?.missing ? "Arquivo não encontrado" : undefined}
                style={{ height: ALTURA_LINHA, transform: `translateY(${item.start}px)` }}
              >
                {m ? (
                  <>
                    <span>{m.title ?? "Sem título"}</span>
                    <span>{m.artist ?? "Artista desconhecido"}</span>
                    <span>{m.album ?? ""}</span>
                    <span className="duracao">{formatarDuracao(m.duration_ms)}</span>
                  </>
                ) : (
                  <span className="carregando">…</span>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
