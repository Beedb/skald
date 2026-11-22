import { tocarAo } from "./android";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { useVirtualizer } from "@tanstack/react-virtual";
import { formatarDuracao, type MediaRow } from "./Biblioteca";
import { Capa } from "./PlayerBar";
import type { PlaylistResumo } from "./Lateral";
import { useMenuMusica } from "./MenuMusica";
import AdicionarMusicas from "./AdicionarMusicas";
import { EllipsisVertical, Play, Plus } from "lucide-react";

type Item = MediaRow & { item_id: number };
type Ordem = "personalizada" | "titulo" | "artista";

const ALTURA_LINHA = 36;

function duracaoTotal(ms: number) {
  const min = Math.round(ms / 60000);
  if (min < 60) return `${min} min`;
  return `${Math.floor(min / 60)} h ${min % 60} min`;
}

type Props = {
  playlist: PlaylistResumo;
  /** Muda quando qualquer playlist é alterada: recarrega os itens. */
  versao: number;
  tocandoId: number | null;
  onExcluida: () => void;
  onErro: (mensagem: string) => void;
};

export default function PlaylistView({ playlist, versao, tocandoId, onExcluida, onErro }: Props) {
  const [itens, setItens] = useState<Item[]>([]);
  const abrirMenu = useMenuMusica();
  const [ordem, setOrdem] = useState<Ordem>("personalizada");
  const [capa, setCapa] = useState<string | null>(null);
  const [renomeando, setRenomeando] = useState(false);
  const [nome, setNome] = useState(playlist.name);
  const [aviso, setAviso] = useState<string | null>(null);
  const [adicionando, setAdicionando] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [pastas, setPastas] = useState<{ id: number; nome: string }[]>([]);
  const fixa = playlist.kind === "curtidas";
  const finalizado = useRef(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let valido = true;
    invoke<Item[]>("playlist_items", { id: playlist.id, ordem })
      .then((i) => valido && setItens(i))
      .catch((e) => onErro(String(e)));
    return () => {
      valido = false;
    };
  }, [playlist.id, ordem, versao]);

  // Capa escolhida pelo usuário; sem ela, a da primeira música da ordem personalizada.
  useEffect(() => {
    let valido = true;
    const id = playlist.primeira_media_id;
    const pedido = playlist.capa_escolhida
      ? invoke<string | null>("playlist_cover", { id: playlist.id })
      : id === null
        ? Promise.resolve(null)
        : invoke<{ capa: string | null } | null>("get_track_details", { id }).then((d) => d?.capa ?? null);
    pedido.then((c) => valido && setCapa(c)).catch(() => valido && setCapa(null));
    return () => {
      valido = false;
    };
  }, [playlist.id, playlist.primeira_media_id, playlist.capa_escolhida, versao]);

  // Trocar de playlist: volta ao topo e à ordem personalizada.
  useEffect(() => {
    setOrdem("personalizada");
    setRenomeando(false);
    setAviso(null);
    setMenu(null);
    setAdicionando(false);
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
  }, [playlist.id]);

  const virtualizer = useVirtualizer({
    count: itens.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ALTURA_LINHA,
    overscan: 20,
  });

  // Menu ⋮: fecha ao clicar fora, rolar, perder o foco ou apertar Esc.
  useEffect(() => {
    if (!menu) return;
    const fechar = () => setMenu(null);
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && fechar();
    window.addEventListener("click", fechar);
    window.addEventListener("blur", fechar);
    window.addEventListener("keydown", tecla);
    window.addEventListener("scroll", fechar, true);
    return () => {
      window.removeEventListener("click", fechar);
      window.removeEventListener("blur", fechar);
      window.removeEventListener("keydown", tecla);
      window.removeEventListener("scroll", fechar, true);
    };
  }, [menu]);

  function abrirMenuPlaylist(e: React.MouseEvent<HTMLButtonElement>) {
    e.stopPropagation();
    if (menu) return setMenu(null);
    invoke<{ id: number; nome: string }[]>("playlist_folders")
      .then(setPastas)
      .catch(() => {});
    const r = e.currentTarget.getBoundingClientRect();
    setMenu({ x: Math.max(4, Math.min(r.left, window.innerWidth - 230)), y: r.bottom + 4 });
  }

  const acao = (cmd: string, args: Record<string, unknown>) => invoke(cmd, args).catch((e) => onErro(String(e)));

  function abrirRenomear() {
    finalizado.current = false;
    setNome(playlist.name);
    setRenomeando(true);
  }

  function salvarNome() {
    if (finalizado.current) return;
    finalizado.current = true;
    setRenomeando(false);
    if (nome.trim() && nome.trim() !== playlist.name) acao("playlist_rename", { id: playlist.id, name: nome });
  }

  async function excluir() {
    const ok = await ask(`Excluir a playlist "${playlist.name}"? As músicas continuam na biblioteca.`, {
      title: "Excluir playlist",
      kind: "warning",
      okLabel: "Excluir",
      cancelLabel: "Cancelar",
    });
    if (!ok) return;
    await acao("playlist_delete", { id: playlist.id });
    onExcluida();
  }

  async function escolherCapa() {
    const arquivo = await open({
      title: "Escolher capa da playlist",
      multiple: false,
      directory: false,
      filters: [{ name: "Imagens", extensions: ["jpg", "jpeg", "png", "webp"] }],
    });
    if (typeof arquivo === "string") acao("playlist_set_cover", { id: playlist.id, origem: arquivo });
  }

  async function exportar() {
    const destino = await save({
      title: "Exportar playlist em M3U",
      defaultPath: `${playlist.name.replace(/[/\\]/g, "-")}.m3u`,
      filters: [{ name: "Playlist M3U", extensions: ["m3u", "m3u8"] }],
    });
    if (!destino) return;
    try {
      const n = await invoke<number>("playlist_export_m3u", { id: playlist.id, destino });
      setAviso(`${n} ${n === 1 ? "música exportada" : "músicas exportadas"} para ${destino}`);
    } catch (e) {
      onErro(String(e));
    }
  }

  const tocar = (indice: number) => acao("play_playlist", { id: playlist.id, ordem, indice });
  const personalizada = ordem === "personalizada";

  return (
    <section className="playlist-view" aria-label={`Playlist ${playlist.name}`}>
      <header className="pl-cabecalho">
        <Capa src={capa} className="capa-playlist" />
        <div className="pl-titulos">
          <span className="pl-tipo">{fixa ? "Playlist fixa" : "Playlist"}</span>
          {renomeando ? (
            <input
              className="pl-renomear"
              autoFocus
              aria-label="Nome da playlist"
              maxLength={200}
              value={nome}
              onChange={(e) => setNome(e.target.value)}
              onBlur={salvarNome}
              onKeyDown={(e) => {
                if (e.key === "Enter") salvarNome();
                if (e.key === "Escape") {
                  e.stopPropagation();
                  finalizado.current = true;
                  setRenomeando(false);
                }
              }}
            />
          ) : (
            <h2 onDoubleClick={fixa ? undefined : abrirRenomear} title={fixa ? undefined : "Dois cliques para renomear"}>
              {playlist.name}
            </h2>
          )}
          <span className="pl-meta">
            {playlist.total} {playlist.total === 1 ? "música" : "músicas"}
            {playlist.total > 0 && ` · ${duracaoTotal(playlist.duracao_ms)}`}
          </span>
          <div className="pl-acoes">
            <button className="primario" onClick={() => tocar(0)} disabled={itens.length === 0}>
              <Play size={15} aria-hidden="true" /> Tocar
            </button>
            <button onClick={() => setAdicionando(true)}>
              <Plus size={15} aria-hidden="true" /> Adicionar músicas
            </button>
            <button
              className="pl-mais"
              onClick={abrirMenuPlaylist}
              aria-label="Mais opções da playlist"
              aria-haspopup="menu"
              aria-expanded={menu !== null}
              title="Mais opções"
            >
              <EllipsisVertical size={16} />
            </button>
            <label>
              Ordem:{" "}
              <select value={ordem} onChange={(e) => setOrdem(e.target.value as Ordem)}>
                <option value="personalizada">Personalizada</option>
                <option value="titulo">Título (A–Z)</option>
                <option value="artista">Artista (A–Z)</option>
              </select>
            </label>
          </div>
          {aviso && <span className="pl-aviso">{aviso}</span>}
        </div>
      </header>

      {menu && (
        <ul className="menu-contexto" role="menu" style={{ left: menu.x, top: menu.y }}>
          <li
            role="menuitem"
            aria-disabled={itens.length === 0}
            className={itens.length === 0 ? "desativado" : undefined}
            onClick={() => itens.length > 0 && acao("queue_add_playlist", { id: playlist.id, ordem, aSeguir: true })}
          >
            Tocar em seguida
          </li>
          <li
            role="menuitem"
            aria-disabled={itens.length === 0}
            className={itens.length === 0 ? "desativado" : undefined}
            onClick={() => itens.length > 0 && acao("queue_add_playlist", { id: playlist.id, ordem, aSeguir: false })}
          >
            Adicionar à fila
          </li>
          <li role="separator" className="separador" />
          {!fixa && (
            <li role="menuitem" onClick={abrirRenomear}>
              Renomear
            </li>
          )}
          <li role="menuitem" onClick={escolherCapa}>
            Escolher capa
          </li>
          {playlist.capa_escolhida && (
            <li role="menuitem" onClick={() => acao("playlist_clear_cover", { id: playlist.id })}>
              Usar capa automática
            </li>
          )}
          <li
            role="menuitem"
            aria-disabled={itens.length === 0}
            className={itens.length === 0 ? "desativado" : undefined}
            onClick={() => itens.length > 0 && exportar()}
          >
            Exportar M3U
          </li>
          {!fixa && pastas.length > 0 && (
            <>
              <li role="separator" className="separador" />
              <li className="titulo-menu">Pasta</li>
              {pastas.map((pa) => (
                <li
                  key={pa.id}
                  role="menuitemradio"
                  aria-checked={playlist.pasta_id === pa.id}
                  onClick={() => acao("playlist_set_folder", { id: playlist.id, pasta: pa.id })}
                >
                  {playlist.pasta_id === pa.id ? "✓ " : ""}
                  {pa.nome}
                </li>
              ))}
              <li
                role="menuitemradio"
                aria-checked={playlist.pasta_id === null}
                onClick={() => acao("playlist_set_folder", { id: playlist.id, pasta: null })}
              >
                {playlist.pasta_id === null ? "✓ " : ""}Fora de pasta
              </li>
            </>
          )}
          {!fixa && (
            <>
              <li role="separator" className="separador" />
              <li role="menuitem" className="perigo" onClick={excluir}>
                Excluir
              </li>
            </>
          )}
        </ul>
      )}
      {adicionando && (
        <AdicionarMusicas
          playlistId={playlist.id}
          nome={playlist.name}
          presentes={new Set(itens.map((i) => i.id))}
          onFechar={() => setAdicionando(false)}
          onErro={onErro}
        />
      )}

      {itens.length === 0 ? (
        <p className="vazio">
          {fixa
            ? "Nenhuma música curtida ainda. Clique no ♡ de uma música para curtir."
            : "Playlist vazia. Use “Adicionar músicas” ou, na biblioteca, clique com o botão direito numa música."}
        </p>
      ) : (
        <>
          <div className="linha linha-pl cabecalho" role="row">
            <span className="num">#</span>
            <span>Título</span>
            <span>Artista</span>
            <span>Álbum</span>
            <span className="duracao">Duração</span>
            <span />
          </div>
          <div ref={scrollRef} className="lista" role="table">
            <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
              {virtualizer.getVirtualItems().map((v) => {
                const m = itens[v.index];
                return (
                  <div
                    key={m.item_id}
                    role="row"
                    className={"linha linha-pl" + (m.missing ? " ausente" : "") + (m.id === tocandoId ? " atual" : "")}
                    title={m.missing ? "Arquivo não encontrado" : undefined}
                    style={{ height: ALTURA_LINHA, transform: `translateY(${v.start}px)` }}
                    {...tocarAo(() => tocar(v.index))}
                    onContextMenu={(e) => abrirMenu(e, m.id, { tocar: () => tocar(v.index) })}
                  >
                    <span className="num">{v.index + 1}</span>
                    <span>{m.title ?? "Sem título"}</span>
                    <span>{m.artist ?? "Artista desconhecido"}</span>
                    <span>{m.album ?? ""}</span>
                    <span className="duracao">{formatarDuracao(m.duration_ms)}</span>
                    <span
                      className="pl-item-acoes"
                      onClick={(e) => e.stopPropagation()}
                      onDoubleClick={(e) => e.stopPropagation()}
                    >
                      {personalizada && (
                        <>
                          <button
                            disabled={v.index === 0}
                            onClick={() => acao("playlist_move_item", { itemId: m.item_id, posicao: v.index - 1 })}
                            aria-label="Mover para cima"
                            title="Mover para cima"
                          >
                            ↑
                          </button>
                          <button
                            disabled={v.index === itens.length - 1}
                            onClick={() => acao("playlist_move_item", { itemId: m.item_id, posicao: v.index + 1 })}
                            aria-label="Mover para baixo"
                            title="Mover para baixo"
                          >
                            ↓
                          </button>
                        </>
                      )}
                      <button
                        onClick={() => acao("playlist_remove_item", { itemId: m.item_id })}
                        aria-label="Remover da playlist"
                        title="Remover da playlist (a música continua na biblioteca)"
                      >
                        ×
                      </button>
                    </span>
                  </div>
                );
              })}
            </div>
          </div>
        </>
      )}
    </section>
  );
}
