import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
  onEnfileirar: (id: number, aSeguir: boolean) => void;
  /** Falso enquanto o player completo ocupa a tela; a lista fica montada e guarda a rolagem. */
  visivel: boolean;
};

type Menu = { x: number; y: number; id: number };

export default function Biblioteca({ total, versao, busca, tocandoId, onTocar, onEnfileirar, visivel }: Props) {
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
  const [menu, setMenu] = useState<Menu | null>(null);

  // Fecha o menu de contexto ao clicar fora, rolar ou apertar Esc.
  useEffect(() => {
    if (!menu) return;
    const fechar = () => setMenu(null);
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && fechar();
    window.addEventListener("click", fechar);
    window.addEventListener("blur", fechar);
    window.addEventListener("keydown", tecla);
    scrollRef.current?.addEventListener("scroll", fechar);
    const lista = scrollRef.current;
    return () => {
      window.removeEventListener("click", fechar);
      window.removeEventListener("blur", fechar);
      window.removeEventListener("keydown", tecla);
      lista?.removeEventListener("scroll", fechar);
    };
  }, [menu]);

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
                className={"linha" + (m?.missing ? " ausente" : "") + (m && m.id === tocandoId ? " atual" : "")}
                onDoubleClick={() => m && onTocar(m.id)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  if (m)
                    setMenu({
                      x: Math.min(e.clientX, window.innerWidth - 200),
                      y: Math.min(e.clientY, window.innerHeight - 120),
                      id: m.id,
                    });
                }}
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
      {menu && (
        <ul className="menu-contexto" role="menu" style={{ left: menu.x, top: menu.y }}>
          <li role="menuitem" onClick={() => onTocar(menu.id)}>
            Tocar agora
          </li>
          <li role="menuitem" onClick={() => onEnfileirar(menu.id, true)}>
            Tocar a seguir
          </li>
          <li role="menuitem" onClick={() => onEnfileirar(menu.id, false)}>
            Adicionar à fila
          </li>
        </ul>
      )}
    </div>
  );
}
