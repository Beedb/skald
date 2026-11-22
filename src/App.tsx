import { useEffect, useRef, useState } from "react";
import { FilePlus, FolderPlus, Menu, Search } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import Biblioteca from "./Biblioteca";
import PlayerBar, { type EstadoPlayer, type Reproducao } from "./PlayerBar";
import PlayerCompleto, { type Detalhes } from "./PlayerCompleto";
import Fila, { type VisaoFila } from "./Fila";
import Lateral, { type PlaylistResumo, type Vista } from "./Lateral";
import PlaylistView from "./PlaylistView";
import Historico from "./Historico";
import AvisoVersao from "./AvisoVersao";
import Pastas from "./Pastas";
import Duplicados from "./Duplicados";
import { Albuns, AlbumView, Artistas, ArtistaView } from "./Colecao";
import Inicio from "./Inicio";
import Podcasts, { ProgramaView } from "./Podcasts";
import Audiolivros, { LivroView } from "./Audiolivros";
import { esquecerMiniatura, miniatura } from "./Capa";
import Informacoes from "./Informacoes";
import EdicaoLote from "./EdicaoLote";
import Completar from "./Completar";
import { MenuMusicaProvider } from "./MenuMusica";
import { CategoriaView, Categorias } from "./CategoriasMusica";
import { aplicarCorCapa, corDaImagem, reaplicarCorCapa } from "./coresCapa";
import { aplicarTema, ModoTema } from "./tema";
import Estatisticas from "./Estatisticas";
import Configuracoes from "./Configuracoes";
import { escolherPasta, garantirPermissao, noAndroid } from "./android";
import "./App.css";

type DbStatus = {
  path: string;
  schema_version: number;
  media_count: number;
};

type ImportResumo = {
  encontrados: number;
  adicionados: number;
  existentes: number;
  falhas: string[];
  reencontrados: number;
  ausentes: number;
  movidos: number;
  duplicados: number;
};

/** Frase curta do que o monitoramento de pastas mudou. */
function avisoMonitoramento(r: ImportResumo) {
  const partes = [];
  const n = (q: number, um: string, varios: string) => `${q} ${q === 1 ? um : varios}`;
  if (r.adicionados) partes.push(n(r.adicionados, "música nova", "músicas novas"));
  if (r.reencontrados) partes.push(n(r.reencontrados, "reencontrada", "reencontradas"));
  if (r.movidos) partes.push(n(r.movidos, "renomeada ou movida reconhecida", "renomeadas ou movidas reconhecidas"));
  if (r.ausentes) partes.push(n(r.ausentes, "não encontrada", "não encontradas"));
  if (r.duplicados)
    partes.push(n(r.duplicados, "grupo de arquivos duplicados", "grupos de arquivos duplicados") + ' (veja "Duplicados")');
  return "Biblioteca: " + partes.join(", ") + ".";
}

function App() {
  const [status, setStatus] = useState<DbStatus | null>(null);
  const [erro, setErro] = useState<string | null>(null);

  const [importando, setImportando] = useState(false);
  const [resumo, setResumo] = useState<ImportResumo | null>(null);
  const [versao, setVersao] = useState(0);

  const carregarStatus = () =>
    invoke<DbStatus>("db_status")
      .then(setStatus)
      .catch((e) => setErro(String(e)));

  const [estado, setEstado] = useState<EstadoPlayer | null>(null);
  const [fila, setFila] = useState<VisaoFila | null>(null);
  const [filaAberta, setFilaAberta] = useState(false);
  // Tela estreita (celular): a lateral vira gaveta, aberta pelo botão ☰ do topo.
  const [gaveta, setGaveta] = useState(false);
  const [tela, setTela] = useState<"biblioteca" | "player">("biblioteca");
  const [volume, setVolume] = useState(1);
  const [detalhes, setDetalhes] = useState<Detalhes | null>(null);
  const [vista, setVista] = useState<Vista>({ tipo: "inicio" });
  const [coresDinamicas, setCoresDinamicas] = useState(false);
  const [playlists, setPlaylists] = useState<PlaylistResumo[]>([]);
  const [versaoPlaylists, setVersaoPlaylists] = useState(0);
  const [curtidas, setCurtidas] = useState<Set<number>>(new Set());
  const [versaoHistorico, setVersaoHistorico] = useState(0);
  const [versaoPodcasts, setVersaoPodcasts] = useState(0);
  const [versaoLivros, setVersaoLivros] = useState(0);
  const [podcast, setPodcast] = useState<Reproducao["podcast"]>(null);
  // Aviso deixado antes de recarregar (ex.: backup restaurado).
  const [aviso, setAviso] = useState<string | null>(() => {
    try {
      const a = sessionStorage.getItem("skald-aviso");
      sessionStorage.removeItem("skald-aviso");
      return a;
    } catch {
      return null;
    }
  });
  const [informacoesId, setInformacoesId] = useState<number | null>(null);
  const [loteIds, setLoteIds] = useState<number[] | null>(null);

  // Busca: `busca` é o texto digitado; `buscaAtiva` é aplicada 200 ms depois da última tecla.
  const [busca, setBusca] = useState("");
  const [buscaAtiva, setBuscaAtiva] = useState("");
  const [total, setTotal] = useState(0);
  const campoBusca = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const t = setTimeout(() => setBuscaAtiva(busca.trim()), 200);
    return () => clearTimeout(t);
  }, [busca]);

  const mediaCount = status?.media_count;
  useEffect(() => {
    if (mediaCount === undefined) return;
    let valido = true;
    invoke<number>("count_media", { busca: buscaAtiva })
      .then((n) => valido && setTotal(n))
      .catch((e) => setErro(String(e)));
    return () => {
      valido = false;
    };
  }, [buscaAtiva, versao, mediaCount]);

  // Ctrl+F leva ao campo de busca.
  useEffect(() => {
    const tecla = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === "f") {
        e.preventDefault();
        setTela("biblioteca");
        setVista({ tipo: "biblioteca" });
        campoBusca.current?.focus();
        campoBusca.current?.select();
      }
    };
    window.addEventListener("keydown", tecla);
    return () => window.removeEventListener("keydown", tecla);
  }, []);

  const mostrarErro = (mensagem: string) => {
    setErro(mensagem);
    // O arquivo pode ter sido marcado como não encontrado: redesenha a lista.
    setVersao((v) => v + 1);
  };

  useEffect(() => {
    carregarStatus();
    const carregarFila = () =>
      invoke<VisaoFila>("get_queue")
        .then(setFila)
        .catch((e) => setErro(String(e)));
    carregarFila();
    // Música retomada ao abrir (pausada): o aviso dela pode ter saído antes de a tela ouvir.
    invoke<EstadoPlayer>("player_state")
      .then((e) => setEstado((atual) => atual ?? e))
      .catch(() => {});
    const carregarPlaylists = () =>
      invoke<PlaylistResumo[]>("playlists_list")
        .then(setPlaylists)
        .catch((e) => setErro(String(e)));
    const carregarCurtidas = () =>
      invoke<number[]>("liked_ids")
        .then((ids) => setCurtidas(new Set(ids)))
        .catch((e) => setErro(String(e)));
    carregarPlaylists();
    carregarCurtidas();
    // O tema salvo no banco vale sobre a cópia local usada no primeiro desenho.
    invoke<{ tema: string; modo_tema: ModoTema; cores_dinamicas: boolean }>("get_settings")
      .then((c) => {
        aplicarTema(c.tema, c.modo_tema);
        setCoresDinamicas(c.cores_dinamicas);
      })
      .catch(() => {});
    const ouvintes = [
      listen<EstadoPlayer>("player-estado", (e) => {
        setEstado(e.payload);
        if (e.payload.erro) setErro(e.payload.erro);
      }),
      listen("fila-mudou", carregarFila),
      listen("playlists-mudou", () => {
        carregarPlaylists();
        carregarCurtidas();
        setVersaoPlaylists((v) => v + 1);
      }),
      listen<string>("player-erro", (e) => mostrarErro(e.payload)),
      listen("historico-mudou", () => setVersaoHistorico((v) => v + 1)),
      listen("podcasts-mudou", () => setVersaoPodcasts((v) => v + 1)),
      listen("audiolivros-mudou", () => setVersaoLivros((v) => v + 1)),
      listen<ImportResumo>("biblioteca-mudou", (e) => {
        setAviso(avisoMonitoramento(e.payload));
        carregarStatus();
        setVersao((v) => v + 1);
        setVersaoPlaylists((v) => v + 1);
        setVersaoPodcasts((v) => v + 1);
      }),
    ];
    return () => {
      ouvintes.forEach((o) => o.then((parar) => parar()));
    };
  }, []);

  // Cores dinâmicas (seção 3.2): acento tirado da miniatura da capa atual.
  useEffect(() => {
    const ligar = (e: Event) => setCoresDinamicas((e as CustomEvent<boolean>).detail);
    window.addEventListener("cores-dinamicas", ligar);
    window.addEventListener("tema-mudou", reaplicarCorCapa);
    return () => {
      window.removeEventListener("cores-dinamicas", ligar);
      window.removeEventListener("tema-mudou", reaplicarCorCapa);
    };
  }, []);
  const idCor = fila?.atual?.id ?? null;
  useEffect(() => {
    if (!coresDinamicas || idCor === null) return aplicarCorCapa(null);
    let valido = true;
    miniatura(idCor)
      .then((url) => (url ? corDaImagem(url) : null))
      .then((cor) => valido && aplicarCorCapa(cor));
    return () => {
      valido = false;
    };
  }, [idCor, coresDinamicas]);

  // Capa e informações da música atual (lidas do arquivo só quando ela muda).
  const idAtual = fila?.atual?.id ?? null;
  // Episódio de podcast? (o player mostra −15/+30 s e a velocidade)
  useEffect(() => {
    if (idAtual === null) return setPodcast(null);
    let valido = true;
    invoke<Reproducao["podcast"]>("podcast_info", { mediaId: idAtual })
      .then((p) => valido && setPodcast(p))
      .catch(() => valido && setPodcast(null));
    return () => {
      valido = false;
    };
  }, [idAtual]);
  useEffect(() => {
    if (idAtual === null) return setDetalhes(null);
    let valido = true;
    invoke<Detalhes | null>("get_track_details", { id: idAtual })
      .then((d) => valido && setDetalhes(d))
      .catch(() => valido && setDetalhes(null));
    return () => {
      valido = false;
    };
  }, [idAtual]);

  // Esc fecha o player completo.
  useEffect(() => {
    if (tela !== "player") return;
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && setTela("biblioteca");
    window.addEventListener("keydown", tecla);
    return () => window.removeEventListener("keydown", tecla);
  }, [tela]);

  const r: Reproducao = {
    musica: fila?.atual ?? null,
    estado,
    capa: detalhes && detalhes.id === idAtual ? detalhes.capa : null,
    volume,
    onVolume: (v) => {
      setVolume(v);
      invoke("set_volume", { volume: v });
    },
    onErro: mostrarErro,
    curtida: idAtual !== null && curtidas.has(idAtual),
    onCurtir: () => idAtual !== null && curtir(idAtual),
    aleatorio: fila?.aleatorio ?? false,
    repetir: fila?.repetir ?? "desligado",
    podcast,
    onVelocidade: (v) =>
      invoke<number>("set_podcast_speed", { velocidade: v })
        .then((real) => setPodcast((p) => (p ? { ...p, velocidade: real } : p)))
        .catch((e) => mostrarErro(String(e))),
  };

  async function tocar(id: number) {
    setErro(null);
    try {
      await invoke("play_media", { id, busca: buscaAtiva });
    } catch (e) {
      mostrarErro(String(e));
    }
  }

  // Logo após criar, a playlist pode ainda não ter chegado na lista: a área fica vazia por um instante.
  // Excluir só acontece pela própria tela da playlist, que volta para a biblioteca.
  const playlistAtual = vista.tipo === "playlist" ? playlists.find((p) => p.id === vista.id) : undefined;

  function curtir(mediaId: number) {
    invoke("toggle_like", { mediaId }).catch((e) => setErro(String(e)));
  }

  function adicionarPlaylist(playlistId: number, mediaId: number) {
    invoke("playlist_add", { id: playlistId, mediaIds: [mediaId] }).catch((e) => setErro(String(e)));
  }

  async function novaPlaylistCom(mediaId: number) {
    try {
      const id = await invoke<number>("playlist_create", { name: "Nova playlist", mediaIds: [mediaId] });
      setVista({ tipo: "playlist", id });
    } catch (e) {
      setErro(String(e));
    }
  }

  async function removerDaBiblioteca(id: number) {
    try {
      await invoke("remove_from_library", { ids: [id] });
      setAviso("Música removida da biblioteca. O arquivo continua no disco.");
      carregarStatus();
      setVersao((v) => v + 1);
    } catch (e) {
      setErro(String(e));
    }
  }

  function enfileirar(id: number, aSeguir: boolean) {
    invoke("queue_add", { id, aSeguir }).catch((e) => setErro(String(e)));
  }

  async function importarPasta() {
    if (noAndroid) return importarMusicaAndroid();
    const pasta = await open({ directory: true, title: "Escolha a pasta de músicas" });
    if (typeof pasta !== "string") return;
    await importar("import_folder", { path: pasta });
  }

  /** Android: pede a permissão, abre o seletor de pastas e importa (e passa a monitorar) a escolhida. */
  async function importarMusicaAndroid() {
    const acesso = await garantirPermissao();
    if (!acesso) {
      setErro("Sem permissão para ler as músicas. Libere em Configurações do Android → Apps → Skáld → Permissões.");
      return;
    }
    const pasta = await escolherPasta();
    if (pasta === null) return;
    await importar("import_folder", { path: pasta });
    if (acesso === "audio") {
      setAviso('Sem "acesso a todos os arquivos", as letras .lrc ao lado das músicas não são lidas. Importe de novo para liberar.');
    }
  }

  /** Arquivos soltos: entram na biblioteca sem monitorar a pasta deles. */
  async function importarArquivos() {
    const arquivos = await open({
      multiple: true,
      title: "Escolha os arquivos de música",
      filters: [{ name: "Áudio", extensions: ["mp3", "flac", "ogg", "opus", "oga", "m4a", "m4b", "aac", "wav"] }],
    });
    if (!arquivos || arquivos.length === 0) return;
    await importar("import_files", { paths: arquivos });
  }

  async function importar(comando: string, args: Record<string, unknown>) {
    setImportando(true);
    setResumo(null);
    setErro(null);
    try {
      setResumo(await invoke<ImportResumo>(comando, args));
      await carregarStatus();
      setVersao((v) => v + 1);
    } catch (e) {
      setErro(String(e));
    } finally {
      setImportando(false);
    }
  }

  return (
    <MenuMusicaProvider
      playlists={playlists}
      onTocar={(id) => invoke("play_media", { id, busca: "" }).catch((e) => mostrarErro(String(e)))}
      onEnfileirar={enfileirar}
      onAdicionarPlaylist={adicionarPlaylist}
      onNovaPlaylistCom={novaPlaylistCom}
      onRemover={removerDaBiblioteca}
      onInformacoes={setInformacoesId}
      onEditarLote={setLoteIds}
      onErro={mostrarErro}
    >
    <main className="app">
      <div className="conteudo" hidden={tela === "player"}>
        <Lateral
          vista={vista}
          playlists={playlists}
          onVista={(v) => {
            setVista(v);
            setTela("biblioteca");
            setGaveta(false);
          }}
          onErro={setErro}
          onAviso={setAviso}
          r={r}
          onAbrirPlayer={() => {
            setTela("player");
            setGaveta(false);
          }}
          aberta={gaveta}
        />
        {gaveta && <div className="gaveta-fundo" onClick={() => setGaveta(false)} aria-hidden="true" />}
        <div className="principal">
          <header className="topo">
            <button className="icone-botao botao-gaveta" onClick={() => setGaveta(true)} aria-label="Abrir menu" title="Menu">
              <Menu size={20} aria-hidden="true" />
            </button>
            <label className="busca-caixa">
              <Search size={16} aria-hidden="true" />
              <input
                ref={campoBusca}
                className="busca"
                type="search"
                placeholder="Buscar músicas, artistas, álbuns… (Ctrl+F)"
                aria-label="Buscar"
                value={busca}
                onChange={(e) => {
                  setBusca(e.target.value);
                  setTela("biblioteca");
                  setVista({ tipo: "biblioteca" });
                }}
                onKeyDown={(e) => {
                  if (e.key === "Escape") {
                    e.stopPropagation();
                    setBusca("");
                  }
                }}
              />
            </label>
            <div className="topo-acoes">
              {status && (
                <span className="contagem" title={`Banco: ${status.path} (esquema v${status.schema_version})`}>
                  {buscaAtiva ? `${total} de ` : ""}
                  {status.media_count} {status.media_count === 1 ? "música" : "músicas"}
                </span>
              )}
              <button onClick={importarPasta} disabled={importando}>
                <FolderPlus size={16} aria-hidden="true" />
                {importando ? "Importando…" : "Importar pasta"}
              </button>
              {!noAndroid && (
              <button onClick={importarArquivos} disabled={importando} title="Adiciona arquivos soltos, sem monitorar a pasta deles">
                <FilePlus size={16} aria-hidden="true" />
                Arquivos
              </button>
              )}
              <Pastas versao={versao} onErro={setErro} />
              <Duplicados versao={versao} onErro={setErro} />
            </div>
          </header>
          <AvisoVersao />
          {aviso && (
            <p className="aviso">
              {aviso}{" "}
              <button className="fechar-aviso" onClick={() => setAviso(null)} aria-label="Fechar aviso" title="Fechar aviso">
                ✕
              </button>
            </p>
          )}
          {erro && <p className="erro">Erro: {erro}</p>}
          {!erro && !status && <p>Conectando ao banco de dados…</p>}
          {resumo && (
            <div className="resumo">
              <p>
                {resumo.encontrados} arquivo(s) de áudio encontrado(s): {resumo.adicionados} adicionado(s),{" "}
                {resumo.existentes} já estava(m) na biblioteca
                {resumo.movidos > 0 && `, ${resumo.movidos} renomeada(s) ou movida(s) reconhecida(s)`}
                {resumo.duplicados > 0 && `, ${resumo.duplicados} grupo(s) de duplicados (veja "Duplicados")`}.
              </p>
              {resumo.falhas.length > 0 && (
                <details>
                  <summary>{resumo.falhas.length} arquivo(s) não puderam ser lidos</summary>
                  <ul>
                    {resumo.falhas.map((f) => (
                      <li key={f}>{f}</li>
                    ))}
                  </ul>
                </details>
              )}
            </div>
          )}
          <div className="area" hidden={vista.tipo !== "biblioteca"}>
            {status && (
              <Biblioteca
                total={total}
                versao={versao}
                busca={buscaAtiva}
                tocandoId={estado && estado.estado !== "parado" ? estado.id : null}
                onTocar={tocar}
                curtidas={curtidas}
                onCurtir={curtir}
                visivel={tela === "biblioteca" && vista.tipo === "biblioteca"}
              />
            )}
          </div>
          {vista.tipo === "completar" && (
            <div className="area">
              <Completar
                aba={vista.aba}
                versao={versao}
                onMudou={(mensagem) => {
                  setAviso(mensagem);
                  setVersao((v) => v + 1);
                }}
                onErro={setErro}
              />
            </div>
          )}
          {vista.tipo === "historico" && (
            <div className="area">
              <Historico
                versao={versaoHistorico}
                visivel={tela === "biblioteca"}
                onTocar={(id) => invoke("play_media", { id, busca: "" }).catch((e) => mostrarErro(String(e)))}
              />
            </div>
          )}
          {vista.tipo === "estatisticas" && (
            <div className="area">
              <Estatisticas
                versao={versaoHistorico}
                onErro={setErro}
                onTocar={(id) => invoke("play_media", { id, busca: "" }).catch((e) => mostrarErro(String(e)))}
              />
            </div>
          )}
          {vista.tipo === "podcasts" && (
            <div className="area">
              <Podcasts versao={versaoPodcasts} onAbrir={(id) => setVista({ tipo: "programa", id })} onErro={setErro} />
            </div>
          )}
          {vista.tipo === "audiolivros" && (
            <div className="area">
              <Audiolivros versao={versaoLivros} onAbrir={(id) => setVista({ tipo: "livro", id })} onErro={setErro} />
            </div>
          )}
          {vista.tipo === "livro" && (
            <div className="area">
              <LivroView
                id={vista.id}
                versao={versaoLivros + versaoHistorico}
                idAtual={idAtual}
                onVoltar={() => setVista({ tipo: "audiolivros" })}
                onErro={setErro}
              />
            </div>
          )}
          {vista.tipo === "programa" && (
            <div className="area">
              <ProgramaView
                id={vista.id}
                versao={versaoPodcasts + versaoHistorico}
                idAtual={idAtual}
                onVoltar={() => setVista({ tipo: "podcasts" })}
                onErro={setErro}
              />
            </div>
          )}
          {vista.tipo === "inicio" && (
            <div className="area">
              <Inicio
                versao={versao + versaoHistorico + versaoPodcasts + versaoLivros}
                onPrograma={(id) => setVista({ tipo: "programa", id })}
                onLivro={(id) => setVista({ tipo: "livro", id })}
                playlists={playlists}
                totalMusicas={status?.media_count ?? 0}
                onPlaylist={(id) => setVista({ tipo: "playlist", id })}
                onAlbum={(chave) => setVista({ tipo: "album", chave, de: vista })}
                onBiblioteca={() => setVista({ tipo: "biblioteca" })}
                onAlbuns={() => setVista({ tipo: "albuns" })}
                onHistorico={() => setVista({ tipo: "historico" })}
                onTocarMusica={(id) => invoke("play_media", { id, busca: "" }).catch((e) => mostrarErro(String(e)))}
                onErro={setErro}
              />
            </div>
          )}
          {vista.tipo === "artistas" && (
            <div className="area">
              <Artistas
                versao={versao}
                onAbrir={(nome) => setVista({ tipo: "artista", nome })}
                onMudou={() => setVersao((v) => v + 1)}
                onErro={setErro}
              />
            </div>
          )}
          {vista.tipo === "categorias" && (
            <div className="area">
              <Categorias
                versao={versao}
                onAbrir={(id) => setVista({ tipo: "categoria", id })}
                onCompletar={() => setVista({ tipo: "completar", aba: "categorias" })}
                onErro={setErro}
              />
            </div>
          )}
          {vista.tipo === "categoria" && (
            <div className="area">
              <CategoriaView
                id={vista.id}
                versao={versao}
                tocandoId={estado && estado.estado !== "parado" ? estado.id : null}
                onVoltar={() => setVista({ tipo: "categorias" })}
                onErro={setErro}
              />
            </div>
          )}
          {vista.tipo === "artista" && (
            <div className="area">
              <ArtistaView
                nome={vista.nome}
                versao={versao}
                onAbrirAlbum={(chave) => setVista({ tipo: "album", chave, de: vista })}
                onVoltar={() => setVista({ tipo: "artistas" })}
                onErro={setErro}
              />
            </div>
          )}
          {vista.tipo === "albuns" && (
            <div className="area">
              <Albuns versao={versao} onAbrir={(chave) => setVista({ tipo: "album", chave, de: vista })} onErro={setErro} />
            </div>
          )}
          {vista.tipo === "album" && (
            <div className="area">
              <AlbumView
                chave={vista.chave}
                versao={versao}
                tocandoId={estado && estado.estado !== "parado" ? estado.id : null}
                onArtista={(nome) => setVista({ tipo: "artista", nome })}
                onEditar={setLoteIds}
                onVoltar={() => setVista(vista.de)}
                onErro={setErro}
              />
            </div>
          )}
          {vista.tipo === "configuracoes" && (
            <div className="area">
              <Configuracoes onErro={setErro} />
            </div>
          )}
          {playlistAtual && (
            <div className="area">
              <PlaylistView
                playlist={playlistAtual}
                versao={versaoPlaylists}
                tocandoId={estado && estado.estado !== "parado" ? estado.id : null}
                onExcluida={() => setVista({ tipo: "biblioteca" })}
                onErro={setErro}
              />
            </div>
          )}
        </div>
        {filaAberta && <Fila fila={fila} onErro={setErro} />}
      </div>
      {tela === "player" ? (
        <PlayerCompleto r={r} fila={fila} detalhes={detalhes} onVoltar={() => setTela("biblioteca")} />
      ) : (
        <PlayerBar
          r={r}
          filaAberta={filaAberta}
          onAlternarFila={() => setFilaAberta((a) => !a)}
          onAbrirPlayer={() => setTela("player")}
        />
      )}
      {loteIds && (
        <EdicaoLote
          ids={loteIds}
          onFechar={() => setLoteIds(null)}
          onSalvo={(mensagem) => {
            setLoteIds(null);
            setAviso(mensagem);
            setVersao((v) => v + 1);
          }}
          onErro={setErro}
        />
      )}
      {informacoesId !== null && (
        <Informacoes
          id={informacoesId}
          onFechar={() => setInformacoesId(null)}
          onSalvo={() => {
            esquecerMiniatura(informacoesId);
            setInformacoesId(null);
            setAviso("Informações salvas.");
            carregarStatus();
            setVersao((v) => v + 1);
          }}
          onErro={setErro}
        />
      )}
    </main>
    </MenuMusicaProvider>
  );
}

export default App;
