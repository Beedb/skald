import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Pastas monitoradas: novos arquivos entram sozinhos na biblioteca. */
export default function Pastas({ versao, onErro }: { versao: number; onErro: (m: string) => void }) {
  const [pastas, setPastas] = useState<string[]>([]);

  const carregar = () =>
    invoke<string[]>("watched_folders")
      .then(setPastas)
      .catch((e) => onErro(String(e)));
  useEffect(() => {
    carregar();
  }, [versao]);

  async function parar(path: string) {
    try {
      await invoke("unwatch_folder", { path });
      await carregar();
    } catch (e) {
      onErro(String(e));
    }
  }

  return (
    <details className="pastas">
      <summary title="Pastas vigiadas: arquivos novos entram sozinhos na biblioteca">
        Pastas monitoradas ({pastas.length})
      </summary>
      <div className="pastas-lista">
        {pastas.length === 0 ? (
          <p>Nenhuma pasta. Ao importar uma pasta, ela passa a ser monitorada.</p>
        ) : (
          <ul>
            {pastas.map((p) => (
              <li key={p}>
                <span className="caminho" title={p}>
                  {p}
                </span>
                <button
                  onClick={() => parar(p)}
                  title="As músicas desta pasta continuam na biblioteca"
                >
                  Parar de monitorar
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </details>
  );
}
