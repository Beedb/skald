import { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ChaveAlbum } from "./Colecao";

export type PlaylistResumo = {
  id: number;
  name: string;
  total: number;
  duracao_ms: number;
  primeira_media_id: number | null;
  kind: "user" | "curtidas";
  capa_escolhida: boolean;
  /** Última vez que foi tocada (segundos Unix). */
  tocada_em: number | null;
};

export type Vista =
  | { tipo: "inicio" }
  | { tipo: "biblioteca" }
  | { tipo: "historico" }
  | { tipo: "estatisticas" }
  | { tipo: "configuracoes" }
  | { tipo: "artistas" }
  | { tipo: "artista"; nome: string | null }
  | { tipo: "albuns" }
  /** `de`: a vista de onde o álbum foi aberto (para o "Voltar"). */
  | { tipo: "album"; chave: ChaveAlbum; de: Vista }
  | { tipo: "playlist"; id: number };

type Props = {
  vista: Vista;
  playlists: PlaylistResumo[];
  onVista: (v: Vista) => void;
  onErro: (mensagem: string) => void;
};

const CHAVE_RECOLHIDA = "skald-lateral-recolhida";

/** Barra lateral: biblioteca e playlists. Pode ser recolhida a uma coluna de ícones. */
export default function Lateral({ vista, playlists, onVista, onErro }: Props) {
  const [criando, setCriando] = useState(false);
  const [nome, setNome] = useState("");
  // Enter/Esc e o blur ao desmontar disparam juntos: só a primeira ação vale.
  const finalizado = useRef(false);

  // Recolhida ou não: preferência local desta tela (lembrada entre aberturas).
  const [recolhida, setRecolhida] = useState(() => {
    try {
      return localStorage.getItem(CHAVE_RECOLHIDA) === "1";
    } catch {
      return false;
    }
  });

  function alternar() {
    const nova = !recolhida;
    setRecolhida(nova);
    try {
      localStorage.setItem(CHAVE_RECOLHIDA, nova ? "1" : "0");
    } catch {
      /* sem armazenamento local: vale só nesta sessão */
    }
  }

  function abrir() {
    finalizado.current = false;
    // Para digitar o nome, a barra precisa estar aberta.
    if (recolhida) alternar();
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

  const item = (ativo: boolean, icone: string, rotulo: string, onClick: () => void, qtd?: number, dica?: string) => (
    <button
      className={"item-lateral" + (ativo ? " ativo" : "")}
      onClick={onClick}
      title={recolhida ? rotulo + (qtd !== undefined ? ` · ${qtd}` : "") : dica}
      aria-label={recolhida ? rotulo : undefined}
    >
      <span className="icone" aria-hidden="true">
        {icone}
      </span>
      <span className="nome">{rotulo}</span>
      {qtd !== undefined && <span className="qtd">{qtd}</span>}
    </button>
  );
  const naPlaylist = (id: number) => vista.tipo === "playlist" && vista.id === id;

  return (
    <nav className={"lateral" + (recolhida ? " recolhida" : "")} aria-label="Navegação">
      <button
        className="item-lateral alternar"
        onClick={alternar}
        aria-expanded={!recolhida}
        title={recolhida ? "Expandir barra lateral" : "Recolher barra lateral"}
      >
        <span className="icone" aria-hidden="true">
          {recolhida ? "»" : "«"}
        </span>
        <span className="nome">Recolher</span>
      </button>
      {item(vista.tipo === "inicio", "⌂", "Início", () => onVista({ tipo: "inicio" }))}
      {item(vista.tipo === "biblioteca", "♫", "Músicas", () => onVista({ tipo: "biblioteca" }))}
      {item(vista.tipo === "artistas" || vista.tipo === "artista", "☻", "Artistas", () => onVista({ tipo: "artistas" }))}
      {item(vista.tipo === "albuns" || vista.tipo === "album", "◉", "Álbuns", () => onVista({ tipo: "albuns" }))}
      {item(vista.tipo === "historico", "⟲", "Histórico", () => onVista({ tipo: "historico" }))}
      {item(vista.tipo === "estatisticas", "▤", "Estatísticas", () => onVista({ tipo: "estatisticas" }))}
      {item(vista.tipo === "configuracoes", "⚙", "Configurações", () => onVista({ tipo: "configuracoes" }))}

      {playlists
        .filter((p) => p.kind === "curtidas")
        .map((p) => (
          <div key={p.id}>{item(naPlaylist(p.id), "♥", p.name, () => onVista({ tipo: "playlist", id: p.id }), p.total)}</div>
        ))}

      <h2>Playlists</h2>
      <ul>
        {playlists.filter((p) => p.kind === "user").map((p) => (
          <li key={p.id}>
            {item(
              naPlaylist(p.id),
              (p.name.trim()[0] ?? "♪").toUpperCase(),
              p.name,
              () => onVista({ tipo: "playlist", id: p.id }),
              p.total,
              `${p.name} · ${p.total} ${p.total === 1 ? "música" : "músicas"}`,
            )}
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
        <button className="item-lateral criar" onClick={abrir} title={recolhida ? "Nova playlist" : undefined}>
          <span className="icone" aria-hidden="true">
            +
          </span>
          <span className="nome">Nova playlist</span>
        </button>
      )}
    </nav>
  );
}
