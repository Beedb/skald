import { useEffect, useRef, useState } from "react";
import { emSegundoPlano } from "./tarefaLonga";
import { invoke } from "@tauri-apps/api/core";
import { useVirtualizer } from "@tanstack/react-virtual";
import CompletarLetras from "./CompletarLetras";
import { CompletarCategorias } from "./CategoriasMusica";

/**
 * Completar informações em lote (seção 6): lista músicas com campos vazios, busca uma
 * sugestão por música no MusicBrainz (1 por segundo, como o serviço pede) e só preenche
 * os campos vazios que o usuário deixar marcados. Campos preenchidos nunca mudam aqui.
 */

type Campo = "title" | "artist" | "album" | "year" | "genre";
type Incompleta = {
  id: number;
  title: string | null;
  artist: string | null;
  album: string | null;
  duration_ms: number | null;
  faltando: Campo[];
};
type Candidato = {
  campos: Record<string, string | number | null>;
  pontuacao: number;
  duracao_ms: number | null;
  incerto: boolean;
};
type Sugestao =
  | { valores: Partial<Record<Campo, string | number>>; marcados: Set<Campo>; incerto: boolean }
  | "nada"
  | "erro";

const NOMES: Record<Campo, string> = { title: "Título", artist: "Artista", album: "Álbum", year: "Ano", genre: "Gênero" };
/** Campos que podem faltar (o título nunca falta; ele só pode vir sujo do nome do arquivo). */
const FALTAVEIS: Campo[] = ["artist", "album", "year", "genre"];
const ALTURA = 64;
/** Músicas por consulta ao MusicBrainz (enriquecer::LOTE). */
const LOTE = 8;

const normal = (t: string) =>
  t
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();

/** O título atual parece nome de arquivo com sobras ("02 - Música", "Artista - Música") em volta do sugerido. */
function tituloSujo(atual: string | null, sugerido: unknown) {
  if (!atual || typeof sugerido !== "string" || !sugerido.trim()) return false;
  const a = normal(atual);
  const s = normal(sugerido);
  return a !== s && s.length > 0 && a.includes(s);
}

/** Melhor candidato confiável: semelhança ≥ 90 e duração a até 5 s (quando conhecida). */
function escolher(c: Candidato[], duracao: number | null) {
  return c.find(
    (x) => x.pontuacao >= 90 && (duracao === null || x.duracao_ms === null || Math.abs(x.duracao_ms - duracao) <= 5000),
  );
}

type Aba = "info" | "letras" | "categorias";

/** Três abas: informações (MusicBrainz), letras (LRCLIB) e categorias (MusicBrainz + década). */
export default function Completar({
  aba: abaInicial,
  ...props
}: {
  aba?: Aba;
  versao: number;
  onMudou: (mensagem: string) => void;
  onErro: (m: string) => void;
}) {
  const [aba, setAba] = useState<Aba>(abaInicial ?? "info");
  useEffect(() => {
    if (abaInicial) setAba(abaInicial);
  }, [abaInicial]);
  return (
    <section className="colecao completar">
      <div className="cabecalho-colecao">
        <h2>Completar informações</h2>
        <div className="abas abas-compactas" role="tablist">
          <button role="tab" aria-selected={aba === "info"} className={aba === "info" ? "ativa" : ""} onClick={() => setAba("info")}>
            Informações
          </button>
          <button role="tab" aria-selected={aba === "letras"} className={aba === "letras" ? "ativa" : ""} onClick={() => setAba("letras")}>
            Letras
          </button>
          <button
            role="tab"
            aria-selected={aba === "categorias"}
            className={aba === "categorias" ? "ativa" : ""}
            onClick={() => setAba("categorias")}
          >
            Categorias
          </button>
        </div>
      </div>
      {aba === "info" && <CompletarInfo {...props} />}
      {aba === "letras" && <CompletarLetras onMudou={props.onMudou} onErro={props.onErro} />}
      {aba === "categorias" && <CompletarCategorias onMudou={props.onMudou} onErro={props.onErro} />}
    </section>
  );
}

function CompletarInfo({
  versao,
  onMudou,
  onErro,
}: {
  versao: number;
  onMudou: (mensagem: string) => void;
  onErro: (m: string) => void;
}) {
  const [itens, setItens] = useState<Incompleta[] | null>(null);
  const [filtro, setFiltro] = useState<Campo | "">("");
  const [sugestoes, setSugestoes] = useState<Map<number, Sugestao>>(new Map());
  const [progresso, setProgresso] = useState<{ feitos: number; total: number; acao: string } | null>(null);
  const [noArquivo, setNoArquivo] = useState(false);
  const [renomear, setRenomear] = useState(false);
  // Reconhecer pelo som (AcoustID) quando o nome não basta; só existe com a chave embutida.
  const [temSom, setTemSom] = useState(false);
  const [pelSom, setPeloSom] = useState(true);
  useEffect(() => {
    invoke<boolean>("acoustic_available").then(setTemSom).catch(() => {});
  }, []);
  const parar = useRef(false);
  const rolagem = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<Incompleta[]>("incomplete_list")
      .then(setItens)
      .catch((e) => onErro(String(e)));
  }, [versao]);

  // Sair da tela interrompe a busca.
  useEffect(() => () => void (parar.current = true), []);

  const lista = (itens ?? []).filter((i) => !filtro || i.faltando.includes(filtro));
  const virt = useVirtualizer({
    count: lista.length,
    getScrollElement: () => rolagem.current,
    estimateSize: () => ALTURA,
    overscan: 8,
  });

  function definir(id: number, s: Sugestao) {
    setSugestoes((m) => new Map(m).set(id, s));
  }

  /** Melhor candidato pelo nome: o do lote ou, se o lote achou algo sem confiança (ou falhou),
   *  a busca sozinha, que tenta as formas limpas do nome. Se o lote não achou nada, a música
   *  não está no MusicBrainz por nome (o lote já tentou "Artista - Título" nos dois sentidos). */
  async function porTexto(i: Incompleta, doLote: Candidato[] | null, semTexto: boolean) {
    let melhor = escolher(doLote ?? [], i.duration_ms);
    if ((!melhor || melhor.incerto) && !semTexto) {
      const c = await invoke<Candidato[]>("metadata_lookup", { titulo: i.title, artista: i.artist, duracaoMs: i.duration_ms });
      const sozinha = escolher(c, i.duration_ms);
      if (sozinha && (!melhor || !sozinha.incerto)) melhor = sozinha;
    }
    return melhor;
  }

  /** Nome não bastou (ou só achou pelo título): tenta pelo som. */
  async function porSom(i: Incompleta, melhor: Candidato | null) {
    try {
      const s = await invoke<Candidato[]>("metadata_recognize", { id: i.id });
      const pelo = escolher(s, i.duration_ms);
      if (pelo && (!melhor || !pelo.incerto)) return pelo;
    } catch {
      // Sem resultado pelo som: fica a busca por texto.
    }
    return melhor;
  }

  function sugerir(i: Incompleta, melhor: Candidato | null) {
    const valores: Partial<Record<Campo, string | number>> = {};
    for (const f of i.faltando) {
      const v = melhor?.campos[f];
      if (v !== null && v !== undefined && v !== "") valores[f] = v;
    }
    // Título vindo do nome do arquivo ("02 - Música"): sugere o título limpo.
    if (melhor && tituloSujo(i.title, melhor.campos.title)) valores.title = melhor.campos.title as string;
    const campos = Object.keys(valores) as Campo[];
    // Achado só pelo título: a pessoa confere antes (nada marcado).
    const marcados = new Set(melhor?.incerto ? [] : campos);
    definir(i.id, campos.length ? { valores, marcados, incerto: !!melhor?.incerto } : "nada");
  }

  async function buscar() {
    parar.current = false;
    const fila = lista.filter((i) => !sugestoes.has(i.id) && i.title);
    const total = fila.length;
    // Músicas que ficaram sem espaço num lote cheio voltam uma vez para o lote seguinte.
    const devolvidas = new Set<number>();
    let feitos = 0;
    const contar = (avisar: (t: string) => void) => {
      feitos++;
      setProgresso({ feitos, total, acao: "Buscando" });
      avisar(`Completando informações: ${feitos} de ${total}`);
    };
    await emSegundoPlano("Completando informações…", async (avisar) => {
      while (fila.length > 0 && !parar.current) {
        // Várias músicas por consulta: o MusicBrainz aceita uma consulta por segundo.
        const lote = fila.splice(0, LOTE);
        setProgresso({ feitos, total, acao: "Buscando" });
        let r: { candidatos: Candidato[][]; cheio: boolean } | null = null;
        try {
          r = await invoke("metadata_lookup_batch", {
            itens: lote.map((i) => ({ titulo: i.title, artista: i.artist, duracao_ms: i.duration_ms })),
          });
        } catch (e) {
          if (String(e).includes("Sem conexão")) {
            lote.forEach((i) => definir(i.id, "erro"));
            onErro(String(e));
            break;
          }
          // Outro erro no lote: cada música é procurada sozinha.
        }
        // 1º pelo nome; quem não for achado vai para o som.
        const peloSom: { i: Incompleta; melhor: Candidato | null }[] = [];
        for (let k = 0; k < lote.length && !parar.current; k++) {
          const i = lote[k];
          const c = r?.candidatos[k] ?? null;
          if (r && c && c.length === 0 && r.cheio && !devolvidas.has(i.id)) {
            devolvidas.add(i.id);
            fila.unshift(i);
            continue;
          }
          try {
            const melhor = (await porTexto(i, c, !!r && !!c && c.length === 0)) ?? null;
            if ((!melhor || melhor.incerto) && temSom && pelSom) {
              peloSom.push({ i, melhor });
              continue;
            }
            sugerir(i, melhor);
          } catch (e) {
            definir(i.id, "erro");
            // Sem internet: não adianta seguir tentando.
            if (String(e).includes("Sem conexão")) {
              onErro(String(e));
              parar.current = true;
            }
          }
          contar(avisar);
        }
        // Pelo som, 3 ao mesmo tempo (a impressão digital é calculada em paralelo).
        const proximo = { n: 0 };
        const trabalhar = async () => {
          while (proximo.n < peloSom.length && !parar.current) {
            const { i, melhor } = peloSom[proximo.n++];
            setProgresso({ feitos, total, acao: "Ouvindo" });
            sugerir(i, await porSom(i, melhor));
            contar(avisar);
          }
        };
        await Promise.all([trabalhar(), trabalhar(), trabalhar()]);
      }
    });
    setProgresso(null);
  }

  function alternar(id: number, campo: Campo) {
    const s = sugestoes.get(id);
    if (!s || typeof s === "string") return;
    const marcados = new Set(s.marcados);
    if (marcados.has(campo)) marcados.delete(campo);
    else marcados.add(campo);
    definir(id, { ...s, marcados });
  }

  const prontas = lista.filter((i) => {
    const s = sugestoes.get(i.id);
    return s && typeof s !== "string" && s.marcados.size > 0;
  });

  async function aplicar() {
    parar.current = false;
    let ok = 0;
    const falhas: string[] = [];
    for (let n = 0; n < prontas.length && !parar.current; n++) {
      const i = prontas[n];
      const s = sugestoes.get(i.id) as Exclude<Sugestao, string>;
      setProgresso({ feitos: n, total: prontas.length, acao: "Gravando" });
      const campos = Object.fromEntries([...s.marcados].map((c) => [c, s.valores[c]]));
      try {
        await invoke("metadata_apply", { id: i.id, campos, noArquivo, releaseCapa: null });
        ok++;
        if (renomear) {
          try {
            await invoke("media_rename", { id: i.id });
          } catch (e) {
            falhas.push(`${i.title ?? i.id} (renomear): ${e}`);
          }
        }
      } catch (e) {
        falhas.push(`${i.title ?? i.id}: ${e}`);
      }
    }
    setProgresso(null);
    setSugestoes(new Map());
    if (falhas.length) onErro(`${falhas.length} com problema. ${falhas.slice(0, 3).join(" · ")}`);
    onMudou(`${ok} ${ok === 1 ? "música completada" : "músicas completadas"}.`);
  }

  if (!itens) return null;
  const buscadas = lista.filter((i) => sugestoes.has(i.id)).length;

  return (
    <>
      <p className="nota">
        {itens.length} {itens.length === 1 ? "música" : "músicas"} sem artista, álbum, ano ou gênero. A busca usa o
        MusicBrainz (precisa de internet; procura 8 músicas por vez) e só sugere quando a semelhança é alta e a duração
        bate. Nomes de arquivo como "02 - Artista - Música" são separados antes da busca. Só os campos vazios são
        preenchidos (e o título, quando vem do nome do arquivo), e só os que ficarem marcados.
      </p>
      <div className="barra-completar">
        <label>
          Faltando:{" "}
          <select value={filtro} onChange={(e) => setFiltro(e.target.value as Campo | "")} disabled={!!progresso}>
            <option value="">qualquer campo</option>
            {FALTAVEIS.map((c) => (
              <option key={c} value={c}>
                {NOMES[c].toLowerCase()}
              </option>
            ))}
          </select>
        </label>
        {progresso ? (
          <>
            <span className="detalhe">
              {progresso.acao} {Math.min(progresso.feitos + 1, progresso.total)} de {progresso.total}…
            </span>
            <button onClick={() => (parar.current = true)}>Parar</button>
          </>
        ) : (
          <>
            <button className="primario" onClick={buscar} disabled={buscadas === lista.length}>
              Buscar sugestões ({lista.length - buscadas})
            </button>
            <label>
              <input type="checkbox" checked={noArquivo} onChange={(e) => setNoArquivo(e.target.checked)} /> gravar
              também nos arquivos
            </label>
            {temSom && (
              <label title="Quando o nome do arquivo não basta, reconhece a música pelo som (AcoustID). Mais lento: alguns segundos por música.">
                <input type="checkbox" checked={pelSom} onChange={(e) => setPeloSom(e.target.checked)} /> reconhecer
                pelo som
              </label>
            )}
            <label title="Renomeia para “Artista - Título” na mesma pasta. Nunca substitui outro arquivo; playlists e histórico continuam iguais.">
              <input type="checkbox" checked={renomear} onChange={(e) => setRenomear(e.target.checked)} /> renomear os
              arquivos (Artista - Título)
            </label>
            <button className="primario" onClick={aplicar} disabled={prontas.length === 0}>
              Aplicar ({prontas.length})
            </button>
          </>
        )}
      </div>
      {lista.length === 0 && <p className="vazio">Nada faltando aqui.</p>}
      <div className="colecao-rolagem" ref={rolagem}>
        <div style={{ height: virt.getTotalSize(), position: "relative" }}>
          {virt.getVirtualItems().map((v) => {
            const i = lista[v.index];
            const s = sugestoes.get(i.id);
            return (
              <div key={i.id} className="linha-completar" style={{ height: ALTURA, transform: `translateY(${v.start}px)` }}>
                <div className="quem">
                  <strong>{i.title ?? "Sem título"}</strong>
                  <span className="detalhe">
                    {i.artist ?? "Artista desconhecido"}
                    {i.album ? ` · ${i.album}` : ""} · falta {i.faltando.map((f) => NOMES[f].toLowerCase()).join(", ")}
                  </span>
                </div>
                <div className="sugestao">
                  {s === undefined && <span className="detalhe">—</span>}
                  {s === "nada" && <span className="detalhe">sem sugestão confiável</span>}
                  {s === "erro" && <span className="detalhe">erro na busca</span>}
                  {s && typeof s !== "string" && s.incerto && (
                    <span className="detalhe" title="Achada só pelo título, sem o artista. Confira antes de marcar.">
                      confira:
                    </span>
                  )}
                  {s &&
                    typeof s !== "string" &&
                    (Object.keys(s.valores) as Campo[]).map((c) => (
                      <label key={c} className={"chip" + (s.marcados.has(c) ? " marcado" : "")}>
                        <input type="checkbox" checked={s.marcados.has(c)} onChange={() => alternar(i.id, c)} />
                        {NOMES[c]}: {String(s.valores[c])}
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
