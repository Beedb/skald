import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Informações da música (seção 6, enriquecimento): sugestões do MusicBrainz e capa do
 * Cover Art Archive. Cada campo só muda se estiver marcado; campos vazios vêm marcados
 * ao escolher uma sugestão, os preenchidos não (nunca sobrescrever sem o usuário ver).
 */

type Campos = {
  title: string | null;
  artist: string | null;
  album: string | null;
  album_artist: string | null;
  genre: string | null;
  year: number | null;
  track_number: number | null;
  disc_number: number | null;
};
type Chave = keyof Campos;
type Atual = Campos & { tem_capa: boolean; missing: boolean; duracao_ms: number | null };
type Candidato = { campos: Campos; pontuacao: number; duracao_ms: number | null; release_id: string | null };

const CAMPOS: [Chave, string, boolean][] = [
  ["title", "Título", false],
  ["artist", "Artista", false],
  ["album", "Álbum", false],
  ["album_artist", "Artista do álbum", false],
  ["genre", "Gênero", false],
  ["year", "Ano", true],
  ["track_number", "Faixa", true],
  ["disc_number", "Disco", true],
];

const texto = (v: string | number | null) => (v === null || v === undefined ? "" : String(v));
const capaUrl = (release: string) => `https://coverartarchive.org/release/${release}/front-250`;

function duracao(ms: number | null) {
  if (!ms) return "";
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export default function Informacoes({
  id,
  onFechar,
  onSalvo,
  onErro,
}: {
  id: number;
  onFechar: () => void;
  onSalvo: () => void;
  onErro: (m: string) => void;
}) {
  const [atual, setAtual] = useState<Atual | null>(null);
  const [valores, setValores] = useState<Record<Chave, string>>({} as Record<Chave, string>);
  const [marcados, setMarcados] = useState<Set<Chave>>(new Set());
  const [busca, setBusca] = useState({ titulo: "", artista: "" });
  const [candidatos, setCandidatos] = useState<Candidato[] | null>(null);
  const [escolhido, setEscolhido] = useState<number | null>(null);
  const [usarCapa, setUsarCapa] = useState(false);
  const [noArquivo, setNoArquivo] = useState(false);
  const [ocupado, setOcupado] = useState<"buscando" | "salvando" | null>(null);
  const [aviso, setAviso] = useState<string | null>(null);

  useEffect(() => {
    invoke<Atual>("metadata_get", { id })
      .then((a) => {
        setAtual(a);
        setValores(Object.fromEntries(CAMPOS.map(([k]) => [k, texto(a[k])])) as Record<Chave, string>);
        setBusca({ titulo: a.title ?? "", artista: a.artist ?? "" });
      })
      .catch((e) => onErro(String(e)));
  }, [id]);

  useEffect(() => {
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && onFechar();
    window.addEventListener("keydown", tecla);
    return () => window.removeEventListener("keydown", tecla);
  }, [onFechar]);

  async function buscar() {
    setOcupado("buscando");
    setAviso(null);
    try {
      const c = await invoke<Candidato[]>("metadata_lookup", {
        titulo: busca.titulo,
        artista: busca.artista || null,
        duracaoMs: atual?.duracao_ms ?? null,
      });
      setCandidatos(c);
      setEscolhido(null);
      if (c.length === 0) setAviso("Nenhuma sugestão encontrada. Tente mudar o título ou o artista da busca.");
    } catch (e) {
      setAviso(String(e));
    } finally {
      setOcupado(null);
    }
  }

  function escolher(i: number) {
    if (!atual || !candidatos) return;
    const c = candidatos[i].campos;
    const novos = { ...valores };
    const marcar = new Set<Chave>();
    for (const [k] of CAMPOS) {
      const sugestao = c[k];
      if (sugestao === null || sugestao === undefined || sugestao === "") continue;
      // Só campos vazios recebem a sugestão já marcada; os preenchidos ficam como estão.
      if (atual[k] === null || atual[k] === "") {
        novos[k] = texto(sugestao);
        marcar.add(k);
      }
    }
    setValores(novos);
    setMarcados(marcar);
    setEscolhido(i);
    setUsarCapa(!atual.tem_capa && !!candidatos[i].release_id);
  }

  function editar(k: Chave, v: string) {
    setValores({ ...valores, [k]: v });
    setMarcados(new Set(marcados).add(k));
  }

  function alternar(k: Chave) {
    const m = new Set(marcados);
    if (m.has(k)) m.delete(k);
    else m.add(k);
    setMarcados(m);
  }

  const release = escolhido !== null ? candidatos?.[escolhido]?.release_id ?? null : null;
  const capaValida = usarCapa && release !== null;

  async function salvar() {
    const campos = Object.fromEntries(
      CAMPOS.map(([k, , numero]) => {
        if (!marcados.has(k)) return [k, null];
        const v = valores[k].trim();
        return [k, v === "" ? null : numero ? Number(v) : v];
      }),
    );
    for (const [k, nome, numero] of CAMPOS) {
      if (numero && marcados.has(k) && valores[k].trim() !== "" && !Number.isInteger(Number(valores[k]))) {
        setAviso(`${nome}: use só números.`);
        return;
      }
    }
    setOcupado("salvando");
    setAviso(null);
    try {
      await invoke("metadata_apply", { id, campos, noArquivo, releaseCapa: capaValida ? release : null });
      onSalvo();
    } catch (e) {
      setAviso(String(e));
    } finally {
      setOcupado(null);
    }
  }

  if (!atual) return null;
  const sugestao = escolhido !== null ? candidatos?.[escolhido]?.campos : null;
  const nada = marcados.size === 0 && !capaValida;

  return (
    <div className="fundo-modal" onMouseDown={(e) => e.target === e.currentTarget && onFechar()}>
      <section className="modal informacoes" role="dialog" aria-modal="true" aria-label="Informações da música">
        <header>
          <h2>Informações da música</h2>
          <button className="fechar-modal" onClick={onFechar} aria-label="Fechar">
            ×
          </button>
        </header>

        <div className="busca-info">
          <input
            value={busca.titulo}
            onChange={(e) => setBusca({ ...busca, titulo: e.target.value })}
            placeholder="Título"
            aria-label="Título para buscar"
            onKeyDown={(e) => e.key === "Enter" && buscar()}
          />
          <input
            value={busca.artista}
            onChange={(e) => setBusca({ ...busca, artista: e.target.value })}
            placeholder="Artista (opcional)"
            aria-label="Artista para buscar"
            onKeyDown={(e) => e.key === "Enter" && buscar()}
          />
          <button className="primario" onClick={buscar} disabled={ocupado !== null || !busca.titulo.trim()}>
            {ocupado === "buscando" ? "Buscando…" : "Buscar no MusicBrainz"}
          </button>
        </div>
        <p className="nota">Precisa de internet. Envia só o título e o artista da busca.</p>

        {candidatos && candidatos.length > 0 && (
          <ul className="candidatos">
            {candidatos.map((c, i) => (
              <li key={i}>
                <button className={i === escolhido ? "escolhido" : undefined} onClick={() => escolher(i)}>
                  <span className="capa-candidato">
                    {c.release_id && (
                      <img
                        src={capaUrl(c.release_id)}
                        alt=""
                        loading="lazy"
                        onError={(e) => (e.currentTarget.style.visibility = "hidden")}
                      />
                    )}
                  </span>
                  <span className="texto-candidato">
                    <strong>{c.campos.title}</strong> · {c.campos.artist ?? "?"}
                    <span className="detalhe">
                      {c.campos.album ?? "sem álbum"}
                      {c.campos.year ? ` · ${c.campos.year}` : ""}
                      {c.campos.track_number ? ` · faixa ${c.campos.track_number}` : ""}
                      {c.duracao_ms ? ` · ${duracao(c.duracao_ms)}` : ""}
                    </span>
                  </span>
                  <span className="pontuacao" title="Semelhança">
                    {Math.max(0, c.pontuacao)}%
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}

        <table className="campos-info">
          <thead>
            <tr>
              <th>Alterar</th>
              <th>Campo</th>
              <th>Valor</th>
            </tr>
          </thead>
          <tbody>
            {CAMPOS.map(([k, nome, numero]) => {
              const s = sugestao?.[k];
              const diferente = s !== null && s !== undefined && texto(s) !== valores[k];
              return (
                <tr key={k}>
                  <td>
                    <input type="checkbox" checked={marcados.has(k)} onChange={() => alternar(k)} aria-label={`Alterar ${nome}`} />
                  </td>
                  <td>{nome}</td>
                  <td>
                    <input
                      value={valores[k]}
                      inputMode={numero ? "numeric" : undefined}
                      onChange={(e) => editar(k, e.target.value)}
                      aria-label={nome}
                    />
                    <span className="detalhe">
                      atual: {texto(atual[k]) || "vazio"}
                      {diferente && (
                        <>
                          {" · "}
                          <button className="link" onClick={() => editar(k, texto(s!))}>
                            usar sugestão: {texto(s!)}
                          </button>
                        </>
                      )}
                    </span>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>

        {release && (
          <label className="opcao-capa">
            <input type="checkbox" checked={usarCapa} onChange={(e) => setUsarCapa(e.target.checked)} />
            <img src={capaUrl(release)} alt="" onError={(e) => (e.currentTarget.style.visibility = "hidden")} />
            <span>
              {atual.tem_capa ? "Trocar a capa pela do Cover Art Archive" : "Usar esta capa (Cover Art Archive)"}
              {usarCapa && (
                <span className="detalhe">
                  {noArquivo ? " — embutida no arquivo" : " — guardada só no Skáld, o arquivo não muda"}
                </span>
              )}
            </span>
          </label>
        )}

        <fieldset className="destino">
          <legend>Onde gravar</legend>
          <label>
            <input type="radio" checked={!noArquivo} onChange={() => setNoArquivo(false)} /> Só no Skáld (o arquivo não
            muda)
          </label>
          <label>
            <input type="radio" checked={noArquivo} disabled={atual.missing} onChange={() => setNoArquivo(true)} /> No
            arquivo e no Skáld (as tags do arquivo são reescritas)
          </label>
        </fieldset>

        {aviso && <p className="aviso-modal">{aviso}</p>}
        <footer>
          <button onClick={onFechar}>Cancelar</button>
          <button className="primario" onClick={salvar} disabled={ocupado !== null || nada}>
            {ocupado === "salvando" ? "Salvando…" : "Salvar"}
          </button>
        </footer>
      </section>
    </div>
  );
}
