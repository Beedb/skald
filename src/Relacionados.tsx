import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { MediaRow } from "./Biblioteca";
import { formatarDuracao } from "./Biblioteca";
import Capa from "./Capa";
import { useMenuMusica } from "./MenuMusica";

type Dados = { album: MediaRow[]; artistas: string[]; do_artista: MediaRow[] };

function Lista({ itens, onErro }: { itens: MediaRow[]; onErro: (m: string) => void }) {
  const abrirMenu = useMenuMusica();
  return (
    <ul className="lista-relacionadas">
      {itens.map((m) => (
        <li key={m.id}>
          <button
            className={m.missing ? "ausente" : undefined}
            onClick={() => invoke("play_media", { id: m.id, busca: "" }).catch((e) => onErro(String(e)))}
            onContextMenu={(e) => abrirMenu(e, m.id)}
            title={m.missing ? "Arquivo não encontrado" : "Tocar"}
          >
            <Capa id={m.id} className="capa-fila" />
            <span className="faixa">
              <strong>{m.title ?? "Sem título"}</strong>
              <span>{m.artist ?? "Artista desconhecido"}</span>
            </span>
            <span className="tempo">{formatarDuracao(m.duration_ms)}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}

/** Aba "Relacionados" do player completo: o resto do álbum e mais dos artistas, só da biblioteca. */
export default function Relacionados({ id, onErro }: { id: number | null; onErro: (m: string) => void }) {
  const [dados, setDados] = useState<Dados | null>(null);
  useEffect(() => {
    if (id === null) return setDados(null);
    let valido = true;
    invoke<Dados>("related_tracks", { id })
      .then((d) => valido && setDados(d))
      .catch((e) => onErro(String(e)));
    return () => {
      valido = false;
    };
  }, [id]);

  if (id === null) return <p className="dica">Nada tocando.</p>;
  if (!dados) return null;
  if (dados.album.length === 0 && dados.do_artista.length === 0)
    return <p className="dica">Nenhuma outra música deste álbum ou artista na biblioteca.</p>;
  return (
    <div className="relacionados">
      {dados.album.length > 0 && (
        <section>
          <h3>Do mesmo álbum</h3>
          <Lista itens={dados.album} onErro={onErro} />
        </section>
      )}
      {dados.do_artista.length > 0 && (
        <section>
          <h3>Mais de {dados.artistas.join(", ")}</h3>
          <Lista itens={dados.do_artista} onErro={onErro} />
        </section>
      )}
    </div>
  );
}
