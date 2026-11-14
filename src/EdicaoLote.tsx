import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Edição em lote (seção 5): artista, álbum, gênero e ano de várias músicas de uma vez. */

type Valor<T> = { valor: T | null; varios: boolean };
type Lote = { artist: Valor<string>; album: Valor<string>; genre: Valor<string>; year: Valor<number>; musicas: number };
type Chave = "artist" | "album" | "genre" | "year";
type Resultado = { alteradas: number; falhas: string[] };

const CAMPOS: [Chave, string][] = [
  ["artist", "Artista"],
  ["album", "Álbum"],
  ["genre", "Gênero"],
  ["year", "Ano"],
];

export default function EdicaoLote({
  ids,
  onFechar,
  onSalvo,
  onErro,
}: {
  ids: number[];
  onFechar: () => void;
  onSalvo: (mensagem: string) => void;
  onErro: (m: string) => void;
}) {
  const [lote, setLote] = useState<Lote | null>(null);
  const [valores, setValores] = useState<Record<Chave, string>>({ artist: "", album: "", genre: "", year: "" });
  const [marcados, setMarcados] = useState<Set<Chave>>(new Set());
  const [noArquivo, setNoArquivo] = useState(false);
  const [salvando, setSalvando] = useState(false);
  const [aviso, setAviso] = useState<string | null>(null);
  // Parte foi salva e parte falhou: fechar ainda precisa atualizar a lista.
  const [parcial, setParcial] = useState<string | null>(null);
  const fechar = () => (parcial ? onSalvo(parcial) : onFechar());

  useEffect(() => {
    invoke<Lote>("batch_values", { ids })
      .then((l) => {
        setLote(l);
        setValores({
          artist: l.artist.valor ?? "",
          album: l.album.valor ?? "",
          genre: l.genre.valor ?? "",
          year: l.year.valor !== null ? String(l.year.valor) : "",
        });
      })
      .catch((e) => onErro(String(e)));
  }, [ids]);

  useEffect(() => {
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && !salvando && fechar();
    window.addEventListener("keydown", tecla);
    return () => window.removeEventListener("keydown", tecla);
  });

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

  async function salvar() {
    if (marcados.has("year") && valores.year.trim() && !/^\d{4}$/.test(valores.year.trim())) {
      setAviso("Ano: use quatro dígitos.");
      return;
    }
    const campos = Object.fromEntries(
      CAMPOS.map(([k]) => {
        const v = valores[k].trim();
        if (!marcados.has(k) || v === "") return [k, null];
        return [k, k === "year" ? Number(v) : v];
      }),
    );
    setSalvando(true);
    setAviso(null);
    try {
      const r = await invoke<Resultado>("batch_edit", { ids, campos, noArquivo });
      if (r.falhas.length === 0) {
        onSalvo(`${r.alteradas} ${r.alteradas === 1 ? "música alterada" : "músicas alteradas"}.`);
      } else {
        if (r.alteradas > 0) setParcial(`${r.alteradas} de ${ids.length} músicas alteradas.`);
        setAviso(
          `${r.alteradas} alterada(s); ${r.falhas.length} com problema:\n` +
            r.falhas.slice(0, 5).join("\n") +
            (r.falhas.length > 5 ? `\n… e mais ${r.falhas.length - 5}` : ""),
        );
      }
    } catch (e) {
      setAviso(String(e));
    } finally {
      setSalvando(false);
    }
  }

  if (!lote) return null;
  const vazios = CAMPOS.filter(([k]) => marcados.has(k) && valores[k].trim() === "").length;

  return (
    <div className="fundo-modal" onMouseDown={(e) => e.target === e.currentTarget && !salvando && fechar()}>
      <section className="modal lote" role="dialog" aria-modal="true" aria-label="Editar várias músicas">
        <header>
          <h2>
            Editar {lote.musicas} {lote.musicas === 1 ? "música" : "músicas"}
          </h2>
          <button className="fechar-modal" onClick={fechar} disabled={salvando} aria-label="Fechar">
            ×
          </button>
        </header>
        <p className="nota">Só os campos marcados mudam, e o mesmo valor vai para todas as músicas.</p>
        <table className="campos-info">
          <tbody>
            {CAMPOS.map(([k, nome]) => (
              <tr key={k}>
                <td>
                  <input type="checkbox" checked={marcados.has(k)} onChange={() => alternar(k)} aria-label={`Alterar ${nome}`} />
                </td>
                <td>{nome}</td>
                <td>
                  <input
                    value={valores[k]}
                    placeholder={lote[k].varios ? "(vários valores)" : "(vazio)"}
                    inputMode={k === "year" ? "numeric" : undefined}
                    onChange={(e) => editar(k, e.target.value)}
                    aria-label={nome}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {vazios > 0 && <p className="nota">Campo marcado e vazio não apaga nada: fica como está.</p>}

        <fieldset className="destino">
          <legend>Onde gravar</legend>
          <label>
            <input type="radio" checked={!noArquivo} onChange={() => setNoArquivo(false)} /> Só no Skáld (os arquivos não
            mudam)
          </label>
          <label>
            <input type="radio" checked={noArquivo} onChange={() => setNoArquivo(true)} /> Nos arquivos e no Skáld (as tags
            são reescritas; arquivos ausentes ficam de fora)
          </label>
        </fieldset>

        {aviso && <p className="aviso-modal pre">{aviso}</p>}
        <footer>
          <button onClick={fechar} disabled={salvando}>
            {aviso ? "Fechar" : "Cancelar"}
          </button>
          <button className="primario" onClick={salvar} disabled={salvando || marcados.size === 0}>
            {salvando ? "Salvando…" : "Salvar"}
          </button>
        </footer>
      </section>
    </div>
  );
}
