import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Pencil, Play, Trash2 } from "lucide-react";
import { noAndroid } from "./android";

type Musica = { id: number; path: string; title: string | null; artist: string | null };
type Grupo = { hash: string; musicas: Musica[] };
type Resolvido = { removidas: number; na_lixeira: number; falhas: string[] };

const separar = (p: string) => {
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return { pasta: i >= 0 ? p.slice(0, i) : "", arquivo: p.slice(i + 1) };
};
const semExtensao = (n: string) => (n.lastIndexOf(".") > 0 ? n.slice(0, n.lastIndexOf(".")) : n);

/** Arquivos idênticos (mesmo hash): escolher qual fica, renomear ou ignorar o aviso. */
export default function Duplicados({ versao, onErro }: { versao: number; onErro: (m: string) => void }) {
  const [grupos, setGrupos] = useState<Grupo[]>([]);
  /** Cópia escolhida para ficar (aguardando confirmar o que fazer com as outras). */
  const [ficar, setFicar] = useState<number | null>(null);
  const [renomeando, setRenomeando] = useState<{ id: number; nome: string } | null>(null);
  const [aviso, setAviso] = useState<string | null>(null);
  const [ocupado, setOcupado] = useState(false);

  const carregar = () =>
    invoke<Grupo[]>("duplicates_list")
      .then(setGrupos)
      .catch((e) => onErro(String(e)));
  useEffect(() => {
    carregar();
  }, [versao]);

  async function ignorar(hash: string) {
    try {
      await invoke("duplicate_ignore", { hash });
      await carregar();
    } catch (e) {
      onErro(String(e));
    }
  }

  async function manter(g: Grupo, id: number, lixeira: boolean) {
    setOcupado(true);
    try {
      const r = await invoke<Resolvido>("duplicate_resolve", {
        manter: id,
        outros: g.musicas.filter((m) => m.id !== id).map((m) => m.id),
        lixeira,
      });
      setAviso(
        (lixeira
          ? `${r.na_lixeira} ${r.na_lixeira === 1 ? "cópia foi" : "cópias foram"} para a Lixeira`
          : `${r.removidas} ${r.removidas === 1 ? "cópia saiu" : "cópias saíram"} da biblioteca (os arquivos ficam no disco)`) +
          (r.falhas.length ? `. Não deu: ${r.falhas.join("; ")}` : "."),
      );
      setFicar(null);
      await carregar();
    } catch (e) {
      onErro(String(e));
    } finally {
      setOcupado(false);
    }
  }

  async function renomear() {
    if (!renomeando) return;
    setOcupado(true);
    try {
      await invoke("duplicate_rename", { id: renomeando.id, nome: renomeando.nome });
      setRenomeando(null);
      await carregar();
    } catch (e) {
      onErro(String(e));
    } finally {
      setOcupado(false);
    }
  }

  if (grupos.length === 0 && !aviso) return null;
  return (
    <details className="pastas">
      <summary title="Arquivos idênticos na biblioteca">Duplicados ({grupos.length})</summary>
      <div className="pastas-lista duplicados">
        <p>
          Arquivos com o mesmo conteúdo. Escolha a cópia que fica: playlists, curtidas e histórico das outras passam para
          ela. {noAndroid ? "" : "As outras podem ir para a Lixeira (dá para recuperar) ou só sair da biblioteca."}
        </p>
        {aviso && <p className="nota destaque">{aviso}</p>}
        <ul>
          {grupos.map((g) => (
            <li key={g.hash} className="grupo-duplicados">
              <div className="caminhos">
                <strong>{[g.musicas[0].title, g.musicas[0].artist].filter(Boolean).join(" — ") || "Sem título"}</strong>
                {g.musicas.map((m) => {
                  const { pasta, arquivo } = separar(m.path);
                  const outras = g.musicas.length - 1;
                  return (
                    <div key={m.id} className="copia-duplicada">
                      {renomeando?.id === m.id ? (
                        <form
                          className="renomear-copia"
                          onSubmit={(e) => {
                            e.preventDefault();
                            renomear();
                          }}
                        >
                          <input
                            autoFocus
                            value={renomeando.nome}
                            aria-label="Novo nome do arquivo (sem a extensão)"
                            onChange={(e) => setRenomeando({ id: m.id, nome: e.target.value })}
                            onKeyDown={(e) => e.key === "Escape" && setRenomeando(null)}
                          />
                          <button type="submit" className="primario" disabled={ocupado}>
                            Renomear
                          </button>
                          <button type="button" onClick={() => setRenomeando(null)}>
                            Cancelar
                          </button>
                        </form>
                      ) : (
                        <span className="caminho" title={m.path}>
                          <span className="arquivo">{arquivo}</span>
                          <span className="pasta">{pasta}</span>
                        </span>
                      )}
                      {renomeando?.id !== m.id && (
                        <span className="acoes-copia">
                          <button
                            className="icone-botao"
                            title="Tocar esta cópia"
                            aria-label={`Tocar ${arquivo}`}
                            onClick={() => invoke("play_media", { id: m.id, busca: "" }).catch((e) => onErro(String(e)))}
                          >
                            <Play size={14} />
                          </button>
                          <button
                            className="icone-botao"
                            title="Renomear o arquivo"
                            aria-label={`Renomear ${arquivo}`}
                            onClick={() => setRenomeando({ id: m.id, nome: semExtensao(arquivo) })}
                          >
                            <Pencil size={14} />
                          </button>
                          <button
                            title={`Ficar com esta e decidir o que fazer com ${outras === 1 ? "a outra" : `as outras ${outras}`}`}
                            onClick={() => setFicar(ficar === m.id ? null : m.id)}
                            aria-expanded={ficar === m.id}
                          >
                            <Check size={14} aria-hidden="true" /> Manter esta
                          </button>
                        </span>
                      )}
                      {ficar === m.id && (
                        <div className="confirmar-copia">
                          <span>
                            {outras === 1 ? "A outra cópia:" : `As outras ${outras} cópias:`}
                          </span>
                          {!noAndroid && (
                            <button className="perigo" disabled={ocupado} onClick={() => manter(g, m.id, true)}>
                              <Trash2 size={14} aria-hidden="true" /> Mandar para a Lixeira
                            </button>
                          )}
                          <button disabled={ocupado} onClick={() => manter(g, m.id, false)}>
                            Só tirar da biblioteca
                          </button>
                          <button onClick={() => setFicar(null)}>Cancelar</button>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
              <button onClick={() => ignorar(g.hash)} title="Manter todas as cópias e parar de avisar sobre este grupo">
                Ignorar aviso
              </button>
            </li>
          ))}
        </ul>
      </div>
    </details>
  );
}
