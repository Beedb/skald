import { useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AudioLines,
  BookOpen,
  ChartColumn,
  ChevronDown,
  Disc3,
  FileInput,
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
  Tags,
} from "lucide-react";
import type { ChaveAlbum } from "./Colecao";
import { ativo, Capa, type Reproducao } from "./PlayerBar";
import { noAndroid } from "./android";

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
  | { tipo: "programa"; id: number };

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
            {item(false, <BookOpen size={16} />, "Audiolivros", () => {}, { desativado: true, dica: "Em breve" })}
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
            <ul>
              {playlists
                .filter((p) => p.kind === "user")
                .map((p) => (
                  <li key={p.id}>
                    {item(
                      naPlaylist(p.id),
                      <span className="inicial">{(p.name.trim()[0] ?? "♪").toUpperCase()}</span>,
                      p.name,
                      () => onVista({ tipo: "playlist", id: p.id }),
                      { qtd: p.total, dica: `${p.name} · ${p.total} ${p.total === 1 ? "música" : "músicas"}` },
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
              item(false, <Plus size={16} />, "Nova playlist", abrir, { classe: "criar" })
            )}
            {!criando && item(false, <FileInput size={16} />, "Importar M3U…", importarM3u, { classe: "criar" })}
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
