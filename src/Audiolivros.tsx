import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Check, FolderPlus, Play, RotateCcw, Sparkle } from "lucide-react";
import Capa from "./Capa";
import { formatarDuracao } from "./Biblioteca";
import { escolherPasta, garantirPermissao, noAndroid, tocarAo } from "./android";

/** Livro (audiolivros::Livro). */
export type Livro = {
  id: number;
  titulo: string;
  autor: string | null;
  capitulos: number;
  duracao_ms: number;
  ouvido_ms: number;
  capitulo_atual: number;
  capa_media: number | null;
  concluido: boolean;
  ouvido_em: number | null;
};

type Capitulo = { media_id: number; titulo: string; duracao_ms: number; missing: boolean };

const plural = (n: number, um: string, varios: string) => `${n} ${n === 1 ? um : varios}`;

/** "3 h 20 min" ou "45 min". */
export function horas(ms: number) {
  const min = Math.round(ms / 60000);
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  return min % 60 ? `${h} h ${min % 60} min` : `${h} h`;
}

/** "Novo", "Faltam 2 h 10 min" ou "Terminado". */
export function estadoLivro(l: Livro) {
  if (l.concluido) return "Terminado";
  if (l.ouvido_ms > 0) return `Faltam ${horas(Math.max(60000, l.duracao_ms - l.ouvido_ms))}`;
  return "Novo";
}

const porcento = (l: Livro) => (l.duracao_ms > 0 ? Math.min(100, (l.ouvido_ms / l.duracao_ms) * 100) : 0);

async function importarPasta(): Promise<boolean> {
  let pasta: string | null = null;
  if (noAndroid) {
    if (!(await garantirPermissao())) {
      throw new Error("Sem permissão para ler os arquivos. Libere em Configurações do Android → Apps → Skáld → Permissões.");
    }
    pasta = await escolherPasta();
  } else {
    const p = await open({ directory: true, title: "Escolha a pasta de audiolivros" });
    pasta = typeof p === "string" ? p : null;
  }
  if (!pasta) return false;
  await invoke("import_audiobook_folder", { path: pasta });
  return true;
}

/** Estante: livros ouvidos por último primeiro, com o progresso. */
export default function Audiolivros({
  versao,
  onAbrir,
  onErro,
}: {
  versao: number;
  onAbrir: (id: number) => void;
  onErro: (m: string) => void;
}) {
  const [livros, setLivros] = useState<Livro[] | null>(null);
  const [importando, setImportando] = useState(false);

  useEffect(() => {
    invoke<Livro[]>("audiobooks_list")
      .then(setLivros)
      .catch((e) => onErro(String(e)));
  }, [versao]);

  async function importar() {
    setImportando(true);
    try {
      if (await importarPasta()) setLivros(await invoke<Livro[]>("audiobooks_list"));
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
          Audiolivros
        </h2>
        <div className="podcasts-acoes">
          <button onClick={importar} disabled={importando}>
            <FolderPlus size={16} aria-hidden="true" />
            {importando ? "Importando…" : "Importar pasta…"}
          </button>
        </div>
      </div>
      <div className="colecao-rolagem">
        {livros && livros.length === 0 && (
          <p className="vazio">
            Nenhum audiolivro ainda. Importe uma pasta de audiolivros: cada pasta com arquivos dentro vira um livro (um
            .m4b só ou um arquivo por capítulo). Nada é apagado nem movido.
          </p>
        )}
        <div className="grade-programas">
          {livros?.map((l) => (
            <button key={l.id} className="cartao-album" onClick={() => onAbrir(l.id)} title={l.titulo}>
              <Capa id={l.capa_media} />
              <span className="nome">{l.titulo}</span>
              <span className="detalhe">
                {estadoLivro(l)}
                {l.autor ? ` · ${l.autor}` : ""}
              </span>
              {l.ouvido_ms > 0 && !l.concluido && (
                <span className="progresso-episodio" aria-hidden="true">
                  <span style={{ width: `${porcento(l)}%` }} />
                </span>
              )}
            </button>
          ))}
        </div>
      </div>
    </section>
  );
}

/** Tela do livro: continuar de onde parou e a lista de capítulos. */
export function LivroView({
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
  const [livro, setLivro] = useState<Livro | null>(null);
  const [capitulos, setCapitulos] = useState<Capitulo[]>([]);

  useEffect(() => {
    invoke<Livro | null>("audiobook_get", { id })
      .then(setLivro)
      .catch((e) => onErro(String(e)));
    invoke<Capitulo[]>("audiobook_chapters", { id })
      .then(setCapitulos)
      .catch((e) => onErro(String(e)));
  }, [id, versao]);

  const tocar = (capitulo: number | null) =>
    invoke("play_audiobook", { id, capitulo }).catch((e) => onErro(String(e)));
  const marcar = (concluido: boolean) => invoke("audiobook_mark", { id, concluido }).catch((e) => onErro(String(e)));

  if (!livro) return <section className="colecao" />;
  const comecado = livro.ouvido_ms > 0 && !livro.concluido;
  return (
    <section className="colecao">
      <button className="voltar" onClick={onVoltar}>
        ← Audiolivros
      </button>
      <div className="cabecalho-colecao">
        <Capa id={livro.capa_media} className="grande" />
        <div className="info-album">
          <h2>{livro.titulo}</h2>
          {livro.autor && <p className="detalhe">{livro.autor}</p>}
          <p className="detalhe">
            {plural(livro.capitulos, "capítulo", "capítulos")} · {horas(livro.duracao_ms)} · {estadoLivro(livro)}
          </p>
          {comecado && (
            <span className="progresso-episodio progresso-livro" aria-hidden="true">
              <span style={{ width: `${porcento(livro)}%` }} />
            </span>
          )}
          <div className="acoes-album">
            <button className="primario" disabled={capitulos.length === 0} onClick={() => tocar(null)}>
              ▶ {comecado ? "Continuar" : livro.concluido ? "Ouvir de novo" : "Começar"}
            </button>
            {livro.concluido || comecado ? (
              <button onClick={() => marcar(false)} title="Volta ao começo do livro">
                <RotateCcw size={14} aria-hidden="true" /> Recomeçar
              </button>
            ) : null}
            {!livro.concluido && (
              <button onClick={() => marcar(true)}>
                <Check size={14} aria-hidden="true" /> Marcar como terminado
              </button>
            )}
          </div>
        </div>
      </div>
      <div className="colecao-rolagem">
        <ol className="episodios">
          {capitulos.map((c, i) => {
            const atual = comecado && i === livro.capitulo_atual;
            return (
              <li key={c.media_id} className={(c.media_id === idAtual ? "atual " : "") + (c.missing ? "ausente" : "")}>
                <button
                  className="tocar-episodio"
                  onClick={() => tocar(c.media_id)}
                  disabled={c.missing}
                  aria-label={`Tocar ${c.titulo}`}
                  title={c.missing ? "Arquivo não encontrado" : atual ? "Continuar deste capítulo" : "Tocar do começo do capítulo"}
                >
                  <Play size={14} fill="currentColor" />
                </button>
                <div className="texto-episodio" {...tocarAo(() => !c.missing && tocar(c.media_id))}>
                  <span className="titulo">{c.titulo}</span>
                  <span className="detalhe">
                    {[formatarDuracao(c.duracao_ms), atual ? "Parou aqui" : ""].filter(Boolean).join(" · ")}
                  </span>
                </div>
              </li>
            );
          })}
        </ol>
      </div>
    </section>
  );
}
