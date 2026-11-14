import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Musica = { id: number; path: string; title: string | null; artist: string | null };
type Grupo = { hash: string; musicas: Musica[] };

/** Arquivos idênticos (mesmo hash). Só avisa: nenhum arquivo é apagado. */
export default function Duplicados({ versao, onErro }: { versao: number; onErro: (m: string) => void }) {
  const [grupos, setGrupos] = useState<Grupo[]>([]);

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

  if (grupos.length === 0) return null;
  return (
    <details className="pastas">
      <summary title="Arquivos idênticos na biblioteca. Nada é apagado.">Duplicados ({grupos.length})</summary>
      <div className="pastas-lista">
        <p>Arquivos com o mesmo conteúdo. O Skáld não apaga nada; você decide o que fazer.</p>
        <ul>
          {grupos.map((g) => (
            <li key={g.hash} className="grupo-duplicados">
              <div className="caminhos">
                <strong>{[g.musicas[0].title, g.musicas[0].artist].filter(Boolean).join(" — ")}</strong>
                {g.musicas.map((m) => (
                  <span key={m.id} className="caminho" title={m.path}>
                    {m.path}
                  </span>
                ))}
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
