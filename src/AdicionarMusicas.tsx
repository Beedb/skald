import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Search } from "lucide-react";
import { formatarDuracao, type MediaRow } from "./Biblioteca";

const ALTURA = 40;
const PAGINA = 200;

type Props = {
  playlistId: number;
  nome: string;
  /** Músicas que já estão na playlist (aparecem marcadas e não são adicionadas de novo). */
  presentes: Set<number>;
  onFechar: () => void;
  onErro: (m: string) => void;
};

/** "Adicionar músicas" da playlist: busca na biblioteca com seleção múltipla. */
export default function AdicionarMusicas({ playlistId, nome, presentes, onFechar, onErro }: Props) {
  const [busca, setBusca] = useState("");
  const [termo, setTermo] = useState("");
  const [lista, setLista] = useState<MediaRow[]>([]);
  const [total, setTotal] = useState(0);
  const [marcadas, setMarcadas] = useState<Set<number>>(new Set());
  const [salvando, setSalvando] = useState(false);
  const carregando = useRef(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Espera a pessoa parar de digitar.
  useEffect(() => {
    const t = setTimeout(() => setTermo(busca), 200);
    return () => clearTimeout(t);
  }, [busca]);

  useEffect(() => {
    let valido = true;
    carregando.current = true;
    Promise.all([
      invoke<number>("count_media", { busca: termo }),
      invoke<MediaRow[]>("list_media", { offset: 0, limit: PAGINA, busca: termo }),
    ])
      .then(([n, l]) => {
        if (!valido) return;
        setTotal(n);
        setLista(l);
        if (scrollRef.current) scrollRef.current.scrollTop = 0;
      })
      .catch((e) => onErro(String(e)))
      .finally(() => {
        carregando.current = false;
      });
    return () => {
      valido = false;
    };
  }, [termo]);

  useEffect(() => {
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && onFechar();
    window.addEventListener("keydown", tecla);
    return () => window.removeEventListener("keydown", tecla);
  }, [onFechar]);

  const virtualizer = useVirtualizer({
    count: lista.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ALTURA,
    overscan: 15,
  });

  // Paginação: perto do fim da lista, busca a próxima página.
  const itens = virtualizer.getVirtualItems();
  const ultimo = itens.length ? itens[itens.length - 1].index : 0;
  useEffect(() => {
    if (carregando.current || lista.length >= total || ultimo < lista.length - 30) return;
    carregando.current = true;
    const pedido = termo;
    invoke<MediaRow[]>("list_media", { offset: lista.length, limit: PAGINA, busca: pedido })
      .then((l) => pedido === termo && setLista((atual) => [...atual, ...l]))
      .catch((e) => onErro(String(e)))
      .finally(() => {
        carregando.current = false;
      });
  }, [ultimo, lista.length, total, termo]);

  function alternar(id: number) {
    if (presentes.has(id)) return;
    setMarcadas((m) => {
      const n = new Set(m);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  }

  async function adicionar() {
    if (marcadas.size === 0) return;
    setSalvando(true);
    try {
      await invoke("playlist_add", { id: playlistId, mediaIds: [...marcadas] });
      onFechar();
    } catch (e) {
      onErro(String(e));
      setSalvando(false);
    }
  }

  return (
    <div className="fundo-modal" onMouseDown={(e) => e.target === e.currentTarget && onFechar()}>
      <section className="modal adicionar-musicas" role="dialog" aria-modal="true" aria-label={`Adicionar músicas a ${nome}`}>
        <header>
          <h2>Adicionar músicas a “{nome}”</h2>
          <button className="fechar-modal" onClick={onFechar} aria-label="Fechar">
            ×
          </button>
        </header>
        <label className="am-busca">
          <Search size={16} aria-hidden="true" />
          <input
            autoFocus
            type="search"
            placeholder="Buscar por título, artista, álbum…"
            aria-label="Buscar músicas"
            value={busca}
            onChange={(e) => setBusca(e.target.value)}
          />
        </label>
        <div ref={scrollRef} className="am-lista" role="listbox" aria-multiselectable="true">
          {lista.length === 0 ? (
            <p className="nota">{termo ? "Nada encontrado." : "A biblioteca está vazia."}</p>
          ) : (
            <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
              {itens.map((v) => {
                const m = lista[v.index];
                const ja = presentes.has(m.id);
                const marcada = ja || marcadas.has(m.id);
                return (
                  <label
                    key={m.id}
                    role="option"
                    aria-selected={marcada}
                    className={"am-item" + (ja ? " ja" : "") + (marcadas.has(m.id) ? " marcada" : "")}
                    style={{ height: ALTURA, transform: `translateY(${v.start}px)` }}
                    title={ja ? "Já está na playlist" : undefined}
                  >
                    <input type="checkbox" checked={marcada} disabled={ja} onChange={() => alternar(m.id)} />
                    <span className="faixa">
                      <strong>{m.title ?? "Sem título"}</strong>
                      <span>
                        {m.artist ?? "Artista desconhecido"}
                        {m.album ? ` · ${m.album}` : ""}
                      </span>
                    </span>
                    <span className="tempo">{ja ? "Na playlist" : formatarDuracao(m.duration_ms)}</span>
                  </label>
                );
              })}
            </div>
          )}
        </div>
        <footer>
          <span className="nota">
            {marcadas.size === 0
              ? `${total} ${total === 1 ? "música" : "músicas"}`
              : `${marcadas.size} ${marcadas.size === 1 ? "selecionada" : "selecionadas"}`}
            {marcadas.size > 0 && (
              <button className="link" onClick={() => setMarcadas(new Set())}>
                Limpar seleção
              </button>
            )}
          </span>
          <span className="am-botoes">
            <button onClick={onFechar}>Cancelar</button>
            <button className="primario" onClick={adicionar} disabled={marcadas.size === 0 || salvando}>
              Adicionar{marcadas.size > 0 ? ` ${marcadas.size}` : ""}
            </button>
          </span>
        </footer>
      </section>
    </div>
  );
}
