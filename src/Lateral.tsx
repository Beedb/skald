import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type PlaylistResumo = {
  id: number;
  name: string;
  total: number;
  duracao_ms: number;
  primeira_media_id: number | null;
  kind: "user" | "curtidas";
  capa_escolhida: boolean;
};

export type Vista = { tipo: "biblioteca" } | { tipo: "historico" } | { tipo: "playlist"; id: number };

type Props = {
  vista: Vista;
  playlists: PlaylistResumo[];
  onVista: (v: Vista) => void;
  onErro: (mensagem: string) => void;
};

/** Barra lateral: biblioteca e playlists. */
export default function Lateral({ vista, playlists, onVista, onErro }: Props) {
  const [criando, setCriando] = useState(false);
  const [nome, setNome] = useState("");
  // Enter/Esc e o blur ao desmontar disparam juntos: só a primeira ação vale.
  const finalizado = useRef(false);

  function abrir() {
    finalizado.current = false;
    setCriando(true);
  }

  function cancelar() {
    finalizado.current = true;
    setNome("");
    setCriando(false);
  }

  async function criar() {
    if (finalizado.current) return;
    finalizado.current = true;
    const n = nome.trim();
    setCriando(false);
    setNome("");
    if (!n) return;
    try {
      const id = await invoke<number>("playlist_create", { name: n });
      onVista({ tipo: "playlist", id });
    } catch (e) {
      onErro(String(e));
    }
  }

  return (
    <nav className="lateral" aria-label="Navegação">
      <button
        className={"item-lateral" + (vista.tipo === "biblioteca" ? " ativo" : "")}
        onClick={() => onVista({ tipo: "biblioteca" })}
      >
        ♫ Músicas
      </button>
      <button
        className={"item-lateral" + (vista.tipo === "historico" ? " ativo" : "")}
        onClick={() => onVista({ tipo: "historico" })}
      >
        ⟲ Histórico
      </button>

      {playlists
        .filter((p) => p.kind === "curtidas")
        .map((p) => (
          <button
            key={p.id}
            className={"item-lateral" + (vista.tipo === "playlist" && vista.id === p.id ? " ativo" : "")}
            onClick={() => onVista({ tipo: "playlist", id: p.id })}
          >
            <span className="nome">♥ {p.name}</span>
            <span className="qtd">{p.total}</span>
          </button>
        ))}

      <h2>Playlists</h2>
      <ul>
        {playlists.filter((p) => p.kind === "user").map((p) => (
          <li key={p.id}>
            <button
              className={"item-lateral" + (vista.tipo === "playlist" && vista.id === p.id ? " ativo" : "")}
              onClick={() => onVista({ tipo: "playlist", id: p.id })}
              title={`${p.name} · ${p.total} ${p.total === 1 ? "música" : "músicas"}`}
            >
              <span className="nome">{p.name}</span>
              <span className="qtd">{p.total}</span>
            </button>
          </li>
        ))}
      </ul>

      {criando ? (
        <input
          className="nova-playlist"
          autoFocus
          placeholder="Nome da playlist"
          aria-label="Nome da nova playlist"
          maxLength={200}
          value={nome}
          onChange={(e) => setNome(e.target.value)}
          onBlur={criar}
          onKeyDown={(e) => {
            if (e.key === "Enter") criar();
            if (e.key === "Escape") {
              e.stopPropagation();
              cancelar();
            }
          }}
        />
      ) : (
        <button className="item-lateral criar" onClick={abrir}>
          + Nova playlist
        </button>
      )}
    </nav>
  );
}
