import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMenuMusica } from "./MenuMusica";
import { useVirtualizer } from "@tanstack/react-virtual";

export type MediaRow = {
  id: number;
  title: string | null;
  artist: string | null;
  album: string | null;
  duration_ms: number | null;
  missing: boolean;
};

const PAGINA = 500;
const ALTURA_LINHA = 36;

export function formatarDuracao(ms: number | null) {
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
  /** Texto da busca (vazio: biblioteca toda). Mudar a busca recarrega a lista e volta ao topo. */
  busca: string;
  tocandoId: number | null;
  onTocar: (id: number) => void;
  curtidas: Set<number>;
  onCurtir: (mediaId: number) => void;
  /** Falso enquanto o player completo ocupa a tela; a lista fica montada e guarda a rolagem. */
  visivel: boolean;
};

export function BotaoCurtir({ curtida, onClick }: { curtida: boolean; onClick: () => void }) {
  return (
    <button
      className={"curtir" + (curtida ? " curtida" : "")}
      onClick={onClick}
      onDoubleClick={(e) => e.stopPropagation()}
      aria-pressed={curtida}
      aria-label={curtida ? "Descurtir" : "Curtir"}
      title={curtida ? "Remover de Curtidas" : "Adicionar a Curtidas"}
    >
      {curtida ? "♥" : "♡"}
    </button>
  );
}

export default function Biblioteca({
  total,
  versao,
  busca,
  tocandoId,
  onTocar,
  curtidas,
  onCurtir,
  visivel,
}: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const rolagem = useRef(0);

  // Ao voltar do player completo, a lista reaparece na mesma posição.
  useLayoutEffect(() => {
    if (visivel && scrollRef.current) scrollRef.current.scrollTop = rolagem.current;
  }, [visivel]);
  const paginas = useRef(new Map<number, MediaRow[]>());
  const pedidas = useRef(new Set<number>());
  const chave = `${versao}\u0000${busca}`;
  const versaoAtual = useRef(chave);
  const buscaAnterior = useRef(busca);
  const [, setTick] = useState(0);
  const abrirMenu = useMenuMusica();
  // Seleção múltipla: clique, Ctrl+clique (alterna) e Shift+clique (intervalo).
  const [selecao, setSelecao] = useState<Set<number>>(new Set());
  const ancora = useRef<number | null>(null);

  useEffect(() => {
    if (selecao.size === 0) return;
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && setSelecao(new Set());
    window.addEventListener("keydown", tecla);
    return () => window.removeEventListener("keydown", tecla);
  }, [selecao]);

  function clicar(e: React.MouseEvent, indice: number, id: number) {
    if (e.shiftKey && ancora.current !== null) {
      const de = ancora.current;
      invoke<number[]>("media_ids_range", { busca, de, ate: indice })
        .then((ids) => setSelecao(new Set(ids)))
        .catch(() => {});
      return;
    }
    ancora.current = indice;
    if (e.ctrlKey || e.metaKey) {
      const s = new Set(selecao);
      if (s.has(id)) s.delete(id);
      else s.add(id);
      setSelecao(s);
    } else {
      setSelecao(new Set([id]));
    }
  }

  function menu(e: React.MouseEvent, indice: number, id: number) {
    let atual = selecao;
    if (!selecao.has(id)) {
      atual = new Set([id]);
      setSelecao(atual);
      ancora.current = indice;
    }
    abrirMenu(e, id, { tocar: () => onTocar(id), selecao: [...atual] });
  }

  const virtualizer = useVirtualizer({
    count: total,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ALTURA_LINHA,
    overscan: 20,
  });

  useEffect(() => {
    versaoAtual.current = chave;
    if (buscaAnterior.current !== busca) {
      buscaAnterior.current = busca;
      rolagem.current = 0;
      if (scrollRef.current) scrollRef.current.scrollTop = 0;
    }
    paginas.current.clear();
    pedidas.current.clear();
    setSelecao(new Set());
    ancora.current = null;
    setTick((t) => t + 1);
  }, [chave]);

  const itens = virtualizer.getVirtualItems();
  const primeira = itens.length ? Math.floor(itens[0].index / PAGINA) : 0;
  const ultima = itens.length ? Math.floor(itens[itens.length - 1].index / PAGINA) : 0;

  // Busca só as páginas que estão (ou estão quase) na tela.
  useEffect(() => {
    for (let p = primeira; p <= ultima; p++) {
      if (pedidas.current.has(p)) continue;
      pedidas.current.add(p);
      const versaoPedida = chave;
      invoke<MediaRow[]>("list_media", { offset: p * PAGINA, limit: PAGINA, busca })
        .then((linhas) => {
          if (versaoPedida !== versaoAtual.current) return;
          paginas.current.set(p, linhas);
          setTick((t) => t + 1);
        })
        .catch(() => pedidas.current.delete(p));
    }
  }, [primeira, ultima, chave]);

  if (total === 0) {
    return (
      <p className="vazio">
        {busca
          ? `Nenhuma música encontrada para “${busca}”.`
          : 'Nenhuma música na biblioteca. Use "Importar pasta" para começar.'}
      </p>
    );
  }

  return (
    <div className="biblioteca">
      <div className="linha cabecalho" role="row">
        <span />
        <span>Título</span>
        <span>Artista</span>
        <span>Álbum</span>
        <span className="duracao">Duração</span>
      </div>
      <div
        ref={scrollRef}
        className="lista"
        role="table"
        onScroll={(e) => {
          if (visivel) rolagem.current = e.currentTarget.scrollTop;
        }}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {itens.map((item) => {
            const m = paginas.current.get(Math.floor(item.index / PAGINA))?.[item.index % PAGINA];
            return (
              <div
                key={item.key}
                role="row"
                className={
                  "linha" +
                  (m?.missing ? " ausente" : "") +
                  (m && m.id === tocandoId ? " atual" : "") +
                  (m && selecao.has(m.id) ? " selecionada" : "")
                }
                aria-selected={m ? selecao.has(m.id) : undefined}
                onClick={(e) => m && clicar(e, item.index, m.id)}
                onDoubleClick={() => m && onTocar(m.id)}
                onContextMenu={(e) => m && menu(e, item.index, m.id)}
                title={m?.missing ? "Arquivo não encontrado" : undefined}
                style={{ height: ALTURA_LINHA, transform: `translateY(${item.start}px)` }}
              >
                {m ? (
                  <>
                    <BotaoCurtir curtida={curtidas.has(m.id)} onClick={() => onCurtir(m.id)} />
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
