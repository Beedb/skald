import { createContext, ReactNode, useContext, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PlaylistResumo } from "./Lateral";

/**
 * Menu de contexto de música, o mesmo em todas as telas (biblioteca, início, álbum,
 * playlist, fila, histórico). Cada tela chama `useMenuMusica()` no clique direito.
 * O menu do navegador (Voltar/Recarregar…) fica bloqueado fora dos campos de texto.
 */

type Opcoes = {
  /** Substitui "Tocar agora" (ex.: tocar dentro da playlist ou do álbum). */
  tocar?: () => void;
  /** Músicas selecionadas (com mais de uma, o menu age sobre todas). */
  selecao?: number[];
};

type Aberto = { x: number; y: number; id: number; opcoes: Opcoes; confirmar?: boolean };

type Abrir = (e: React.MouseEvent, id: number, opcoes?: Opcoes) => void;

const Contexto = createContext<Abrir>(() => {});

export const useMenuMusica = () => useContext(Contexto);

type Props = {
  children: ReactNode;
  playlists: PlaylistResumo[];
  onTocar: (id: number) => void;
  onEnfileirar: (id: number, aSeguir: boolean) => void;
  onAdicionarPlaylist: (playlistId: number, mediaId: number) => void;
  onNovaPlaylistCom: (mediaId: number) => void;
  onRemover: (mediaId: number) => void;
  onInformacoes: (mediaId: number) => void;
  onEditarLote: (ids: number[]) => void;
  onErro: (m: string) => void;
};

const LARGURA = 220;
const ALTURA = 380;

export function MenuMusicaProvider({ children, playlists, ...acoes }: Props) {
  const [menu, setMenu] = useState<Aberto | null>(null);

  const abrir: Abrir = (e, id, opcoes = {}) => {
    e.preventDefault();
    e.stopPropagation();
    setMenu({
      x: Math.max(4, Math.min(e.clientX, window.innerWidth - LARGURA)),
      y: Math.max(4, Math.min(e.clientY, window.innerHeight - ALTURA)),
      id,
      opcoes,
    });
  };

  // Sem menu do navegador no app; campos de texto mantêm copiar/colar.
  useEffect(() => {
    const bloquear = (e: MouseEvent) => {
      const alvo = e.target as HTMLElement | null;
      if (alvo?.closest("input, textarea, [contenteditable='true']")) return;
      e.preventDefault();
    };
    document.addEventListener("contextmenu", bloquear);
    return () => document.removeEventListener("contextmenu", bloquear);
  }, []);

  // Fecha ao clicar fora, rolar qualquer lista, perder o foco ou apertar Esc.
  useEffect(() => {
    if (!menu) return;
    const fechar = () => setMenu(null);
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && fechar();
    // Rolar a lista de playlists dentro do próprio menu não o fecha.
    const rolou = (e: Event) => !(e.target as HTMLElement).closest?.(".menu-contexto") && fechar();
    window.addEventListener("click", fechar);
    window.addEventListener("blur", fechar);
    window.addEventListener("keydown", tecla);
    window.addEventListener("scroll", rolou, true);
    return () => {
      window.removeEventListener("click", fechar);
      window.removeEventListener("blur", fechar);
      window.removeEventListener("keydown", tecla);
      window.removeEventListener("scroll", rolou, true);
    };
  }, [menu]);

  return (
    <Contexto.Provider value={abrir}>
      {children}
      {menu && menu.opcoes.selecao && menu.opcoes.selecao.length > 1 && (
        <MenuSelecao ids={menu.opcoes.selecao} x={menu.x} y={menu.y} playlists={playlists} acoes={acoes} />
      )}
      {menu && !(menu.opcoes.selecao && menu.opcoes.selecao.length > 1) && (
        <ul className="menu-contexto" role="menu" style={{ left: menu.x, top: menu.y }}>
          <li role="menuitem" onClick={() => (menu.opcoes.tocar ? menu.opcoes.tocar() : acoes.onTocar(menu.id))}>
            Tocar agora
          </li>
          <li role="menuitem" onClick={() => acoes.onEnfileirar(menu.id, true)}>
            Tocar a seguir
          </li>
          <li role="menuitem" onClick={() => acoes.onEnfileirar(menu.id, false)}>
            Adicionar à fila
          </li>
          <li role="separator" className="separador" />
          <li role="presentation" className="titulo-menu">
            Adicionar à playlist
          </li>
          {playlists
            .filter((p) => p.kind === "user")
            .map((p) => (
              <li key={p.id} role="menuitem" className="recuado" onClick={() => acoes.onAdicionarPlaylist(p.id, menu.id)}>
                {p.name}
              </li>
            ))}
          <li role="menuitem" className="recuado" onClick={() => acoes.onNovaPlaylistCom(menu.id)}>
            + Nova playlist com esta música
          </li>
          <li role="separator" className="separador" />
          <li role="menuitem" onClick={() => acoes.onInformacoes(menu.id)}>
            Informações e sugestões…
          </li>
          {menu.confirmar ? (
            <li role="menuitem" className="perigo" onClick={() => acoes.onRemover(menu.id)}>
              Confirmar: remover (o arquivo fica no disco)
            </li>
          ) : (
            <li
              role="menuitem"
              onClick={(e) => {
                e.stopPropagation();
                setMenu({ ...menu, confirmar: true });
              }}
              title="Tira a música da biblioteca e das playlists; o arquivo não é apagado e o histórico fica"
            >
              Remover da biblioteca…
            </li>
          )}
        </ul>
      )}
    </Contexto.Provider>
  );
}

/** Menu para várias músicas selecionadas. */
function MenuSelecao({
  ids,
  x,
  y,
  playlists,
  acoes,
}: {
  ids: number[];
  x: number;
  y: number;
  playlists: PlaylistResumo[];
  acoes: Omit<Props, "children" | "playlists">;
}) {
  const fazer = (cmd: string, args: Record<string, unknown>) => invoke(cmd, args).catch((e) => acoes.onErro(String(e)));
  return (
    <ul className="menu-contexto" role="menu" style={{ left: x, top: y }}>
      <li role="presentation" className="titulo-menu">
        {ids.length} músicas selecionadas
      </li>
      <li role="menuitem" onClick={() => fazer("queue_add_many", { ids, aSeguir: true })}>
        Tocar a seguir
      </li>
      <li role="menuitem" onClick={() => fazer("queue_add_many", { ids, aSeguir: false })}>
        Adicionar à fila
      </li>
      <li role="separator" className="separador" />
      <li role="presentation" className="titulo-menu">
        Adicionar à playlist
      </li>
      {playlists
        .filter((p) => p.kind === "user")
        .map((p) => (
          <li key={p.id} role="menuitem" className="recuado" onClick={() => fazer("playlist_add", { id: p.id, mediaIds: ids })}>
            {p.name}
          </li>
        ))}
      <li role="separator" className="separador" />
      <li role="menuitem" onClick={() => acoes.onEditarLote(ids)}>
        Editar {ids.length} músicas…
      </li>
    </ul>
  );
}
