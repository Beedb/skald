import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import {
  AudioLines,
  BookOpen,
  ChartColumn,
  ChevronDown,
  Disc3,
  FileInput,
  Folder,
  FolderOpen,
  FolderPlus,
  Heart,
  History,
  House,
  Library,
  ListMusic,
  MicVocal,
  Music,
  PanelLeftClose,
  PanelLeftOpen,
  Pause,
  PencilLine,
  Play,
  Plus,
  Podcast,
  Settings,
  Sparkles,
  Tags,
} from "lucide-react";
import type { ChaveAlbum } from "./Colecao";
import { ativo, Capa, type Reproducao } from "./PlayerBar";
import { noAndroid } from "./android";
import EditorInteligente from "./EditorInteligente";

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
  /** Pasta da lateral (nenhuma = solta). */
  pasta_id: number | null;
  /** Músicas escolhidas por regras. */
  inteligente: boolean;
};

type PastaPlaylists = { id: number; nome: string };

export type Vista =
  | { tipo: "inicio" }
  | { tipo: "biblioteca" }
  | { tipo: "historico" }
  | { tipo: "estatisticas" }
  | { tipo: "completar"; aba?: "info" | "letras" | "categorias" }
  | { tipo: "configuracoes" }
  | { tipo: "artistas" }
  | { tipo: "artista"; nome: string | null }
  | { tipo: "albuns" }
  /** `de`: a vista de onde o álbum foi aberto (para o "Voltar"). */
  | { tipo: "album"; chave: ChaveAlbum; de: Vista }
  | { tipo: "playlist"; id: number }
  | { tipo: "categorias" }
  | { tipo: "categoria"; id: number }
  | { tipo: "podcasts" }
  | { tipo: "programa"; id: number }
  | { tipo: "audiolivros" }
  | { tipo: "livro"; id: number };

type Props = {
  vista: Vista;
  playlists: PlaylistResumo[];
  onVista: (v: Vista) => void;
  onErro: (mensagem: string) => void;
  onAviso: (mensagem: string) => void;
  r: Reproducao;
  onAbrirPlayer: () => void;
  /** Tela estreita: gaveta aberta por cima do conteúdo. */
  aberta?: boolean;
};

const CHAVE_RECOLHIDA = "skald-lateral-recolhida";
const CHAVE_GRUPOS = "skald-lateral-grupos";
const CHAVE_PASTAS_FECHADAS = "skald-pastas-fechadas";

function lerLocal<T>(chave: string, padrao: T): T {
  try {
    const v = localStorage.getItem(chave);
    return v === null ? padrao : (JSON.parse(v) as T);
  } catch {
    return padrao;
  }
}

function gravarLocal(chave: string, valor: unknown) {
  try {
    localStorage.setItem(chave, JSON.stringify(valor));
  } catch {
    /* sem armazenamento local: vale só nesta sessão */
  }
}

/** Música atual no rodapé da lateral; clicar abre o player completo. */
function TocandoAgora({ r, onAbrir }: { r: Reproducao; onAbrir: () => void }) {
  if (!ativo(r) || !r.musica) return null;
  const tocando = r.estado?.estado === "tocando";
  return (
    <div className="lateral-tocando">
      <button className="lateral-tocando-faixa" onClick={onAbrir} title="Abrir o player completo">
        <Capa src={r.capa} className="capa-lateral" />
        <span className="faixa">
          <strong>{r.musica.title ?? "Sem título"}</strong>
          <span>{r.musica.artist ?? "Artista desconhecido"}</span>
        </span>
      </button>
      <button
        className="lateral-tocando-play"
        onClick={() => invoke("toggle_pause").catch((e) => r.onErro(String(e)))}
        aria-label={tocando ? "Pausar" : "Tocar"}
        title={tocando ? "Pausar" : "Tocar"}
      >
        {tocando ? <Pause size={16} /> : <Play size={16} />}
      </button>
    </div>
  );
}

/** Barra lateral: marca, navegação, playlists e música atual. Pode ser recolhida a uma coluna de ícones. */
export default function Lateral({ vista, playlists, onVista, onErro, onAviso, r, onAbrirPlayer, aberta = false }: Props) {
  const [criando, setCriando] = useState(false);
  const [nome, setNome] = useState("");
  // Enter/Esc e o blur ao desmontar disparam juntos: só a primeira ação vale.
  const finalizado = useRef(false);

  // Recolhida ou não e grupos abertos: preferências locais desta tela (lembradas entre aberturas).
  const [recolhida, setRecolhida] = useState(() => lerLocal(CHAVE_RECOLHIDA, false));
  const [grupos, setGrupos] = useState(() => lerLocal(CHAVE_GRUPOS, { biblioteca: true, playlists: true }));

  // Pastas de playlists: lista, quais estão fechadas, menu (renomear/excluir) e nome sendo digitado.
  const [pastas, setPastas] = useState<PastaPlaylists[]>([]);
  const [fechadas, setFechadas] = useState<number[]>(() => lerLocal(CHAVE_PASTAS_FECHADAS, [] as number[]));
  const [menuPasta, setMenuPasta] = useState<{ id: number; x: number; y: number } | null>(null);
  const [editPasta, setEditPasta] = useState<{ id: number | null; nome: string } | null>(null);
  const [criandoInteligente, setCriandoInteligente] = useState(false);
  const pastaFinalizada = useRef(false);

  useEffect(() => {
    const carregar = () =>
      invoke<PastaPlaylists[]>("playlist_folders")
        .then(setPastas)
        .catch(() => {});
    carregar();
    const parar = listen("playlists-mudou", carregar);
    return () => void parar.then((f) => f());
  }, []);

  useEffect(() => {
    if (!menuPasta) return;
    const fechar = () => setMenuPasta(null);
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && fechar();
    window.addEventListener("click", fechar);
    window.addEventListener("keydown", tecla);
    window.addEventListener("scroll", fechar, true);
    return () => {
      window.removeEventListener("click", fechar);
      window.removeEventListener("keydown", tecla);
      window.removeEventListener("scroll", fechar, true);
    };
  }, [menuPasta]);

  function alternarPasta(id: number) {
    const novas = fechadas.includes(id) ? fechadas.filter((f) => f !== id) : [...fechadas, id];
    setFechadas(novas);
    gravarLocal(CHAVE_PASTAS_FECHADAS, novas);
  }

  function novaPasta() {
    pastaFinalizada.current = false;
    if (recolhida) alternar();
    if (!grupos.playlists) alternarGrupo("playlists");
    setEditPasta({ id: null, nome: "" });
  }

  async function concluirPasta() {
    if (pastaFinalizada.current || !editPasta) return;
    pastaFinalizada.current = true;
    const { id, nome: n } = editPasta;
    setEditPasta(null);
    if (!n.trim()) return;
    try {
      if (id === null) await invoke("playlist_folder_create", { nome: n });
      else await invoke("playlist_folder_rename", { id, nome: n });
    } catch (e) {
      onErro(String(e));
    }
  }

  async function excluirPasta(id: number) {
    const pasta = pastas.find((p) => p.id === id);
    const ok = await ask(`Excluir a pasta “${pasta?.nome ?? ""}”? As playlists dela continuam, fora de pasta.`, {
      title: "Excluir pasta",
      kind: "warning",
      okLabel: "Excluir",
      cancelLabel: "Cancelar",
    });
    if (!ok) return;
    invoke("playlist_folder_delete", { id }).catch((e) => onErro(String(e)));
  }

  function alternar() {
    const nova = !recolhida;
    setRecolhida(nova);
    gravarLocal(CHAVE_RECOLHIDA, nova);
  }

  function alternarGrupo(g: keyof typeof grupos) {
    const novos = { ...grupos, [g]: !grupos[g] };
    setGrupos(novos);
    gravarLocal(CHAVE_GRUPOS, novos);
  }

  function abrir() {
    finalizado.current = false;
    // Para digitar o nome, a barra precisa estar aberta e o grupo de playlists visível.
    if (recolhida) alternar();
    if (!grupos.playlists) alternarGrupo("playlists");
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

  /** Cria uma playlist a partir de um M3U com as músicas que já estão na biblioteca. */
  async function importarM3u() {
    const origem = await open({
      title: "Escolha a playlist (M3U)",
      multiple: false,
      // No Android o filtro por extensão vira tipo de arquivo e esconde os .m3u.
      filters: noAndroid ? undefined : [{ name: "Playlist M3U", extensions: ["m3u", "m3u8"] }],
    });
    if (typeof origem !== "string") return;
    try {
      const r = await invoke<{ id: number; nome: string; achadas: number; faltando: string[] }>("playlist_import_m3u", { origem });
      const falta = r.faltando.length
        ? ` ${r.faltando.length} não ${r.faltando.length === 1 ? "está" : "estão"} na biblioteca: ${r.faltando.slice(0, 5).join(", ")}${r.faltando.length > 5 ? "…" : ""}.`
        : "";
      onAviso(`Playlist “${r.nome}” criada com ${r.achadas} ${r.achadas === 1 ? "música" : "músicas"}.${falta}`);
      onVista({ tipo: "playlist", id: r.id });
    } catch (e) {
      onErro(String(e));
    }
  }

  const item = (
    ativo: boolean,
    icone: ReactNode,
    rotulo: string,
    onClick: () => void,
    extra?: { qtd?: number; dica?: string; classe?: string; desativado?: boolean },
  ) => (
    <button
      className={"item-lateral" + (ativo ? " ativo" : "") + (extra?.classe ? " " + extra.classe : "")}
      onClick={onClick}
      disabled={extra?.desativado}
      title={recolhida ? rotulo + (extra?.qtd !== undefined ? ` · ${extra.qtd}` : "") : extra?.dica}
      aria-label={recolhida ? rotulo : undefined}
    >
      <span className="icone" aria-hidden="true">
        {icone}
      </span>
      <span className="nome">{rotulo}</span>
      {extra?.qtd !== undefined && <span className="qtd">{extra.qtd}</span>}
    </button>
  );

  const grupo = (g: keyof typeof grupos, ativo: boolean, icone: ReactNode, rotulo: string, onClick: () => void) => (
    <div className="grupo-lateral">
      {item(ativo, icone, rotulo, onClick)}
      <button
        className={"abrir-grupo" + (grupos[g] ? " aberto" : "")}
        onClick={() => alternarGrupo(g)}
        aria-expanded={grupos[g]}
        aria-label={grupos[g] ? `Recolher ${rotulo}` : `Expandir ${rotulo}`}
        title={grupos[g] ? `Recolher ${rotulo}` : `Expandir ${rotulo}`}
      >
        <ChevronDown size={14} />
      </button>
    </div>
  );

  const naPlaylist = (id: number) => vista.tipo === "playlist" && vista.id === id;

  const linhaPlaylist = (p: PlaylistResumo) => (
    <li key={p.id}>
      {item(
        naPlaylist(p.id),
        p.inteligente ? <Sparkles size={16} /> : <span className="inicial">{(p.name.trim()[0] ?? "♪").toUpperCase()}</span>,
        p.name,
        () => onVista({ tipo: "playlist", id: p.id }),
        { qtd: p.total, dica: `${p.name} · ${p.total} ${p.total === 1 ? "música" : "músicas"}` },
      )}
    </li>
  );

  const entradaPasta = () => (
    <input
      className="nova-playlist"
      autoFocus
      placeholder="Nome da pasta"
      aria-label="Nome da pasta"
      maxLength={200}
      value={editPasta?.nome ?? ""}
      onChange={(e) => setEditPasta((p) => (p ? { ...p, nome: e.target.value } : p))}
      onBlur={concluirPasta}
      onKeyDown={(e) => {
        if (e.key === "Enter") concluirPasta();
        if (e.key === "Escape") {
          e.stopPropagation();
          pastaFinalizada.current = true;
          setEditPasta(null);
        }
      }}
    />
  );
  const tam = 18;

  return (
    <nav className={"lateral" + (recolhida ? " recolhida" : "") + (aberta ? " aberta" : "")} aria-label="Navegação">
      <div className="marca">
        <span className="marca-nome">{recolhida ? "S" : "Skáld"}</span>
        <button
          className="alternar-lateral"
          onClick={alternar}
          aria-expanded={!recolhida}
          title={recolhida ? "Expandir barra lateral" : "Recolher barra lateral"}
        >
          {recolhida ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
        </button>
      </div>

      <div className="lateral-rolagem">
        {item(vista.tipo === "inicio", <House size={tam} />, "Início", () => onVista({ tipo: "inicio" }))}
        {grupo("biblioteca", false, <Library size={tam} />, "Biblioteca", () => onVista({ tipo: "biblioteca" }))}
        {grupos.biblioteca && (
          <div className="subitens">
            {item(vista.tipo === "biblioteca", <Music size={16} />, "Músicas", () => onVista({ tipo: "biblioteca" }))}
            {item(vista.tipo === "podcasts" || vista.tipo === "programa", <Podcast size={16} />, "Podcasts", () =>
              onVista({ tipo: "podcasts" }),
            )}
            {item(vista.tipo === "audiolivros" || vista.tipo === "livro", <BookOpen size={16} />, "Audiolivros", () =>
              onVista({ tipo: "audiolivros" }),
            )}
            {item(false, <AudioLines size={16} />, "Outros áudios", () => {}, { desativado: true, dica: "Em breve" })}
          </div>
        )}
        {item(vista.tipo === "albuns" || vista.tipo === "album", <Disc3 size={tam} />, "Álbuns", () =>
          onVista({ tipo: "albuns" }),
        )}
        {item(vista.tipo === "artistas" || vista.tipo === "artista", <MicVocal size={tam} />, "Artistas", () =>
          onVista({ tipo: "artistas" }),
        )}
        {item(vista.tipo === "categorias" || vista.tipo === "categoria", <Tags size={tam} />, "Categorias", () =>
          onVista({ tipo: "categorias" }),
        )}
        {item(vista.tipo === "historico", <History size={tam} />, "Histórico", () => onVista({ tipo: "historico" }))}
        {item(vista.tipo === "estatisticas", <ChartColumn size={tam} />, "Estatísticas", () =>
          onVista({ tipo: "estatisticas" }),
        )}
        {item(vista.tipo === "completar", <PencilLine size={tam} />, "Completar informações", () =>
          onVista({ tipo: "completar" }),
        )}

        <div className="divisor-lateral" />
        {grupo("playlists", false, <ListMusic size={tam} />, "Playlists", () => alternarGrupo("playlists"))}
        {grupos.playlists && (
          <div className="subitens">
            {playlists
              .filter((p) => p.kind === "curtidas")
              .map((p) => (
                <div key={p.id}>
                  {item(naPlaylist(p.id), <Heart size={16} />, p.name, () => onVista({ tipo: "playlist", id: p.id }), {
                    qtd: p.total,
                  })}
                </div>
              ))}
            {pastas.map((pa) => {
              const dentro = playlists.filter((p) => p.kind === "user" && p.pasta_id === pa.id);
              const aberta = !fechadas.includes(pa.id);
              return (
                <div key={pa.id} className="pasta-lateral">
                  {editPasta?.id === pa.id ? (
                    entradaPasta()
                  ) : (
                    <div
                      onContextMenu={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        setMenuPasta({ id: pa.id, x: Math.min(e.clientX, window.innerWidth - 190), y: e.clientY });
                      }}
                    >
                      {item(
                        false,
                        aberta ? <FolderOpen size={16} /> : <Folder size={16} />,
                        pa.nome,
                        () => alternarPasta(pa.id),
                        {
                          qtd: dentro.length,
                          dica: `${pa.nome} · ${dentro.length} ${dentro.length === 1 ? "playlist" : "playlists"} (botão direito: renomear ou excluir)`,
                        },
                      )}
                    </div>
                  )}
                  {aberta && dentro.length > 0 && <ul className="dentro-pasta">{dentro.map(linhaPlaylist)}</ul>}
                </div>
              );
            })}
            <ul>{playlists.filter((p) => p.kind === "user" && (p.pasta_id === null || !pastas.some((pa) => pa.id === p.pasta_id))).map(linhaPlaylist)}</ul>
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
              item(false, <Plus size={16} />, "Nova playlist", abrir, { classe: "criar" })
            )}
            {editPasta?.id === null
              ? entradaPasta()
              : !criando && item(false, <FolderPlus size={16} />, "Nova pasta", novaPasta, { classe: "criar" })}
            {!criando &&
              !editPasta &&
              item(false, <Sparkles size={16} />, "Nova playlist inteligente", () => setCriandoInteligente(true), { classe: "criar" })}
            {!criando && !editPasta && item(false, <FileInput size={16} />, "Importar M3U…", importarM3u, { classe: "criar" })}
            {/* No body: no celular a lateral é uma gaveta com transform, que prenderia o modal. */}
            {criandoInteligente &&
              createPortal(
                <EditorInteligente
                  id={null}
                  onFechar={() => setCriandoInteligente(false)}
                  onSalvo={(id) => {
                    setCriandoInteligente(false);
                    onVista({ tipo: "playlist", id });
                  }}
                  onErro={onErro}
                />,
                document.body,
              )}
            {menuPasta && (
              <ul className="menu-contexto" role="menu" style={{ left: menuPasta.x, top: menuPasta.y }}>
                <li
                  role="menuitem"
                  onClick={() => {
                    pastaFinalizada.current = false;
                    setEditPasta({ id: menuPasta.id, nome: pastas.find((p) => p.id === menuPasta.id)?.nome ?? "" });
                  }}
                >
                  Renomear pasta
                </li>
                <li role="menuitem" className="perigo" onClick={() => excluirPasta(menuPasta.id)}>
                  Excluir pasta
                </li>
              </ul>
            )}
          </div>
        )}
      </div>

      <div className="lateral-base">
        {item(vista.tipo === "configuracoes", <Settings size={tam} />, "Configurações", () =>
          onVista({ tipo: "configuracoes" }),
        )}
        <TocandoAgora r={r} onAbrir={onAbrirPlayer} />
      </div>
    </nav>
  );
}
