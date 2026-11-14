import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useVirtualizer } from "@tanstack/react-virtual";

/**
 * Completar informações em lote (seção 6): lista músicas com campos vazios, busca uma
 * sugestão por música no MusicBrainz (1 por segundo, como o serviço pede) e só preenche
 * os campos vazios que o usuário deixar marcados. Campos preenchidos nunca mudam aqui.
 */

type Campo = "artist" | "album" | "year" | "genre";
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
};
type Sugestao = { valores: Partial<Record<Campo, string | number>>; marcados: Set<Campo> } | "nada" | "erro";

const NOMES: Record<Campo, string> = { artist: "Artista", album: "Álbum", year: "Ano", genre: "Gênero" };
const INTERVALO_MS = 1100;
const ALTURA = 64;

const espera = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Melhor candidato confiável: semelhança ≥ 90 e duração a até 5 s (quando conhecida). */
function escolher(c: Candidato[], duracao: number | null) {
  return c.find(
    (x) => x.pontuacao >= 90 && (duracao === null || x.duracao_ms === null || Math.abs(x.duracao_ms - duracao) <= 5000),
  );
}

export default function Completar({
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

  async function buscar() {
    parar.current = false;
    const fila = lista.filter((i) => !sugestoes.has(i.id) && i.title);
    for (let n = 0; n < fila.length && !parar.current; n++) {
      const i = fila[n];
      setProgresso({ feitos: n, total: fila.length, acao: "Buscando" });
      try {
        const c = await invoke<Candidato[]>("metadata_lookup", {
          titulo: i.title,
          artista: i.artist,
          duracaoMs: i.duration_ms,
        });
        const melhor = escolher(c, i.duration_ms);
        const valores: Partial<Record<Campo, string | number>> = {};
        for (const f of i.faltando) {
          const v = melhor?.campos[f];
          if (v !== null && v !== undefined && v !== "") valores[f] = v;
        }
        const campos = Object.keys(valores) as Campo[];
        definir(i.id, campos.length ? { valores, marcados: new Set(campos) } : "nada");
      } catch (e) {
        definir(i.id, "erro");
        // Sem internet: não adianta seguir tentando.
        if (String(e).includes("Sem conexão")) {
          onErro(String(e));
          break;
        }
      }
      if (n < fila.length - 1) await espera(INTERVALO_MS);
    }
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
    <section className="colecao completar">
      <div className="cabecalho-colecao">
        <h2>Completar informações</h2>
      </div>
      <p className="nota">
        {itens.length} {itens.length === 1 ? "música" : "músicas"} sem artista, álbum, ano ou gênero. A busca usa o
        MusicBrainz (precisa de internet, uma música por segundo) e só sugere quando a semelhança é alta e a duração
        bate. Só os campos vazios são preenchidos, e só os que ficarem marcados.
      </p>
      <div className="barra-completar">
        <label>
          Faltando:{" "}
          <select value={filtro} onChange={(e) => setFiltro(e.target.value as Campo | "")} disabled={!!progresso}>
            <option value="">qualquer campo</option>
            {(Object.keys(NOMES) as Campo[]).map((c) => (
              <option key={c} value={c}>
                {NOMES[c].toLowerCase()}
              </option>
            ))}
          </select>
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
              Buscar sugestões ({lista.length - buscadas})
            </button>
            <label>
              <input type="checkbox" checked={noArquivo} onChange={(e) => setNoArquivo(e.target.checked)} /> gravar
              também nos arquivos
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
    </section>
  );
}
