import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatarTempoLongo, nomeMes, numero, type Item } from "./Estatisticas";
import { gerarPng, type DadosRetro } from "./retroCanvas";

type Retrospectiva = {
  periodo: string;
  tempo_ms: number;
  escutadas: number;
  artistas: Item[];
  musicas: Item[];
  albuns: Item[];
  genero: Item | null;
  mais_repetida: { titulo: string; artista: string | null; vezes: number; dia: string } | null;
  dominante: { nome: string; percentual: number } | null;
};

/** "2026-10" → "outubro de 2026"; "2026" → "2026". */
function nomePeriodo(p: string) {
  const [ano, mes] = p.split("-");
  return mes ? `${nomeMes(ano, mes, "long")} de ${ano}` : ano;
}

function dataCurta(dia: string) {
  const [a, m, d] = dia.split("-");
  return new Date(Number(a), Number(m) - 1, Number(d)).toLocaleDateString("pt-BR", { day: "numeric", month: "long" });
}

function Miniatura({ src }: { src: string | null | undefined }) {
  return src ? <img className="rt-capa" src={src} alt="" /> : <span className="rt-capa rt-sem-capa">♪</span>;
}

function Lista({ titulo, itens, capas }: { titulo: string; itens: Item[]; capas?: boolean }) {
  return (
    <div className="rt-bloco">
      <h3>{titulo}</h3>
      <ol>
        {itens.map((it, i) => (
          <li key={i}>
            <span className="rt-pos">{i + 1}</span>
            {capas && <Miniatura src={it.capa} />}
            <span className="rt-nome">
              {it.nome}
              {it.detalhe && <small>{it.detalhe}</small>}
            </span>
            <span className="rt-vezes">{numero(it.escutadas)}×</span>
          </li>
        ))}
      </ol>
    </div>
  );
}

/** Composição única da retrospectiva (seção 11.7), sobre fundo escuro. */
/** Textos da composição, usados na tela e no PNG. */
function dados(r: Retrospectiva): DadosRetro {
  const dominanteCapa = r.artistas.find((a) => a.nome === r.dominante?.nome)?.capa ?? null;
  return {
    titulo: nomePeriodo(r.periodo).replace(/^./, (c) => c.toUpperCase()),
    tempo: formatarTempoLongo(r.tempo_ms),
    escutadas: numero(r.escutadas),
    dominante: r.dominante && {
      nome: r.dominante.nome,
      detalhe: `${Math.round(r.dominante.percentual)}% do tempo ouvido`,
      capa: dominanteCapa,
    },
    repetida: r.mais_repetida && {
      titulo: r.mais_repetida.titulo,
      detalhe: `${r.mais_repetida.artista ? `${r.mais_repetida.artista} · ` : ""}${r.mais_repetida.vezes}× em ${dataCurta(r.mais_repetida.dia)}`,
    },
    genero: r.genero && { nome: r.genero.nome, detalhe: `${numero(r.genero.escutadas)} escutadas` },
    artistas: r.artistas,
    musicas: r.musicas,
    albuns: r.albuns,
  };
}

function Composicao({ r, onFechar, onErro }: { r: Retrospectiva; onFechar: () => void; onErro: (m: string) => void }) {
  const d = dados(r);
  const [exportando, setExportando] = useState(false);
  const [salvo, setSalvo] = useState<string | null>(null);

  async function exportar() {
    setExportando(true);
    setSalvo(null);
    try {
      const png = await gerarPng(d);
      setSalvo(await invoke<string>("retrospective_save_png", { periodo: r.periodo, pngBase64: png }));
    } catch (e) {
      onErro(String(e));
    } finally {
      setExportando(false);
    }
  }

  useEffect(() => {
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && onFechar();
    window.addEventListener("keydown", tecla);
    return () => window.removeEventListener("keydown", tecla);
  }, [onFechar]);

  return (
    <div className="rt-fundo" role="dialog" aria-modal="true" aria-label={`Retrospectiva de ${nomePeriodo(r.periodo)}`}>
      <div className="rt-acoes">
        {salvo && <span className="rt-salvo">Salvo em {salvo}</span>}
        <button onClick={exportar} disabled={exportando}>
          {exportando ? "Exportando…" : "Exportar PNG"}
        </button>
        <button className="rt-fechar" onClick={onFechar} aria-label="Fechar" title="Fechar (Esc)">
          ✕
        </button>
      </div>
      <article className="rt-composicao">
        <header>
          <span className="rt-marca">Skáld · Retrospectiva</span>
          <h2>{nomePeriodo(r.periodo)}</h2>
        </header>

        <div className="rt-destaques">
          <div className="rt-bloco rt-grande">
            <strong>{formatarTempoLongo(r.tempo_ms)}</strong>
            <span>de música · {numero(r.escutadas)} escutadas</span>
          </div>
          {d.dominante && (
            <div className="rt-bloco rt-com-capa">
              <Miniatura src={d.dominante.capa} />
              <div>
                <h3>Artista dominante</h3>
                <strong>{d.dominante.nome}</strong>
                <span>{d.dominante.detalhe}</span>
              </div>
            </div>
          )}
          {r.mais_repetida && (
            <div className="rt-bloco">
              <h3>Música mais repetida</h3>
              <strong>{r.mais_repetida.titulo}</strong>
              <span>
                {r.mais_repetida.artista && `${r.mais_repetida.artista} · `}
                {r.mais_repetida.vezes}× em {dataCurta(r.mais_repetida.dia)}
              </span>
            </div>
          )}
          {r.genero && (
            <div className="rt-bloco">
              <h3>Gênero predominante</h3>
              <strong>{r.genero.nome}</strong>
              <span>{numero(r.genero.escutadas)} escutadas</span>
            </div>
          )}
        </div>

        <div className="rt-listas">
          <Lista titulo="Top 5 artistas" itens={r.artistas} capas />
          <Lista titulo="Top 10 músicas" itens={r.musicas} />
          <Lista titulo="Top 5 álbuns" itens={r.albuns} capas />
        </div>
      </article>
    </div>
  );
}

/** Retrospectivas mensais e anuais já liberadas (seções 11.5 e 11.6). */
export default function Retrospectivas({ versao, onErro }: { versao: number; onErro: (m: string) => void }) {
  const [lista, setLista] = useState<string[]>([]);
  const [aberta, setAberta] = useState<Retrospectiva | null>(null);

  useEffect(() => {
    invoke<string[]>("retrospective_list")
      .then(setLista)
      .catch((e) => onErro(String(e)));
  }, [versao]);

  function abrir(periodo: string) {
    invoke<Retrospectiva>("retrospective_get", { periodo })
      .then(setAberta)
      .catch((e) => onErro(String(e)));
  }

  return (
    <section className="retrospectivas">
      <h2>Retrospectivas</h2>
      {lista.length === 0 ? (
        <p className="nota">
          Nenhuma retrospectiva disponível ainda. A de cada mês fica pronta no dia 1º do mês seguinte, e a do ano em
          1º de janeiro.
        </p>
      ) : (
        <div className="rt-lista">
          {lista.map((p) => (
            <button key={p} className={p.length === 4 ? "rt-ano" : ""} onClick={() => abrir(p)}>
              {p.length === 4 ? `Ano de ${p}` : nomePeriodo(p)}
            </button>
          ))}
        </div>
      )}
      {aberta && <Composicao r={aberta} onFechar={() => setAberta(null)} onErro={onErro} />}
    </section>
  );
}
