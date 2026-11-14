import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Linha = { ms: number; texto: string };
type Letra =
  | { tipo: "sincronizada"; linhas: Linha[]; origem: string }
  | { tipo: "estatica"; texto: string; origem: string };

/** Índice da última linha que já começou (-1 antes da primeira). */
function linhaAtual(linhas: Linha[], posicao: number) {
  let ini = 0;
  let fim = linhas.length - 1;
  let achada = -1;
  while (ini <= fim) {
    const meio = (ini + fim) >> 1;
    if (linhas[meio].ms <= posicao) {
      achada = meio;
      ini = meio + 1;
    } else {
      fim = meio - 1;
    }
  }
  return achada;
}

type Encontrada = { texto: string; sincronizada: boolean; instrumental: boolean; titulo: string; artista: string };

/** Primeiras linhas da letra encontrada, sem as marcas de tempo. */
function previa(texto: string) {
  return texto
    .split("\n")
    .map((l) => l.replace(/\[[^\]]*\]/g, "").trim())
    .filter(Boolean)
    .slice(0, 6);
}

/** Busca no LRCLIB (só ao clicar) e deixa escolher onde guardar. */
function BuscarLetra({ id, onSalva }: { id: number; onSalva: () => void }) {
  const [estado, setEstado] = useState<
    { tipo: "parado" } | { tipo: "buscando" } | { tipo: "nada" } | { tipo: "erro"; msg: string } | { tipo: "achou"; e: Encontrada }
  >({ tipo: "parado" });

  useEffect(() => setEstado({ tipo: "parado" }), [id]);

  async function buscar() {
    setEstado({ tipo: "buscando" });
    try {
      const e = await invoke<Encontrada | null>("lyrics_lookup", { id });
      setEstado(e ? { tipo: "achou", e } : { tipo: "nada" });
    } catch (err) {
      setEstado({ tipo: "erro", msg: String(err) });
    }
  }

  async function salvar(noArquivo: boolean) {
    if (estado.tipo !== "achou") return;
    try {
      await invoke("lyrics_save", { id, texto: estado.e.texto, noArquivo });
      setEstado({ tipo: "parado" });
      onSalva();
    } catch (err) {
      setEstado({ tipo: "erro", msg: String(err) });
    }
  }

  if (estado.tipo === "achou") {
    const { e } = estado;
    if (e.instrumental)
      return (
        <div className="buscar-letra">
          <p>O LRCLIB marca esta música como instrumental (sem letra).</p>
          <button onClick={() => setEstado({ tipo: "parado" })}>Fechar</button>
        </div>
      );
    return (
      <div className="buscar-letra">
        <p>
          <strong>Encontrada</strong> ({e.sincronizada ? "sincronizada" : "sem tempos"}): {e.titulo} · {e.artista}
        </p>
        <blockquote>
          {previa(e.texto).map((l, i) => (
            <span key={i}>{l}</span>
          ))}
        </blockquote>
        <div className="acoes-letra">
          <button className="primario" onClick={() => salvar(false)} title="Guarda na pasta de dados do Skáld; o arquivo de música não muda">
            Guardar no Skáld
          </button>
          <button
            onClick={() => salvar(true)}
            disabled={!e.sincronizada}
            title={
              e.sincronizada
                ? "Cria um arquivo .lrc com o mesmo nome da música (nunca substitui um existente)"
                : "Só letras sincronizadas viram arquivo .lrc"
            }
          >
            Criar .lrc ao lado do arquivo
          </button>
          <button onClick={() => setEstado({ tipo: "parado" })}>Descartar</button>
        </div>
      </div>
    );
  }

  return (
    <div className="buscar-letra">
      <button onClick={buscar} disabled={estado.tipo === "buscando"}>
        {estado.tipo === "buscando" ? "Buscando…" : "Buscar letra na internet (LRCLIB)"}
      </button>
      {estado.tipo === "nada" && <p>Nenhuma letra encontrada para esta música.</p>}
      {estado.tipo === "erro" && <p className="erro">{estado.msg}</p>}
      <p className="nota">Envia só título, artista, álbum e duração ao lrclib.net.</p>
    </div>
  );
}

type Props = {
  id: number | null;
  posicaoMs: number;
  onErro: (m: string) => void;
};

/** Letra da música atual: sincronizada (linha atual em destaque) ou estática. */
export default function Letra({ id, posicaoMs, onErro }: Props) {
  const [letra, setLetra] = useState<Letra | null | undefined>(undefined);
  const [recarga, setRecarga] = useState(0);
  const caixa = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (id === null) return setLetra(null);
    let valido = true;
    if (recarga === 0) setLetra(undefined);
    invoke<Letra | null>("get_lyrics", { id })
      .then((l) => valido && setLetra(l))
      .catch((e) => valido && onErro(String(e)));
    return () => {
      valido = false;
    };
  }, [id, recarga]);
  useEffect(() => setRecarga(0), [id]);

  const atual = letra?.tipo === "sincronizada" ? linhaAtual(letra.linhas, posicaoMs) : -1;

  // Mantém a linha atual no meio da caixa (sem rolar a página inteira).
  useEffect(() => {
    const c = caixa.current;
    const el = c?.querySelector<HTMLElement>(`[data-linha="${atual}"]`);
    if (!c || !el) return;
    c.scrollTo({ top: el.offsetTop - c.clientHeight / 2 + el.clientHeight / 2, behavior: "smooth" });
  }, [atual]);

  if (letra === undefined) return <p className="letra-vazia">Carregando a letra…</p>;
  const recarregar = () => setRecarga((n) => n + 1);
  if (letra === null)
    return (
      <div>
        <p className="letra-vazia">Sem letra para esta música.</p>
        {id !== null && <BuscarLetra id={id} onSalva={recarregar} />}
      </div>
    );

  return (
    <div className="letra" ref={caixa}>
      {letra.tipo === "sincronizada" ? (
        letra.linhas.map((l, i) => (
          <button
            key={i}
            data-linha={i}
            className={"linha-letra" + (i === atual ? " atual" : i < atual ? " passada" : "")}
            onClick={() => invoke("seek", { posicaoMs: l.ms }).catch((e) => onErro(String(e)))}
            title="Ir para este trecho"
          >
            {l.texto || "♪"}
          </button>
        ))
      ) : (
        <p className="letra-estatica">{letra.texto}</p>
      )}
      <p className="letra-origem">Fonte: {letra.origem}</p>
      {letra.tipo === "estatica" && id !== null && (
        <BuscarLetra id={id} onSalva={recarregar} />
      )}
    </div>
  );
}
