import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { Check, CircleDashed, Download, FolderPlus, Play, RefreshCw, Rss, RotateCcw, Sparkle, X } from "lucide-react";
import Capa from "./Capa";
import { formatarDuracao } from "./Biblioteca";
import { escolherPasta, garantirPermissao, noAndroid, tocarAo } from "./android";

/** Programa na grade (podcasts::Programa). */
export type Programa = {
  id: number;
  titulo: string;
  autor: string | null;
  descricao: string | null;
  capa_media: number | null;
  episodios: number;
  nao_ouvidos: number;
  ultimo_em: number | null;
  assinado: boolean;
  tem_capa: boolean;
};

/** Episódio (podcasts::Episodio). */
export type Episodio = {
  id: number;
  show_id: number;
  programa: string;
  titulo: string;
  publicado_em: number | null;
  descricao: string | null;
  duracao_ms: number | null;
  media_id: number | null;
  ouvido: boolean;
  posicao_ms: number | null;
  ausente: boolean;
  /** Do feed e ainda não baixado. */
  baixavel: boolean;
  tamanho: number | null;
};

type ProgressoDownload = { episodio: number; feito: number; total: number | null };

/** Capa do programa: a baixada do feed ou a do episódio mais novo. */
function CapaPrograma({ p, className }: { p: Programa; className?: string }) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!p.tem_capa) return setUrl(null);
    let valido = true;
    invoke<string | null>("podcast_cover", { id: p.id })
      .then((u) => valido && setUrl(u))
      .catch(() => {});
    return () => {
      valido = false;
    };
  }, [p.id, p.tem_capa]);
  if (url) {
    return (
      <div className={"capa-colecao " + (className ?? "")}>
        <img src={url} alt="" draggable={false} />
      </div>
    );
  }
  return <Capa id={p.capa_media} className={className} />;
}

const mb = (b: number) => `${(b / 1048576).toLocaleString("pt-BR", { maximumFractionDigits: 0 })} MB`;

const plural = (n: number, um: string, varios: string) => `${n} ${n === 1 ? um : varios}`;

const porcentagem = (p: ProgressoDownload) => (p.total ? Math.min(100, (p.feito / p.total) * 100) : 0);
const textoDownload = (p: ProgressoDownload) =>
  p.total ? `Baixando ${Math.round(porcentagem(p))}%` : p.feito ? `Baixando ${mb(p.feito)}` : "Baixando…";

export function dataEpisodio(s: number | null) {
  if (!s) return "";
  return new Date(s * 1000).toLocaleDateString("pt-BR", { day: "numeric", month: "short", year: "numeric" });
}

/** "Novo", "Faltam 12 min" ou "Ouvido". */
export function estadoEpisodio(e: Episodio) {
  if (e.ouvido) return "Ouvido";
  if (e.posicao_ms && e.duracao_ms) {
    const faltam = Math.max(1, Math.round((e.duracao_ms - e.posicao_ms) / 60000));
    return `Faltam ${faltam} min`;
  }
  return "Novo";
}

/** Escolhe uma pasta e importa como Podcasts (cada subpasta vira um programa). */
export async function importarPastaDePodcasts(): Promise<number | null> {
  let pasta: string | null = null;
  if (noAndroid) {
    if (!(await garantirPermissao())) {
      throw new Error("Sem permissão para ler os arquivos. Libere em Configurações do Android → Apps → Skáld → Permissões.");
    }
    pasta = await escolherPasta();
  } else {
    const p = await open({ directory: true, title: "Escolha a pasta de podcasts" });
    pasta = typeof p === "string" ? p : null;
  }
  if (!pasta) return null;
  const r = await invoke<{ adicionados: number; existentes: number }>("import_podcast_folder", { path: pasta });
  return r.adicionados + r.existentes;
}

/** Grade de programas (etapa 55). */
export default function Podcasts({
  versao,
  onAbrir,
  onErro,
}: {
  versao: number;
  onAbrir: (id: number) => void;
  onErro: (m: string) => void;
}) {
  const [programas, setProgramas] = useState<Programa[] | null>(null);
  const [importando, setImportando] = useState(false);
  const [assinando, setAssinando] = useState<string | null>(null);
  const [ocupado, setOcupado] = useState<"assinando" | "atualizando" | null>(null);
  const [aviso, setAviso] = useState<string | null>(null);

  useEffect(() => {
    invoke<Programa[]>("podcast_shows")
      .then(setProgramas)
      .catch((e) => onErro(String(e)));
  }, [versao]);

  async function importar() {
    setImportando(true);
    try {
      await importarPastaDePodcasts();
      setProgramas(await invoke<Programa[]>("podcast_shows"));
    } catch (e) {
      onErro(e instanceof Error ? e.message : String(e));
    } finally {
      setImportando(false);
    }
  }

  async function assinar() {
    if (!assinando?.trim()) return;
    setOcupado("assinando");
    setAviso(null);
    try {
      const id = await invoke<number>("podcast_subscribe", { url: assinando.trim() });
      setAssinando(null);
      onAbrir(id);
    } catch (e) {
      onErro(String(e));
    } finally {
      setOcupado(null);
    }
  }

  async function atualizarTudo() {
    setOcupado("atualizando");
    setAviso(null);
    try {
      const r = await invoke<{ novos: number; falhas: string[] }>("podcast_refresh", { id: null });
      setAviso(
        (r.novos ? plural(r.novos, "episódio novo", "episódios novos") : "Nenhum episódio novo") +
          (r.falhas.length ? `. Não deu para atualizar: ${r.falhas.join("; ")}` : "."),
      );
    } catch (e) {
      onErro(String(e));
    } finally {
      setOcupado(null);
    }
  }

  const algumAssinado = programas?.some((p) => p.assinado) ?? false;

  return (
    <section className="colecao">
      <div className="podcasts-topo">
        <h2>
          <Sparkle size={18} className="estrela" aria-hidden="true" />
          Podcasts
        </h2>
        <div className="podcasts-acoes">
          {algumAssinado && (
            <button onClick={atualizarTudo} disabled={ocupado !== null} title="Busca episódios novos de todos os assinados">
              <RefreshCw size={16} aria-hidden="true" className={ocupado === "atualizando" ? "girando" : ""} />
              {ocupado === "atualizando" ? "Atualizando…" : "Atualizar"}
            </button>
          )}
          <button onClick={() => setAssinando(assinando === null ? "" : null)} aria-expanded={assinando !== null}>
            <Rss size={16} aria-hidden="true" />
            Assinar por RSS…
          </button>
          <button onClick={importar} disabled={importando}>
            <FolderPlus size={16} aria-hidden="true" />
            {importando ? "Importando…" : "Importar pasta…"}
          </button>
        </div>
      </div>
      {assinando !== null && (
        <form
          className="assinar-rss"
          onSubmit={(e) => {
            e.preventDefault();
            assinar();
          }}
        >
          <label htmlFor="url-feed">Endereço do feed (RSS) do podcast</label>
          <div className="assinar-linha">
            <input
              id="url-feed"
              type="url"
              inputMode="url"
              placeholder="https://…"
              value={assinando}
              onChange={(e) => setAssinando(e.target.value)}
              autoFocus
              required
            />
            <button type="submit" className="primario" disabled={ocupado !== null}>
              {ocupado === "assinando" ? "Lendo o feed…" : "Assinar"}
            </button>
          </div>
          <p className="nota">
            O endereço costuma aparecer como “RSS” no site do podcast. A internet só é usada para ler a lista e baixar os
            episódios que você escolher.
          </p>
        </form>
      )}
      {aviso && <p className="nota destaque">{aviso}</p>}
      <div className="colecao-rolagem">
        {programas && programas.length === 0 && (
          <p className="vazio">
            Nenhum podcast ainda. Assine pelo endereço RSS ou importe uma pasta de episódios (cada subpasta vira um
            programa). Nada é apagado nem movido.
          </p>
        )}
        <div className="grade-programas">
          {programas?.map((p) => (
            <button key={p.id} className="cartao-album" onClick={() => onAbrir(p.id)} title={p.titulo}>
              <CapaPrograma p={p} />
              <span className="nome">{p.titulo}</span>
              <span className="detalhe">
                {p.nao_ouvidos > 0 ? plural(p.nao_ouvidos, "não ouvido", "não ouvidos") : "Tudo ouvido"}
                {p.autor ? ` · ${p.autor}` : ""}
              </span>
            </button>
          ))}
        </div>
      </div>
    </section>
  );
}

/** Tela do programa: episódios do mais novo ao mais antigo, estado e filtro (etapa 55). */
export function ProgramaView({
  id,
  versao,
  idAtual,
  onVoltar,
  onErro,
}: {
  id: number;
  versao: number;
  idAtual: number | null;
  onVoltar: () => void;
  onErro: (m: string) => void;
}) {
  const [programa, setPrograma] = useState<Programa | null>(null);
  const [episodios, setEpisodios] = useState<Episodio[]>([]);
  const [soNaoOuvidos, setSoNaoOuvidos] = useState(false);
  const [aberto, setAberto] = useState<number | null>(null);
  const [baixando, setBaixando] = useState<Record<number, ProgressoDownload>>({});
  const [atualizando, setAtualizando] = useState(false);
  const [aviso, setAviso] = useState<string | null>(null);

  useEffect(() => {
    const parar = listen<ProgressoDownload>("podcast-download", (e) => {
      const p = e.payload;
      setBaixando((b) => (b[p.episodio] && p.total !== 0 ? { ...b, [p.episodio]: p } : b));
    });
    return () => {
      parar.then((f) => f());
    };
  }, []);

  useEffect(() => {
    let valido = true;
    Promise.all([invoke<Programa | null>("podcast_show", { id }), invoke<Episodio[]>("podcast_episodes", { id })])
      .then(([p, e]) => {
        if (!valido) return;
        setPrograma(p);
        setEpisodios(e);
      })
      .catch((e) => onErro(String(e)));
    return () => {
      valido = false;
    };
  }, [id, versao]);

  const tocar = (e: Episodio) => invoke("play_episode", { id: e.id }).catch((x) => onErro(String(x)));

  async function baixar(e: Episodio) {
    setBaixando((b) => ({ ...b, [e.id]: { episodio: e.id, feito: 0, total: e.tamanho } }));
    try {
      await invoke("podcast_download", { id: e.id });
    } catch (x) {
      if (String(x) !== "Download cancelado.") onErro(String(x));
    } finally {
      setBaixando((b) => {
        const { [e.id]: _, ...resto } = b;
        return resto;
      });
    }
  }

  async function atualizar() {
    setAtualizando(true);
    setAviso(null);
    try {
      const r = await invoke<{ novos: number; falhas: string[] }>("podcast_refresh", { id });
      setAviso(r.falhas.length ? r.falhas.join("; ") : r.novos ? plural(r.novos, "episódio novo", "episódios novos") : "Nenhum episódio novo.");
    } catch (x) {
      onErro(String(x));
    } finally {
      setAtualizando(false);
    }
  }

  async function cancelarAssinatura() {
    const ok = await ask(
      "O programa para de ser atualizado e os episódios não baixados saem da lista. Os episódios baixados (e os arquivos) ficam.",
      { title: "Cancelar assinatura", kind: "warning", okLabel: "Cancelar assinatura", cancelLabel: "Voltar" },
    );
    if (!ok) return;
    invoke("podcast_unsubscribe", { id }).catch((x) => onErro(String(x)));
  }
  const marcar = (e: Episodio, ouvido: boolean) =>
    invoke("podcast_mark", { id: e.id, ouvido }).catch((x) => onErro(String(x)));

  const lista = soNaoOuvidos ? episodios.filter((e) => !e.ouvido) : episodios;
  // "Continuar": o começado mais recente; senão o não ouvido mais antigo.
  const seguinte =
    episodios.find((e) => !e.ouvido && (e.posicao_ms ?? 0) > 0 && !e.ausente && e.media_id !== null) ??
    [...episodios].reverse().find((e) => !e.ouvido && !e.ausente && e.media_id !== null);

  if (!programa) return <section className="colecao" />;
  return (
    <section className="colecao">
      <button className="voltar" onClick={onVoltar}>
        ← Podcasts
      </button>
      <div className="cabecalho-colecao">
        <CapaPrograma p={programa} className="grande" />
        <div className="info-album">
          <h2>{programa.titulo}</h2>
          {programa.autor && <p className="detalhe">{programa.autor}</p>}
          <p className="detalhe">
            {plural(programa.episodios, "episódio", "episódios")} ·{" "}
            {programa.nao_ouvidos > 0 ? plural(programa.nao_ouvidos, "não ouvido", "não ouvidos") : "tudo ouvido"}
          </p>
          {programa.descricao && <p className="descricao-programa">{programa.descricao}</p>}
          <div className="acoes-album">
            <button className="primario" disabled={!seguinte} onClick={() => seguinte && tocar(seguinte)}>
              ▶ {seguinte && (seguinte.posicao_ms ?? 0) > 0 ? "Continuar" : "Tocar"}
            </button>
            {programa.assinado && (
              <>
                <button onClick={atualizar} disabled={atualizando}>
                  <RefreshCw size={14} aria-hidden="true" className={atualizando ? "girando" : ""} />
                  {atualizando ? "Atualizando…" : "Atualizar"}
                </button>
                <button onClick={cancelarAssinatura}>Cancelar assinatura…</button>
              </>
            )}
            <label className="filtro-episodios">
              <input type="checkbox" checked={soNaoOuvidos} onChange={(e) => setSoNaoOuvidos(e.target.checked)} />
              Só não ouvidos
            </label>
          </div>
          {aviso && <p className="nota destaque">{aviso}</p>}
        </div>
      </div>
      <div className="colecao-rolagem">
        {lista.length === 0 && <p className="vazio">Nenhum episódio não ouvido.</p>}
        <ol className="episodios">
          {lista.map((e) => {
            const progresso = !e.ouvido && e.posicao_ms && e.duracao_ms ? (e.posicao_ms / e.duracao_ms) * 100 : 0;
            return (
              <li
                key={e.id}
                className={
                  (e.media_id === idAtual ? "atual " : "") + (e.ouvido ? "ouvido " : "") + (e.ausente ? "ausente" : "")
                }
              >
                {e.baixavel ? (
                  baixando[e.id] ? (
                    <button
                      className="tocar-episodio baixando"
                      onClick={() => invoke("podcast_download_cancel", { id: e.id })}
                      aria-label={`Cancelar o download de ${e.titulo}`}
                      title="Cancelar o download"
                    >
                      <X size={14} />
                    </button>
                  ) : (
                    <button
                      className="tocar-episodio"
                      onClick={() => baixar(e)}
                      aria-label={`Baixar ${e.titulo}`}
                      title={e.tamanho ? `Baixar (${mb(e.tamanho)})` : "Baixar"}
                    >
                      <Download size={14} />
                    </button>
                  )
                ) : (
                  <button
                    className="tocar-episodio"
                    onClick={() => tocar(e)}
                    disabled={e.ausente}
                    aria-label={`Tocar ${e.titulo}`}
                    title={e.ausente ? "Arquivo não encontrado" : "Tocar"}
                  >
                    <Play size={14} fill="currentColor" />
                  </button>
                )}
                <div className="texto-episodio" {...tocarAo(() => !e.ausente && !e.baixavel && tocar(e))}>
                  <span className="titulo">{e.titulo}</span>
                  <span className="detalhe">
                    {[
                      dataEpisodio(e.publicado_em),
                      formatarDuracao(e.duracao_ms),
                      e.baixavel ? (baixando[e.id] ? textoDownload(baixando[e.id]) : "Não baixado") : estadoEpisodio(e),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
                  {baixando[e.id] && (
                    <span className="progresso-episodio baixando" aria-hidden="true">
                      <span style={{ width: `${porcentagem(baixando[e.id])}%` }} />
                    </span>
                  )}
                  {progresso > 0 && (
                    <span className="progresso-episodio" aria-hidden="true">
                      <span style={{ width: `${progresso}%` }} />
                    </span>
                  )}
                  {e.descricao && (
                    <span
                      className={"descricao-episodio" + (aberto === e.id ? " aberta" : "")}
                      onClick={(x) => {
                        x.stopPropagation();
                        setAberto(aberto === e.id ? null : e.id);
                      }}
                      title={aberto === e.id ? "Mostrar menos" : "Mostrar a descrição inteira"}
                    >
                      {e.descricao}
                    </span>
                  )}
                </div>
                <button
                  className="marcar-episodio"
                  onClick={() => marcar(e, !e.ouvido)}
                  title={e.ouvido ? "Marcar como não ouvido" : "Marcar como ouvido"}
                  aria-label={e.ouvido ? "Marcar como não ouvido" : "Marcar como ouvido"}
                >
                  {e.ouvido ? <RotateCcw size={16} /> : progresso > 0 ? <CircleDashed size={16} /> : <Check size={16} />}
                </button>
              </li>
            );
          })}
        </ol>
      </div>
    </section>
  );
}
