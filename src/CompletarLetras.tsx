import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { MediaRow } from "./Biblioteca";

/**
 * Letras em lote: lista as músicas sem letra em lugar nenhum e busca no LRCLIB uma por vez.
 * Só preenche o que está vazio; o destino (só Skáld ou .lrc ao lado) é escolhido antes.
 */

type Encontrada = { texto: string; sincronizada: boolean; instrumental: boolean };
type Resultado = "sincronizada" | "simples" | "instrumental" | "nada" | "pulada" | { erro: string };

const INTERVALO_MS = 300;
const ALTURA = 56;
const espera = (ms: number) => new Promise((r) => setTimeout(r, ms));

function rotulo(r: Resultado | undefined) {
  if (r === undefined) return "—";
  if (typeof r !== "string") return `erro: ${r.erro}`;
  return {
    sincronizada: "✓ letra sincronizada",
    simples: "✓ letra sem tempos",
    instrumental: "instrumental",
    nada: "não encontrada",
    pulada: "só sem tempos (pulada)",
  }[r];
}

export default function CompletarLetras({ onMudou, onErro }: { onMudou: (m: string) => void; onErro: (m: string) => void }) {
  const [itens, setItens] = useState<MediaRow[] | null>(null);
  const [resultados, setResultados] = useState<Map<number, Resultado>>(new Map());
  const [progresso, setProgresso] = useState<{ feitos: number; total: number } | null>(null);
  const [noArquivo, setNoArquivo] = useState(false);
  const [soSincronizadas, setSoSincronizadas] = useState(false);
  const parar = useRef(false);
  const rolagem = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<MediaRow[]>("lyrics_missing")
      .then(setItens)
      .catch((e) => onErro(String(e)));
  }, []);

  // Sair da tela interrompe a busca.
  useEffect(() => () => void (parar.current = true), []);

  const lista = itens ?? [];
  const virt = useVirtualizer({
    count: lista.length,
    getScrollElement: () => rolagem.current,
    estimateSize: () => ALTURA,
    overscan: 8,
  });

  const definir = (id: number, r: Resultado) => setResultados((m) => new Map(m).set(id, r));

  async function buscar() {
    parar.current = false;
    const fila = lista.filter((i) => !resultados.has(i.id));
    let salvas = 0;
    for (let n = 0; n < fila.length && !parar.current; n++) {
      const i = fila[n];
      setProgresso({ feitos: n, total: fila.length });
      try {
        const e = await invoke<Encontrada | null>("lyrics_lookup", { id: i.id });
        if (!e) definir(i.id, "nada");
        else if (e.instrumental) definir(i.id, "instrumental");
        else if (!e.sincronizada && soSincronizadas) definir(i.id, "pulada");
        else {
          // .lrc ao lado só aceita letra sincronizada; a sem tempos fica no Skáld.
          await invoke("lyrics_save", { id: i.id, texto: e.texto, noArquivo: noArquivo && e.sincronizada });
          definir(i.id, e.sincronizada ? "sincronizada" : "simples");
          salvas++;
        }
      } catch (err) {
        definir(i.id, { erro: String(err) });
        // Sem internet: não adianta seguir tentando.
        if (String(err).includes("Sem conexão")) {
          onErro(String(err));
          break;
        }
      }
      if (n < fila.length - 1) await espera(INTERVALO_MS);
    }
    setProgresso(null);
    if (salvas) onMudou(`${salvas} ${salvas === 1 ? "letra guardada" : "letras guardadas"}.`);
  }

  if (!itens) return <p className="detalhe">Verificando quais músicas não têm letra…</p>;
  const pendentes = lista.filter((i) => !resultados.has(i.id)).length;

  return (
    <>
      <p className="nota">
        {lista.length} {lista.length === 1 ? "música" : "músicas"} sem letra (nem .lrc, nem tag, nem guardada no
        Skáld). A busca usa o LRCLIB (precisa de internet) e envia só título, artista, álbum e duração. Nenhuma letra
        existente é substituída.
      </p>
      <div className="barra-completar">
        {progresso ? (
          <>
            <span className="detalhe">
              Buscando {progresso.feitos + 1} de {progresso.total}…
            </span>
            <button onClick={() => (parar.current = true)}>Parar</button>
          </>
        ) : (
          <>
            <button className="primario" onClick={buscar} disabled={pendentes === 0}>
              Buscar letras ({pendentes})
            </button>
            <label>
              Guardar:{" "}
              <select value={noArquivo ? "arquivo" : "skald"} onChange={(e) => setNoArquivo(e.target.value === "arquivo")}>
                <option value="skald">só no Skáld</option>
                <option value="arquivo">.lrc ao lado da música</option>
              </select>
            </label>
            <label title="Ignora letras sem marcação de tempo">
              <input type="checkbox" checked={soSincronizadas} onChange={(e) => setSoSincronizadas(e.target.checked)} /> só
              sincronizadas
            </label>
          </>
        )}
      </div>
      {noArquivo && (
        <p className="nota">
          Cria um arquivo .lrc novo com o mesmo nome da música; letras sem tempos ficam só no Skáld.
        </p>
      )}
      {lista.length === 0 && <p className="vazio">Todas as músicas já têm letra.</p>}
      <div className="colecao-rolagem" ref={rolagem}>
        <div style={{ height: virt.getTotalSize(), position: "relative" }}>
          {virt.getVirtualItems().map((v) => {
            const i = lista[v.index];
            const r = resultados.get(i.id);
            return (
              <div key={i.id} className="linha-completar" style={{ height: ALTURA, transform: `translateY(${v.start}px)` }}>
                <div className="quem">
                  <strong>{i.title ?? "Sem título"}</strong>
                  <span className="detalhe">
                    {i.artist ?? "Artista desconhecido"}
                    {i.album ? ` · ${i.album}` : ""}
                  </span>
                </div>
                <div className="sugestao">
                  <span className={"detalhe" + (r === "sincronizada" || r === "simples" ? " ok" : "")}>{rotulo(r)}</span>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </>
  );
}
