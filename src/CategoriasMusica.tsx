import { useEffect, useRef, useState } from "react";
import { emSegundoPlano } from "./tarefaLonga";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Play, Tags, X } from "lucide-react";
import Capa from "./Capa";
import { tocarAo } from "./android";
import { formatarDuracao } from "./Biblioteca";
import { useMenuMusica } from "./MenuMusica";

/**
 * Categorias (várias por música): "MPB", "Samba", "Anos 70"... Ficam só no banco do Skáld.
 * Vêm de sugestões (gênero do arquivo, MusicBrainz e década) que a pessoa confere, ou à mão.
 * Cada categoria é uma lista tocável; a mesma música aparece em quantas tiver.
 */

export type Categoria = { id: number; nome: string; musicas: number; primeira_media_id: number | null };
type Musica = {
  id: number;
  title: string | null;
  artist: string | null;
  album: string | null;
  duration_ms: number | null;
  missing: boolean;
  categorias: string[];
};
type Sugestao = { nomes: string[]; incerto: boolean };

const EVENTO = "categorias-mudou";
const plural = (n: number, um: string, varios: string) => `${n} ${n === 1 ? um : varios}`;
const normal = (t: string) =>
  t
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .trim();

/** Recarrega quando as categorias mudam (em qualquer tela). */
function useMudancas(versao: number) {
  const [n, setN] = useState(0);
  useEffect(() => {
    const p = listen(EVENTO, () => setN((x) => x + 1));
    return () => void p.then((f) => f());
  }, []);
  return `${versao}-${n}`;
}

/** Grade com todas as categorias. */
export function Categorias({
  versao,
  onAbrir,
  onCompletar,
  onErro,
}: {
  versao: number;
  onAbrir: (id: number) => void;
  onCompletar: () => void;
  onErro: (m: string) => void;
}) {
  const [lista, setLista] = useState<Categoria[] | null>(null);
  const chave = useMudancas(versao);
  useEffect(() => {
    invoke<Categoria[]>("categories_list")
      .then(setLista)
      .catch((e) => onErro(String(e)));
  }, [chave]);

  return (
    <section className="colecao">
      <div className="cabecalho-colecao">
        <div>
          <h2>Categorias</h2>
          {lista && <p className="detalhe">{plural(lista.length, "categoria", "categorias")}</p>}
        </div>
        <button onClick={onCompletar} title="Sugere categorias pela internet (MusicBrainz) para a biblioteca toda">
          <Tags size={16} aria-hidden="true" /> Sugerir categorias…
        </button>
      </div>
      <div className="colecao-rolagem">
        {lista && lista.length === 0 && (
          <p className="vazio">
            Nenhuma categoria ainda. Em "Sugerir categorias…", o Skáld procura o estilo de cada música pela internet (e a
            década do lançamento) e você escolhe o que fica. Também dá para pôr à mão pelo menu da música (clique direito
            ou toque longo → Categorias…). Uma música pode estar em várias categorias.
          </p>
        )}
        <div className="grade-programas">
          {lista?.map((c) => (
            <button key={c.id} className="cartao-album" onClick={() => onAbrir(c.id)} title={c.nome}>
              <Capa id={c.primeira_media_id} />
              <span className="nome">{c.nome}</span>
              <span className="detalhe">{plural(c.musicas, "música", "músicas")}</span>
            </button>
          ))}
        </div>
      </div>
    </section>
  );
}

const ALTURA_LINHA = 44;

/** Uma categoria: tocar, renomear, excluir e as músicas (virtualizadas). */
export function CategoriaView({
  id,
  versao,
  tocandoId,
  onVoltar,
  onErro,
}: {
  id: number;
  versao: number;
  tocandoId: number | null;
  onVoltar: () => void;
  onErro: (m: string) => void;
}) {
  const [nome, setNome] = useState<string | null>(null);
  const [musicas, setMusicas] = useState<Musica[] | null>(null);
  const [editando, setEditando] = useState<string | null>(null);
  const [confirmar, setConfirmar] = useState(false);
  const abrirMenu = useMenuMusica();
  const rolagem = useRef<HTMLDivElement>(null);
  const chave = useMudancas(versao);

  useEffect(() => {
    invoke<string | null>("category_name", { id })
      .then((n) => (n === null ? onVoltar() : setNome(n)))
      .catch((e) => onErro(String(e)));
    invoke<Musica[]>("category_tracks", { id })
      .then(setMusicas)
      .catch((e) => onErro(String(e)));
  }, [id, chave]);

  const virt = useVirtualizer({
    count: musicas?.length ?? 0,
    getScrollElement: () => rolagem.current,
    estimateSize: () => ALTURA_LINHA,
    overscan: 10,
  });

  const fazer = (cmd: string, args: Record<string, unknown>) => invoke(cmd, args).catch((e) => onErro(String(e)));
  const tocar = (indice: number) => fazer("play_category", { id, indice });

  async function renomear() {
    if (editando === null) return;
    const novo = editando.trim();
    setEditando(null);
    if (novo && novo !== nome) await fazer("category_rename", { id, nome: novo });
  }

  if (nome === null || !musicas) return null;
  const duracao = musicas.reduce((t, m) => t + (m.duration_ms ?? 0), 0);
  return (
    <section className="colecao">
      <button className="voltar" onClick={onVoltar}>
        ← Categorias
      </button>
      <div className="cabecalho-colecao">
        <div className="info-album">
          {editando !== null ? (
            <input
              className="renomear-categoria"
              autoFocus
              value={editando}
              onChange={(e) => setEditando(e.target.value)}
              onBlur={renomear}
              onKeyDown={(e) => {
                if (e.key === "Enter") renomear();
                if (e.key === "Escape") setEditando(null);
              }}
              aria-label="Nome da categoria"
            />
          ) : (
            <h2>{nome}</h2>
          )}
          <p className="detalhe">
            {plural(musicas.length, "música", "músicas")} · {Math.round(duracao / 60000)} min
          </p>
          <div className="acoes-album">
            <button className="primario" disabled={musicas.length === 0} onClick={() => tocar(0)}>
              <Play size={16} aria-hidden="true" /> Tocar
            </button>
            <button onClick={() => setEditando(nome)}>Renomear</button>
            {confirmar ? (
              <button className="perigo" onClick={() => fazer("category_delete", { id }).then(onVoltar)}>
                Confirmar: excluir (as músicas ficam)
              </button>
            ) : (
              <button onClick={() => setConfirmar(true)} title="Apaga só a categoria; nenhuma música sai da biblioteca">
                Excluir…
              </button>
            )}
          </div>
        </div>
      </div>
      <div className="colecao-rolagem" ref={rolagem}>
        <div style={{ height: virt.getTotalSize(), position: "relative" }}>
          {virt.getVirtualItems().map((v) => {
            const m = musicas[v.index];
            return (
              <div
                key={m.id}
                className={"linha-categoria" + (m.missing ? " ausente" : "") + (m.id === tocandoId ? " atual" : "")}
                style={{ height: ALTURA_LINHA, transform: `translateY(${v.start}px)` }}
                title={m.missing ? "Arquivo não encontrado" : undefined}
                {...tocarAo(() => tocar(v.index))}
                onContextMenu={(e) => abrirMenu(e, m.id, { tocar: () => tocar(v.index) })}
              >
                <span className="titulo">
                  {m.title ?? "Sem título"}
                  <span className="detalhe"> · {m.artist ?? "Artista desconhecido"}</span>
                </span>
                <span className="duracao">{formatarDuracao(m.duration_ms)}</span>
                <button
                  className="icone-botao"
                  title={`Tirar de "${nome}"`}
                  aria-label={`Tirar de ${nome}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    fazer("category_remove", { id: m.id, nome });
                  }}
                  onDoubleClick={(e) => e.stopPropagation()}
                >
                  <X size={14} aria-hidden="true" />
                </button>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}

/** Janela "Categorias…" de uma música (ou várias selecionadas: só adiciona). */
export function JanelaCategorias({ ids, onFechar, onErro }: { ids: number[]; onFechar: () => void; onErro: (m: string) => void }) {
  const uma = ids.length === 1;
  const [atuais, setAtuais] = useState<string[]>([]);
  const [todas, setTodas] = useState<Categoria[]>([]);
  const [texto, setTexto] = useState("");
  const [sugestoes, setSugestoes] = useState<Sugestao | null>(null);
  const [buscando, setBuscando] = useState(false);

  const recarregar = () => {
    if (uma) invoke<string[]>("media_categories", { id: ids[0] }).then(setAtuais).catch((e) => onErro(String(e)));
    invoke<Categoria[]>("categories_list").then(setTodas).catch(() => {});
  };
  useEffect(recarregar, []);
  useEffect(() => {
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && onFechar();
    window.addEventListener("keydown", tecla);
    return () => window.removeEventListener("keydown", tecla);
  }, []);

  async function adicionar(nomes: string[], sugestao = false) {
    const limpos = nomes.map((n) => n.trim()).filter(Boolean);
    if (!limpos.length) return;
    try {
      for (const id of ids) await invoke("category_add", { id, nomes: limpos, sugestao });
      setTexto("");
      if (!uma) onFechar();
      else recarregar();
    } catch (e) {
      onErro(String(e));
    }
  }

  async function sugerir() {
    setBuscando(true);
    try {
      setSugestoes(await invoke<Sugestao>("category_suggest", { id: ids[0] }));
    } catch (e) {
      onErro(String(e));
    } finally {
      setBuscando(false);
    }
  }

  const tem = new Set(atuais.map(normal));
  const novas = sugestoes?.nomes.filter((n) => !tem.has(normal(n))) ?? [];
  return (
    <div className="fundo-modal" onMouseDown={(e) => e.target === e.currentTarget && onFechar()}>
      <section className="modal categorias-musica" role="dialog" aria-modal="true" aria-label="Categorias">
        <header>
          <h2>{uma ? "Categorias da música" : `Pôr ${ids.length} músicas numa categoria`}</h2>
          <button className="fechar-modal" onClick={onFechar} aria-label="Fechar">
            ×
          </button>
        </header>
        <p className="nota">
          Uma música pode estar em várias categorias (ex.: "MPB" e "Anos 70"). Ficam só no Skáld; os arquivos não mudam.
        </p>
        {uma && (
          <div className="chips">
            {atuais.length === 0 && <span className="detalhe">Nenhuma ainda.</span>}
            {atuais.map((n) => (
              <span key={n} className="chip marcado">
                {n}
                <button
                  className="icone-botao"
                  aria-label={`Tirar de ${n}`}
                  title={`Tirar de "${n}"`}
                  onClick={() =>
                    invoke("category_remove", { id: ids[0], nome: n })
                      .then(recarregar)
                      .catch((e) => onErro(String(e)))
                  }
                >
                  <X size={12} aria-hidden="true" />
                </button>
              </span>
            ))}
          </div>
        )}
        <form
          className="nova-categoria"
          onSubmit={(e) => {
            e.preventDefault();
            adicionar([texto]);
          }}
        >
          <input
            list="lista-categorias"
            value={texto}
            onChange={(e) => setTexto(e.target.value)}
            placeholder="Nome da categoria"
            maxLength={60}
            aria-label="Nome da categoria"
          />
          <datalist id="lista-categorias">
            {todas.map((c) => (
              <option key={c.id} value={c.nome} />
            ))}
          </datalist>
          <button className="primario" type="submit" disabled={!texto.trim()}>
            Adicionar
          </button>
        </form>
        {uma && (
          <div className="sugerir-categorias">
            <button onClick={sugerir} disabled={buscando}>
              {buscando ? "Buscando…" : "Sugerir pela internet"}
            </button>
            {sugestoes && novas.length === 0 && <span className="detalhe">Nada novo para sugerir.</span>}
            {sugestoes && sugestoes.incerto && novas.length > 0 && (
              <span className="detalhe">Achada só pelo título: confira.</span>
            )}
            <div className="chips">
              {novas.map((n) => (
                <button key={n} className="chip" onClick={() => adicionar([n], true)} title="Adicionar">
                  + {n}
                </button>
              ))}
            </div>
          </div>
        )}
        <footer>
          <button onClick={onFechar}>Fechar</button>
        </footer>
      </section>
    </div>
  );
}

const ALTURA_COMPLETAR = 64;
type Proposta = { nomes: string[]; marcados: Set<string>; incerto: boolean } | "nada" | "erro";

/** Aba "Categorias" de Completar informações: sugere para a biblioteca toda e grava as marcadas. */
export function CompletarCategorias({ onMudou, onErro }: { onMudou: (m: string) => void; onErro: (m: string) => void }) {
  const [musicas, setMusicas] = useState<Musica[] | null>(null);
  const [soSem, setSoSem] = useState(true);
  const [propostas, setPropostas] = useState<Map<number, Proposta>>(new Map());
  const [progresso, setProgresso] = useState<{ feitos: number; total: number; acao: string } | null>(null);
  const parar = useRef(false);
  const rolagem = useRef<HTMLDivElement>(null);

  const carregar = () =>
    invoke<Musica[]>("category_tracks", { id: null })
      .then(setMusicas)
      .catch((e) => onErro(String(e)));
  useEffect(() => {
    carregar();
    return () => void (parar.current = true);
  }, []);

  const lista = (musicas ?? []).filter((m) => !soSem || m.categorias.length === 0 || propostas.has(m.id));
  const virt = useVirtualizer({
    count: lista.length,
    getScrollElement: () => rolagem.current,
    estimateSize: () => ALTURA_COMPLETAR,
    overscan: 8,
  });
  const definir = (id: number, p: Proposta) => setPropostas((m) => new Map(m).set(id, p));

  async function buscar() {
    parar.current = false;
    const fila = lista.filter((m) => !propostas.has(m.id));
    await emSegundoPlano("Buscando categorias…", async (avisar) => {
      for (let n = 0; n < fila.length && !parar.current; n++) {
        const m = fila[n];
        setProgresso({ feitos: n, total: fila.length, acao: "Buscando" });
        avisar(`Buscando categorias: ${n + 1} de ${fila.length}`);
        try {
          // O Rust espaça as consultas (uma por segundo, como o MusicBrainz pede).
          const s = await invoke<Sugestao>("category_suggest", { id: m.id });
          const tem = new Set(m.categorias.map(normal));
          const nomes = s.nomes.filter((x) => !tem.has(normal(x)));
          definir(m.id, nomes.length ? { nomes, marcados: new Set(s.incerto ? [] : nomes), incerto: s.incerto } : "nada");
        } catch (e) {
          definir(m.id, "erro");
          if (String(e).includes("Sem conexão")) {
            onErro(String(e));
            break;
          }
        }
      }
    });
    setProgresso(null);
  }

  function alternar(id: number, nome: string) {
    const p = propostas.get(id);
    if (!p || typeof p === "string") return;
    const marcados = new Set(p.marcados);
    if (marcados.has(nome)) marcados.delete(nome);
    else marcados.add(nome);
    definir(id, { ...p, marcados });
  }

  const prontas = lista.filter((m) => {
    const p = propostas.get(m.id);
    return p && typeof p !== "string" && p.marcados.size > 0;
  });

  async function aplicar() {
    let ok = 0;
    for (let n = 0; n < prontas.length; n++) {
      const m = prontas[n];
      const p = propostas.get(m.id) as Exclude<Proposta, string>;
      setProgresso({ feitos: n, total: prontas.length, acao: "Gravando" });
      try {
        await invoke("category_add", { id: m.id, nomes: [...p.marcados], sugestao: true });
        ok++;
      } catch (e) {
        onErro(String(e));
        break;
      }
    }
    setProgresso(null);
    setPropostas(new Map());
    await carregar();
    onMudou(`${plural(ok, "música categorizada", "músicas categorizadas")}.`);
  }

  if (!musicas) return null;
  const buscadas = lista.filter((m) => propostas.has(m.id)).length;
  return (
    <>
      <p className="nota">
        Procura o estilo de cada música no MusicBrainz (precisa de internet, uma música por segundo), junta com o gênero do
        arquivo e a década do lançamento. Uma música pode ficar em várias categorias. Só as marcadas são gravadas, e só no
        Skáld.
      </p>
      <div className="barra-completar">
        <label>
          <input type="checkbox" checked={soSem} onChange={(e) => setSoSem(e.target.checked)} disabled={!!progresso} /> só
          as sem categoria
        </label>
        {progresso ? (
          <>
            <span className="detalhe">
              {progresso.acao} {progresso.feitos + 1} de {progresso.total}…
            </span>
            <button onClick={() => (parar.current = true)}>Parar</button>
          </>
        ) : (
          <>
            <button className="primario" onClick={buscar} disabled={buscadas === lista.length}>
              Buscar categorias ({lista.length - buscadas})
            </button>
            <button className="primario" onClick={aplicar} disabled={prontas.length === 0}>
              Aplicar ({prontas.length})
            </button>
          </>
        )}
      </div>
      {lista.length === 0 && <p className="vazio">Todas as músicas já têm categoria.</p>}
      <div className="colecao-rolagem" ref={rolagem}>
        <div style={{ height: virt.getTotalSize(), position: "relative" }}>
          {virt.getVirtualItems().map((v) => {
            const m = lista[v.index];
            const p = propostas.get(m.id);
            return (
              <div key={m.id} className="linha-completar" style={{ height: ALTURA_COMPLETAR, transform: `translateY(${v.start}px)` }}>
                <div className="quem">
                  <strong>{m.title ?? "Sem título"}</strong>
                  <span className="detalhe">
                    {m.artist ?? "Artista desconhecido"}
                    {m.categorias.length ? ` · já em: ${m.categorias.join(", ")}` : ""}
                  </span>
                </div>
                <div className="sugestao">
                  {p === undefined && <span className="detalhe">—</span>}
                  {p === "nada" && <span className="detalhe">nada novo</span>}
                  {p === "erro" && <span className="detalhe">erro na busca</span>}
                  {p && typeof p !== "string" && p.incerto && <span className="detalhe">confira:</span>}
                  {p &&
                    typeof p !== "string" &&
                    p.nomes.map((n) => (
                      <label key={n} className={"chip" + (p.marcados.has(n) ? " marcado" : "")}>
                        <input type="checkbox" checked={p.marcados.has(n)} onChange={() => alternar(m.id, n)} />
                        {n}
                      </label>
                    ))}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </>
  );
}
