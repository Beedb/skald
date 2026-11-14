import { useEffect, useRef, useState } from "react";
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
import Pastas from "./Pastas";
import Duplicados from "./Duplicados";
import Categorias from "./Categorias";
import { Albuns, AlbumView, Artistas, ArtistaView } from "./Colecao";
import Inicio from "./Inicio";
import { esquecerMiniatura, miniatura } from "./Capa";
import Informacoes from "./Informacoes";
import EdicaoLote from "./EdicaoLote";
import Completar from "./Completar";
import { MenuMusicaProvider } from "./MenuMusica";
import { aplicarCorCapa, corDaImagem, reaplicarCorCapa } from "./coresCapa";
import { aplicarTema, ModoTema } from "./tema";
import Estatisticas from "./Estatisticas";
import Configuracoes from "./Configuracoes";
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
  const [tela, setTela] = useState<"biblioteca" | "player">("biblioteca");
  const [volume, setVolume] = useState(1);
  const [detalhes, setDetalhes] = useState<Detalhes | null>(null);
  const [vista, setVista] = useState<Vista>({ tipo: "inicio" });
  const [coresDinamicas, setCoresDinamicas] = useState(false);
  const [playlists, setPlaylists] = useState<PlaylistResumo[]>([]);
  const [versaoPlaylists, setVersaoPlaylists] = useState(0);
  const [curtidas, setCurtidas] = useState<Set<number>>(new Set());
  const [versaoHistorico, setVersaoHistorico] = useState(0);
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
      listen<ImportResumo>("biblioteca-mudou", (e) => {
        setAviso(avisoMonitoramento(e.payload));
        carregarStatus();
        setVersao((v) => v + 1);
        setVersaoPlaylists((v) => v + 1);
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
    const pasta = await open({ directory: true, title: "Escolha a pasta de músicas" });
    if (typeof pasta !== "string") return;
    await importar("import_folder", { path: pasta });
  }

  /** Arquivos soltos: entram na biblioteca sem monitorar a pasta deles. */
  async function importarArquivos() {
    const arquivos = await open({
      multiple: true,
      title: "Escolha os arquivos de música",
      filters: [{ name: "Áudio", extensions: ["mp3", "flac", "ogg", "opus", "oga", "m4a", "aac", "wav"] }],
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
      <header className="topo">
        <h1>Skáld</h1>
        {status && (
          <span className="contagem" title={`Banco: ${status.path} (esquema v${status.schema_version})`}>
            {buscaAtiva ? `${total} de ` : ""}
            {status.media_count} {status.media_count === 1 ? "música" : "músicas"}
          </span>
        )}
        <input
          ref={campoBusca}
          className="busca"
          type="search"
          placeholder="Buscar por título, artista, álbum ou gênero (Ctrl+F)"
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
        <button onClick={importarPasta} disabled={importando}>
          {importando ? "Importando…" : "Importar pasta"}
        </button>
        <button onClick={importarArquivos} disabled={importando} title="Adiciona arquivos soltos, sem monitorar a pasta deles">
          Importar arquivos
        </button>
        <Pastas versao={versao} onErro={setErro} />
        <Duplicados versao={versao} onErro={setErro} />
      </header>
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
      <div className="conteudo" hidden={tela === "player"}>
        <Lateral
          vista={vista}
          playlists={playlists}
          onVista={(v) => {
            setVista(v);
            setTela("biblioteca");
          }}
          onErro={setErro}
        />
        <div className="area" hidden={vista.tipo !== "biblioteca"}>
          <Categorias />
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
        {vista.tipo === "inicio" && (
          <div className="area">
            <Inicio
              versao={versao + versaoHistorico}
              playlists={playlists}
              totalMusicas={status?.media_count ?? 0}
              onPlaylist={(id) => setVista({ tipo: "playlist", id })}
              onAlbum={(chave) => setVista({ tipo: "album", chave, de: vista })}
              onBiblioteca={() => setVista({ tipo: "biblioteca" })}
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
