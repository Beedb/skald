import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Check, CircleDashed, FolderPlus, Play, RotateCcw, Sparkle } from "lucide-react";
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
};

const plural = (n: number, um: string, varios: string) => `${n} ${n === 1 ? um : varios}`;

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

  return (
    <section className="colecao">
      <div className="podcasts-topo">
        <h2>
          <Sparkle size={18} className="estrela" aria-hidden="true" />
          Podcasts
        </h2>
        <button onClick={importar} disabled={importando}>
          <FolderPlus size={16} aria-hidden="true" />
          {importando ? "Importando…" : "Importar pasta de podcasts…"}
        </button>
      </div>
      <div className="colecao-rolagem">
        {programas && programas.length === 0 && (
          <p className="vazio">
            Nenhum podcast ainda. Importe uma pasta de episódios: cada subpasta (ou a própria pasta) vira um programa.
            Nada é apagado nem movido.
          </p>
        )}
        <div className="grade-programas">
          {programas?.map((p) => (
            <button key={p.id} className="cartao-album" onClick={() => onAbrir(p.id)} title={p.titulo}>
              <Capa id={p.capa_media} />
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
  const marcar = (e: Episodio, ouvido: boolean) =>
    invoke("podcast_mark", { id: e.id, ouvido }).catch((x) => onErro(String(x)));

  const lista = soNaoOuvidos ? episodios.filter((e) => !e.ouvido) : episodios;
  // "Continuar": o começado mais recente; senão o não ouvido mais antigo.
  const seguinte =
    episodios.find((e) => !e.ouvido && (e.posicao_ms ?? 0) > 0 && !e.ausente) ??
    [...episodios].reverse().find((e) => !e.ouvido && !e.ausente);

  if (!programa) return <section className="colecao" />;
  return (
    <section className="colecao">
      <button className="voltar" onClick={onVoltar}>
        ← Podcasts
      </button>
      <div className="cabecalho-colecao">
        <Capa id={programa.capa_media} className="grande" />
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
            <label className="filtro-episodios">
              <input type="checkbox" checked={soNaoOuvidos} onChange={(e) => setSoNaoOuvidos(e.target.checked)} />
              Só não ouvidos
            </label>
          </div>
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
                <button
                  className="tocar-episodio"
                  onClick={() => tocar(e)}
                  disabled={e.ausente}
                  aria-label={`Tocar ${e.titulo}`}
                  title={e.ausente ? "Arquivo não encontrado" : "Tocar"}
                >
                  <Play size={14} fill="currentColor" />
                </button>
                <div className="texto-episodio" {...tocarAo(() => !e.ausente && tocar(e))}>
                  <span className="titulo">{e.titulo}</span>
                  <span className="detalhe">
                    {[dataEpisodio(e.publicado_em), formatarDuracao(e.duracao_ms), estadoEpisodio(e)]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
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
