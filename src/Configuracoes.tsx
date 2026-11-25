import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { aplicarTema, ModoTema, temDuasVariantes, TEMAS } from "./tema";
import { escolherPasta, garantirPermissao, noAndroid } from "./android";
import { Bell, Cloud, Database, FolderOpen, Library, Monitor, Palette, Play, type LucideIcon } from "lucide-react";

type Normalizacao = "desligada" | "faixa" | "album";
type Preset = "desligado" | "grave" | "agudo" | "bateria" | "voz" | "instrumental" | "personalizado";
type Config = {
  crossfade_ms: number;
  normalizacao: Normalizacao;
  equalizador: Preset;
  equalizador_bandas: number[];
  tema: string;
  modo_tema: ModoTema;
  cores_dinamicas: boolean;
  bandeja: boolean;
  notificar: boolean;
  continuar_tocando: boolean;
  sync_enviar: boolean;
  sync_pasta: string | null;
  verificar_versao: boolean;
};

const PRESETS: [Preset, string][] = [
  ["desligado", "Desligado"],
  ["grave", "Grave"],
  ["agudo", "Agudo"],
  ["bateria", "Bateria"],
  ["voz", "Voz"],
  ["instrumental", "Instrumental"],
  ["personalizado", "Personalizado"],
];

type Saida = { chave: string; nome: string };

/** Saída de áudio (só no computador; no Android quem escolhe é o sistema). */
function SeletorSaida({ onErro }: { onErro: (m: string) => void }) {
  const [lista, setLista] = useState<Saida[] | null>(null);
  const [escolhida, setEscolhida] = useState<string | null>(null);
  const carregar = () =>
    invoke<{ lista: Saida[]; escolhida: string | null }>("audio_outputs")
      .then((r) => {
        setLista(r.lista);
        setEscolhida(r.escolhida);
      })
      .catch((e) => onErro(String(e)));
  useEffect(() => {
    carregar();
  }, []);
  async function escolher(chave: string | null) {
    const anterior = escolhida;
    setEscolhida(chave);
    try {
      await invoke("set_audio_output", { chave });
    } catch (e) {
      setEscolhida(anterior);
      onErro(String(e));
      carregar();
    }
  }
  // A escolhida pode estar desligada agora (fone Bluetooth): aparece mesmo assim.
  const sumiu = escolhida !== null && lista !== null && !lista.some((s) => s.chave === escolhida);
  return (
    <Linha
      titulo="Saída de áudio"
      nota="Onde o som sai. Na padrão do sistema, o Skáld segue o que você escolher no sistema (ao ligar um fone, por exemplo). Vale na hora."
    >
      <select
        value={escolhida ?? ""}
        aria-label="Saída de áudio"
        disabled={lista === null}
        onFocus={carregar}
        onChange={(e) => escolher(e.target.value || null)}
      >
        <option value="">Padrão do sistema</option>
        {sumiu && <option value={escolhida}>Escolhida antes (desconectada)</option>}
        {(lista ?? []).map((s) => (
          <option key={s.chave} value={s.chave}>
            {s.nome}
          </option>
        ))}
      </select>
    </Linha>
  );
}

/** Frequências do equalizer-10bands do GStreamer. */
const FREQUENCIAS = ["29 Hz", "59 Hz", "119 Hz", "237 Hz", "474 Hz", "947 Hz", "1,9 kHz", "3,8 kHz", "7,5 kHz", "15 kHz"];

/** Ajuste manual das 10 bandas (−12 a +12 dB). Grava enquanto arrasta, no máximo a cada 120 ms. */
function EqualizadorManual({ bandas, onMudar }: { bandas: number[]; onMudar: (b: number[]) => void }) {
  return (
    <div className="eq-manual" role="group" aria-label="Equalizador personalizado">
      {bandas.map((db, i) => (
        <label key={i} className="eq-banda">
          <span className="eq-freq">{FREQUENCIAS[i]}</span>
          <input
            type="range"
            min={-12}
            max={12}
            step={0.5}
            value={db}
            aria-label={`${FREQUENCIAS[i]}: ${db} dB`}
            onChange={(e) => onMudar(bandas.map((v, j) => (j === i ? Number(e.target.value) : v)))}
          />
          <span className="eq-db">
            {db > 0 ? "+" : ""}
            {db.toLocaleString("pt-BR")} dB
          </span>
        </label>
      ))}
      <button className="eq-zerar" onClick={() => onMudar(bandas.map(() => 0))}>
        Zerar
      </button>
    </div>
  );
}

function tamanho(bytes: number) {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1).replace(".", ",")} MB`;
}

/** Um cartão de seção, com ícone e título. */
/** Página de inscrição nos avisos de novas versões (lista e envio ficam no Buttondown). */
const PAGINA_AVISOS = "https://buttondown.com/skald";

function Cartao({ icone: Icone, titulo, children }: { icone: LucideIcon; titulo: string; children: React.ReactNode }) {
  return (
    <section className="cfg-cartao" aria-label={titulo}>
      <h3>
        <span className="cfg-icone" aria-hidden="true">
          <Icone size={18} />
        </span>
        {titulo}
      </h3>
      <div className="cfg-linhas">{children}</div>
    </section>
  );
}

/** Uma opção: nome e explicação à esquerda, controle à direita. */
function Linha({ titulo, nota, children }: { titulo: React.ReactNode; nota?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="cfg-linha">
      <div className="cfg-texto">
        <span className="cfg-titulo">{titulo}</span>
        {nota && <p className="nota">{nota}</p>}
      </div>
      {children && <div className="cfg-controle">{children}</div>}
    </div>
  );
}

type EstadoSync = {
  disponivel: boolean;
  conectado: boolean;
  email: string | null;
  nome: string | null;
  sincronizando: boolean;
};
type ProgressoSync = { fase: "listando" | "subindo" | "baixando" | "dados"; feito: number; total: number; nome: string };
type ResultadoSync = {
  subidas: number;
  baixadas: number;
  iguais: number;
  falhas: string[];
  total_falhas: number;
  cancelado: boolean;
  pasta: string;
  dados: { playlists: number; escutas: number; apelidos: number; categorias?: number } | null;
};

function plural(n: number, um: string, varios: string) {
  return `${n} ${n === 1 ? um : varios}`;
}

function textoProgresso(p: ProgressoSync | null) {
  if (!p || p.fase === "listando") return "Conferindo o que há no Drive…";
  if (p.fase === "dados") return "Sincronizando playlists, curtidas, histórico e categorias…";
  const verbo = p.fase === "subindo" ? "Enviando" : "Baixando";
  return `${verbo} ${p.feito + 1} de ${p.total}: ${p.nome}`;
}

function partesDados(d: ResultadoSync["dados"]) {
  const partes = [];
  if (d?.playlists) partes.push(plural(d.playlists, "playlist atualizada", "playlists atualizadas"));
  if (d?.escutas) partes.push(`${plural(d.escutas, "escuta", "escutas")} no histórico`);
  if (d?.apelidos) partes.push(plural(d.apelidos, "apelido novo", "apelidos novos"));
  if (d?.categorias) partes.push(plural(d.categorias, "mudança nas categorias", "mudanças nas categorias"));
  return partes;
}

function textoDados(d: ResultadoSync["dados"]) {
  const partes = partesDados(d);
  return partes.length ? `Dados da pasta: ${partes.join("; ")}.` : "Dados da pasta: tudo igual, nada mudou.";
}

function textoResultado(r: ResultadoSync) {
  const partes = [];
  if (r.subidas) partes.push(`${plural(r.subidas, "música enviada", "músicas enviadas")} ao Drive`);
  if (r.baixadas) partes.push(`${plural(r.baixadas, "música baixada", "músicas baixadas")} para ${r.pasta}`);
  partes.push(...partesDados(r.dados));
  if (!partes.length) partes.push("Tudo igual: nada mudou");
  let t = partes.join("; ") + ".";
  if (r.cancelado) t = "Sincronização interrompida. " + t;
  if (r.total_falhas) t += ` ${plural(r.total_falhas, "arquivo falhou", "arquivos falharam")}: ${r.falhas.join("; ")}`;
  return t;
}

/** Conta do Google Drive para sincronizar entre aparelhos (etapas 63 a 65). */
function CartaoSincronizacao() {
  const [estado, setEstado] = useState<EstadoSync | null>(null);
  const [entrando, setEntrando] = useState(false);
  const [erro, setErro] = useState<string | null>(null);
  const [sincronizando, setSincronizando] = useState(false);
  const [progresso, setProgresso] = useState<ProgressoSync | null>(null);
  const [resultado, setResultado] = useState<string | null>(null);
  const [enviar, setEnviar] = useState(true);
  const [pastaDados, setPastaDados] = useState<string | null>(null);
  const [sincronizandoPasta, setSincronizandoPasta] = useState(false);

  useEffect(() => {
    invoke<Config>("get_settings")
      .then((c) => {
        setEnviar(c.sync_enviar);
        setPastaDados(c.sync_pasta);
      })
      .catch(() => {});
    invoke<EstadoSync>("sync_status")
      .then((e) => {
        setEstado(e);
        // Saiu das Configurações e voltou no meio da sincronização: retoma o progresso.
        if (e.sincronizando) setSincronizando(true);
      })
      .catch(() => {});
    const progresso = listen<ProgressoSync>("sincronizacao-progresso", (e) => {
      setSincronizando(true);
      setProgresso(e.payload);
    });
    const fim = listen<{ ok?: ResultadoSync; erro?: string }>("sincronizacao-fim", (e) => {
      setSincronizando(false);
      setProgresso(null);
      if (e.payload.ok) setResultado(textoResultado(e.payload.ok));
      if (e.payload.erro) setErro(e.payload.erro);
    });
    return () => {
      progresso.then((f) => f());
      fim.then((f) => f());
    };
  }, []);

  function sincronizar() {
    setErro(null);
    setResultado(null);
    setProgresso(null);
    setSincronizando(true);
    // O resultado chega pelo evento "sincronizacao-fim" (mesmo se a tela fechar e reabrir).
    invoke("sync_now").catch(() => {});
  }

  function mudarEnviar(ativo: boolean) {
    setEnviar(ativo);
    invoke("set_sync_upload", { ativo }).catch((e) => setErro(String(e)));
  }

  /** Sincronizar dados por pasta: escolher, desligar (`null`) ou só rodar agora. */
  async function pastaDadosAcao(acao: "escolher" | "desligar" | "agora") {
    setErro(null);
    setResultado(null);
    let pasta: string | null = null;
    if (acao === "escolher") {
      if (noAndroid) {
        if (!(await garantirPermissao())) {
          setErro("Sem permissão para ler os arquivos. Libere em Configurações do Android → Apps → Skáld → Permissões.");
          return;
        }
        pasta = await escolherPasta();
      } else {
        const p = await open({ directory: true, title: "Escolha a pasta dos dados (a mesma que o Syncthing sincroniza)" });
        pasta = typeof p === "string" ? p : null;
      }
      if (!pasta) return;
    }
    setSincronizandoPasta(true);
    try {
      const m =
        acao === "agora"
          ? await invoke<ResultadoSync["dados"]>("sync_folder_now")
          : await invoke<ResultadoSync["dados"]>("set_sync_folder", { pasta });
      setPastaDados(acao === "agora" ? pastaDados : pasta);
      if (m) setResultado(textoDados(m));
    } catch (e) {
      setErro(String(e));
    } finally {
      setSincronizandoPasta(false);
    }
  }

  async function entrar() {
    setErro(null);
    setEntrando(true);
    try {
      setEstado(await invoke<EstadoSync>("sync_login"));
    } catch (e) {
      setErro(String(e));
    } finally {
      setEntrando(false);
    }
  }

  async function sair() {
    setErro(null);
    try {
      setEstado(await invoke<EstadoSync>("sync_logout"));
    } catch (e) {
      setErro(String(e));
    }
  }

  return (
    <Cartao icone={Cloud} titulo="Sincronização">
      {!estado ? (
        <Linha titulo="Google Drive" nota="…" />
      ) : !estado.disponivel ? (
        <Linha titulo="Google Drive" nota="Indisponível nesta versão do Skáld (compilada sem as credenciais do Google)." />
      ) : estado.conectado ? (
        <Linha
          titulo={
            <>
              Conectado como <strong>{estado.email}</strong>
            </>
          }
          nota="A pasta “Skáld” está pronta no seu Google Drive. O Skáld só enxerga essa pasta. Sair desconecta este aparelho; nada no Drive é apagado."
        >
          <button onClick={sair} disabled={sincronizando}>
            Sair
          </button>
        </Linha>
      ) : (
        <Linha
          titulo="Google Drive"
          nota={
            entrando
              ? "Autorize no navegador que abriu e depois volte para cá."
              : "Guarde músicas, playlists e histórico numa pasta “Skáld” no seu Google Drive para usar nos outros aparelhos. O Skáld só enxerga essa pasta, e nada sai do aparelho sem você pedir."
          }
        >
          {entrando ? (
            <button onClick={() => invoke("sync_cancel").catch(() => {})}>Cancelar</button>
          ) : (
            <button onClick={entrar}>Entrar com o Google</button>
          )}
        </Linha>
      )}
      {estado?.conectado && (
        <Linha
          titulo="Músicas e dados"
          nota={
            sincronizando
              ? textoProgresso(progresso)
              : `Baixa as músicas do Drive que faltam neste aparelho${enviar ? " e envia as que o Drive não tem" : ""}. Depois junta playlists, curtidas, histórico e apelidos de todos os aparelhos. Nenhuma música é apagada, nem aqui nem no Drive.`
          }
        >
          {sincronizando ? (
            <button onClick={() => invoke("sync_cancel").catch(() => {})}>Parar</button>
          ) : (
            <button onClick={sincronizar}>Sincronizar agora</button>
          )}
        </Linha>
      )}
      {estado?.conectado && (
        <Linha
          titulo="Enviar músicas deste aparelho"
          nota="Desligado, só baixa do Drive (útil num aparelho que já tem as mesmas músicas de outro). Playlists e histórico seguem sincronizando."
        >
          <Interruptor ligado={enviar} rotulo="Enviar músicas deste aparelho" onMudar={mudarEnviar} />
        </Linha>
      )}
      <Linha
        titulo="Dados por pasta (Syncthing)"
        nota={
          pastaDados ? (
            <>
              Playlists, curtidas, histórico e apelidos ficam em <strong>{pastaDados}</strong>, juntados ao abrir, a cada 5
              minutos e ao fechar. Use a mesma pasta, sincronizada pelo Syncthing, em todos os aparelhos. As músicas vão pela
              pasta de música que o Syncthing também sincroniza.
            </>
          ) : (
            "Sem nuvem: escolha uma pasta que outro programa (como o Syncthing) mantém igual nos seus aparelhos, e o Skáld junta playlists, curtidas, histórico e apelidos por ela. Nada vai para a internet."
          )
        }
      >
        {pastaDados && (
          <button onClick={() => pastaDadosAcao("agora")} disabled={sincronizandoPasta}>
            {sincronizandoPasta ? "Sincronizando…" : "Sincronizar dados agora"}
          </button>
        )}
        <button onClick={() => pastaDadosAcao("escolher")} disabled={sincronizandoPasta}>
          {pastaDados ? "Trocar pasta…" : "Escolher pasta…"}
        </button>
        {pastaDados && (
          <button onClick={() => pastaDadosAcao("desligar")} disabled={sincronizandoPasta}>
            Desligar
          </button>
        )}
      </Linha>
      {sincronizando && progresso && progresso.total > 0 && (
        <progress className="cfg-progresso" max={progresso.total} value={progresso.feito} />
      )}
      {resultado && <p className="nota destaque">{resultado}</p>}
      {erro && <p className="nota destaque">{erro}</p>}
    </Cartao>
  );
}

/** Interruptor liga/desliga (checkbox com outro visual). */
function Interruptor({ ligado, rotulo, onMudar }: { ligado: boolean; rotulo: string; onMudar: (v: boolean) => void }) {
  return (
    <input
      type="checkbox"
      role="switch"
      className="interruptor"
      aria-label={rotulo}
      checked={ligado}
      onChange={(e) => onMudar(e.target.checked)}
    />
  );
}

/** Configurações (seção 12), em cartões: Aparência, Reprodução, Biblioteca, Sistema e Dados. */
export default function Configuracoes({ onErro }: { onErro: (m: string) => void }) {
  const [crossfade, setCrossfade] = useState<number | null>(null);
  const [normalizacao, setNormalizacao] = useState<Normalizacao>("faixa");
  const [equalizador, setEqualizador] = useState<Preset>("desligado");
  const [bandas, setBandas] = useState<number[]>(Array(10).fill(0));
  const envioBandas = useRef<number | null>(null);
  const [tema, setTema] = useState("carmesim");
  const [modo, setModo] = useState<ModoTema>("sistema");
  const [coresDinamicas, setCoresDinamicas] = useState(true);
  const [bandeja, setBandeja] = useState(false);
  const [notificar, setNotificar] = useState(true);
  const [continuar, setContinuar] = useState(true);
  const [verificarVersao, setVerificarVersao] = useState(false);
  const [autostart, setAutostart] = useState(false);
  const [cache, setCache] = useState<number | null>(null);
  const [mpris, setMpris] = useState<string | null | undefined>(undefined);
  const [analise, setAnalise] = useState<{ analisadas: number; pendentes: number } | null>(null);
  const [avisoDados, setAvisoDados] = useState<string | null>(null);
  const [pastas, setPastas] = useState<string[]>([]);
  const [musicas, setMusicas] = useState<number | null>(null);
  const [ausentes, setAusentes] = useState(0);
  const [confirmarAusentes, setConfirmarAusentes] = useState(false);
  const [avisoPodcasts, setAvisoPodcasts] = useState<string | null>(null);
  const [versao, setVersao] = useState<string | null>(null);
  const [importandoPodcasts, setImportandoPodcasts] = useState(false);

  useEffect(() => {
    invoke<Config>("get_settings")
      .then((c) => {
        setCrossfade(c.crossfade_ms / 1000);
        setNormalizacao(c.normalizacao);
        setEqualizador(c.equalizador);
        setBandas(c.equalizador_bandas);
        setTema(c.tema);
        setModo(c.modo_tema);
        setCoresDinamicas(c.cores_dinamicas);
        setBandeja(c.bandeja);
        setNotificar(c.notificar);
        setContinuar(c.continuar_tocando);
        setVerificarVersao(c.verificar_versao);
      })
      .catch((e) => onErro(String(e)));
    invoke<boolean>("autostart_get")
      .then(setAutostart)
      .catch(() => {});
    invoke<string[]>("watched_folders")
      .then(setPastas)
      .catch(() => {});
    invoke<{ media_count: number }>("db_status")
      .then((d) => setMusicas(d.media_count))
      .catch(() => {});
    getVersion()
      .then(setVersao)
      .catch(() => {});
    invoke<number>("missing_count")
      .then(setAusentes)
      .catch(() => {});
    invoke<number>("cache_size")
      .then(setCache)
      .catch(() => {});
    const lerMpris = () =>
      invoke<string | null>("mpris_status")
        .then(setMpris)
        .catch(() => {});
    lerMpris();
    const lerAnalise = () =>
      invoke<{ analisadas: number; pendentes: number }>("replaygain_status")
        .then(setAnalise)
        .catch(() => {});
    lerAnalise();
    const relogio = setInterval(lerAnalise, 5000);
    const parar = listen("mpris-mudou", lerMpris);
    return () => {
      parar.then((f) => f());
      clearInterval(relogio);
    };
  }, []);

  async function exportarBackup() {
    const data = new Date().toISOString().slice(0, 10);
    const destino = await save({
      title: "Salvar backup do Skáld",
      defaultPath: `skald-backup-${data}.db`,
      filters: [{ name: "Banco do Skáld", extensions: ["db"] }],
    });
    if (!destino) return;
    try {
      await invoke("backup_export", { destino });
      setAvisoDados(`Backup salvo em ${destino}`);
    } catch (e) {
      onErro(String(e));
    }
  }

  async function restaurarBackup() {
    const origem = await open({
      title: "Escolha o backup do Skáld",
      multiple: false,
      filters: [{ name: "Banco do Skáld", extensions: ["db"] }],
    });
    if (typeof origem !== "string") return;
    const ok = await ask(
      "A biblioteca, playlists, histórico e configurações atuais serão trocados pelos do backup. " +
        "O banco atual fica guardado numa cópia de segurança. Os arquivos de música não são alterados. Continuar?",
      { title: "Restaurar backup", kind: "warning", okLabel: "Restaurar", cancelLabel: "Cancelar" },
    );
    if (!ok) return;
    try {
      const copia = await invoke<string>("backup_restore", { origem });
      sessionStorage.setItem("skald-aviso", `Backup restaurado. O banco anterior foi guardado em ${copia}`);
      window.location.reload();
    } catch (e) {
      onErro(String(e));
    }
  }

  /** Etapa 54: importa uma pasta como Podcasts (cada subpasta vira um programa). */
  async function importarPodcasts() {
    let pasta: string | null = null;
    if (noAndroid) {
      if (!(await garantirPermissao())) {
        onErro("Sem permissão para ler os arquivos. Libere em Configurações do Android → Apps → Skáld → Permissões.");
        return;
      }
      pasta = await escolherPasta();
    } else {
      const p = await open({ directory: true, title: "Escolha a pasta de podcasts" });
      pasta = typeof p === "string" ? p : null;
    }
    if (!pasta) return;
    setImportandoPodcasts(true);
    setAvisoPodcasts(null);
    try {
      const r = await invoke<{ adicionados: number; existentes: number }>("import_podcast_folder", { path: pasta });
      const total = r.adicionados + r.existentes;
      setAvisoPodcasts(`${total} ${total === 1 ? "episódio" : "episódios"} em Podcasts. Estão em Biblioteca → Podcasts.`);
      invoke<string[]>("watched_folders").then(setPastas).catch(() => {});
    } catch (e) {
      onErro(String(e));
    } finally {
      setImportandoPodcasts(false);
    }
  }

  async function removerAusentes() {
    setConfirmarAusentes(false);
    try {
      const n = await invoke<number>("remove_missing");
      setAusentes(0);
      setMusicas((m) => (m === null ? m : m - n));
    } catch (e) {
      onErro(String(e));
    }
  }

  async function limparCache() {
    try {
      const liberado = await invoke<number>("cache_clear");
      setCache(0);
      setAvisoDados(`Cache de capas limpo (${tamanho(liberado)} liberados). As capas são refeitas quando aparecerem.`);
    } catch (e) {
      onErro(String(e));
    }
  }

  async function pararDeMonitorar(path: string) {
    try {
      await invoke("unwatch_folder", { path });
      setPastas((p) => p.filter((x) => x !== path));
    } catch (e) {
      onErro(String(e));
    }
  }

  function gravarBandeja(ativo: boolean) {
    setBandeja(ativo);
    invoke("set_tray", { ativo }).catch((e) => {
      setBandeja(false);
      onErro(String(e));
    });
  }

  function gravarVerificarVersao(ativo: boolean) {
    setVerificarVersao(ativo);
    invoke("set_update_check", { ativo })
      .then(() => ativo && window.dispatchEvent(new Event("verificar-versao")))
      .catch((e) => onErro(String(e)));
  }

  function gravarContinuar(ativo: boolean) {
    setContinuar(ativo);
    invoke("set_continue", { ativo }).catch((e) => onErro(String(e)));
  }

  function gravarNotificar(ativo: boolean) {
    setNotificar(ativo);
    invoke("set_notifications", { ativo }).catch((e) => onErro(String(e)));
  }

  function gravarAutostart(ativo: boolean) {
    setAutostart(ativo);
    invoke("set_autostart", { ativo }).catch((e) => {
      setAutostart(!ativo);
      onErro(String(e));
    });
  }

  function gravarAparencia(novoTema: string, novoModo: ModoTema) {
    setTema(novoTema);
    setModo(novoModo);
    aplicarTema(novoTema, novoModo);
    invoke("set_appearance", { tema: novoTema, modo: novoModo }).catch((e) => onErro(String(e)));
  }

  function gravarCoresDinamicas(ativo: boolean) {
    setCoresDinamicas(ativo);
    window.dispatchEvent(new CustomEvent("cores-dinamicas", { detail: ativo }));
    invoke("set_dynamic_colors", { ativo }).catch((e) => onErro(String(e)));
  }

  function gravarEqualizador(preset: Preset) {
    setEqualizador(preset);
    invoke("set_equalizer", { preset }).catch((e) => onErro(String(e)));
  }

  function gravarBandas(b: number[]) {
    setBandas(b);
    if (envioBandas.current !== null) window.clearTimeout(envioBandas.current);
    envioBandas.current = window.setTimeout(() => {
      envioBandas.current = null;
      invoke("set_equalizer", { preset: "personalizado", bandas: b }).catch((e) => onErro(String(e)));
    }, 120);
  }

  function gravarNormalizacao(modo: Normalizacao) {
    setNormalizacao(modo);
    invoke("set_normalization", { modo }).catch((e) => onErro(String(e)));
  }

  function gravar(segundos: number) {
    invoke<number>("set_crossfade", { ms: Math.round(segundos * 1000) })
      .then((ms) => setCrossfade(ms / 1000))
      .catch((e) => onErro(String(e)));
  }

  if (crossfade === null) return null;
  return (
    <section className="configuracoes">
      <h2>Configurações</h2>

      <Cartao icone={Bell} titulo="Novidades">
        <Linha
          titulo={
            <>
              Versão do Skáld: <strong>{versao ?? "…"}</strong>
            </>
          }
          nota="O que mudou em cada versão está nas notas, no GitHub."
        >
          <button onClick={() => openUrl("https://github.com/Beedb/skald/releases")}>Notas das versões…</button>
        </Linha>
        <Linha
          titulo="Aviso de novas versões por e-mail"
          nota="Abre a página de inscrição no navegador. O cadastro é feito no Buttondown, com confirmação por e-mail e descadastro em qualquer aviso; o Skáld não guarda nem envia o seu e-mail."
        >
          <button onClick={() => openUrl(PAGINA_AVISOS)}>Receber avisos…</button>
        </Linha>
        <Linha
          titulo="Verificar novas versões ao abrir"
          nota="Consulta o GitHub ao abrir o Skáld e mostra um aviso no topo quando houver versão nova. É o único acesso à internet que o Skáld faz sozinho, por isso vem desligado."
        >
          <Interruptor ligado={verificarVersao} rotulo="Verificar novas versões ao abrir" onMudar={gravarVerificarVersao} />
        </Linha>
      </Cartao>

      <Cartao icone={Palette} titulo="Aparência">
        <Linha titulo="Tema">
          <select value={tema} aria-label="Tema" onChange={(e) => gravarAparencia(e.target.value, modo)}>
            {TEMAS.map((t) => (
              <option key={t.id} value={t.id}>
                {t.nome}
              </option>
            ))}
          </select>
        </Linha>
        <Linha titulo="Claro ou escuro" nota={temDuasVariantes(tema) ? undefined : "Este tema tem uma variante só."}>
          <select
            value={modo}
            aria-label="Claro ou escuro"
            disabled={!temDuasVariantes(tema)}
            onChange={(e) => gravarAparencia(tema, e.target.value as ModoTema)}
          >
            <option value="sistema">Seguir o sistema</option>
            <option value="claro">Claro</option>
            <option value="escuro">Escuro</option>
          </select>
        </Linha>
        <Linha
          titulo="Cores da capa"
          nota="O destaque da interface e o fundo do player acompanham a capa da música atual, sempre com contraste legível."
        >
          <Interruptor ligado={coresDinamicas} rotulo="Cores da capa" onMudar={gravarCoresDinamicas} />
        </Linha>
      </Cartao>

      <Cartao icone={Play} titulo="Reprodução">
        <Linha
          titulo={
            <>
              Crossfade: <strong>{crossfade === 0 ? "desligado" : `${crossfade} s`}</strong>
            </>
          }
          nota="Sobrepõe o fim de uma música ao início da seguinte. Entre faixas do mesmo álbum não há crossfade: elas são emendadas sem pausa (gapless). Vale a partir da próxima troca de música."
        >
          <input
            type="range"
            className="cfg-faixa"
            min={0}
            max={12}
            step={1}
            value={crossfade}
            aria-label="Duração do crossfade em segundos"
            onChange={(e) => setCrossfade(Number(e.target.value))}
            onPointerUp={(e) => gravar(Number((e.target as HTMLInputElement).value))}
            onKeyUp={(e) => gravar(Number((e.target as HTMLInputElement).value))}
          />
        </Linha>
        <Linha
          titulo="Continuar tocando"
          nota="Quando a fila ou a playlist acaba, segue com músicas do mesmo artista, depois do mesmo gênero e depois da biblioteca inteira em ordem aleatória. A playlist não muda. Repetir fila ou faixa tem prioridade."
        >
          <Interruptor ligado={continuar} rotulo="Continuar tocando" onMudar={gravarContinuar} />
        </Linha>
        <Linha
          titulo="Normalização de volume (ReplayGain)"
          nota={
            <>
              Usa os valores de ReplayGain gravados nos arquivos para equilibrar o volume. "Por faixa" iguala todas as
              músicas; "por álbum" mantém a diferença de volume entre as faixas de um mesmo álbum. Arquivos sem esses
              valores são analisados pelo Skáld em segundo plano (o resultado fica só no banco; os arquivos não mudam).
              {analise && normalizacao !== "desligada" && (
                <>
                  <br />
                  {analise.pendentes > 0
                    ? `Analisando o volume: ${analise.pendentes} ${analise.pendentes === 1 ? "música restante" : "músicas restantes"}.`
                    : "Análise de volume em dia."}{" "}
                  {analise.analisadas}{" "}
                  {analise.analisadas === 1 ? "música sem tags já analisada" : "músicas sem tags já analisadas"}.
                </>
              )}
            </>
          }
        >
          <select
            value={normalizacao}
            aria-label="Normalização de volume"
            onChange={(e) => gravarNormalizacao(e.target.value as Normalizacao)}
          >
            <option value="desligada">Desligada</option>
            <option value="faixa">Por faixa</option>
            <option value="album">Por álbum</option>
          </select>
        </Linha>
        <Linha
          titulo="Equalizador"
          nota="Grave e agudo reforçam as pontas; bateria destaca o bumbo e a caixa; voz realça os médios; instrumental dá mais clareza e corpo; personalizado deixa você ajustar cada faixa de frequência. O volume é compensado para não distorcer. Vale na hora."
        >
          <select value={equalizador} aria-label="Equalizador" onChange={(e) => gravarEqualizador(e.target.value as Preset)}>
            {PRESETS.map(([valor, nome]) => (
              <option key={valor} value={valor}>
                {nome}
              </option>
            ))}
          </select>
        </Linha>
        {equalizador === "personalizado" && <EqualizadorManual bandas={bandas} onMudar={gravarBandas} />}
        {!noAndroid && <SeletorSaida onErro={onErro} />}
      </Cartao>

      <Cartao icone={Library} titulo="Biblioteca">
        <Linha
          titulo={
            <>
              Músicas: <strong>{musicas === null ? "…" : musicas}</strong>
            </>
          }
          nota="Importe pastas ou arquivos pelos botões do topo da biblioteca. Remover da biblioteca nunca apaga o arquivo."
        />
        {ausentes > 0 && (
          <Linha
            titulo={`Músicas não encontradas (${ausentes})`}
            nota={
              confirmarAusentes
                ? "Elas saem das playlists e das curtidas, e o histórico fica sem o vínculo com a música. Não dá para desfazer."
                : "O arquivo dessas músicas sumiu do disco. Removê-las tira só o registro da biblioteca."
            }
          >
            {confirmarAusentes ? (
              <>
                <button onClick={removerAusentes}>Remover {ausentes}</button>
                <button onClick={() => setConfirmarAusentes(false)}>Cancelar</button>
              </>
            ) : (
              <button onClick={() => setConfirmarAusentes(true)}>Remover da biblioteca…</button>
            )}
          </Linha>
        )}
        <Linha
          titulo={`Pastas monitoradas (${pastas.length})`}
          nota="Arquivos novos, movidos ou apagados nessas pastas são notados sozinhos. Ao importar uma pasta, ela passa a ser monitorada."
        />
        <Linha
          titulo="Podcasts"
          nota="Importa uma pasta de episódios. Cada subpasta (ou a própria pasta) vira um programa, e os arquivos dela deixam de aparecer em Músicas. Nada é apagado nem movido."
        >
          <button onClick={importarPodcasts} disabled={importandoPodcasts}>
            {importandoPodcasts ? "Importando…" : "Importar pasta de podcasts…"}
          </button>
        </Linha>
        {avisoPodcasts && <p className="nota destaque">{avisoPodcasts}</p>}
        {pastas.length > 0 && (
          <ul className="cfg-pastas">
            {pastas.map((p) => (
              <li key={p}>
                <FolderOpen size={16} aria-hidden="true" />
                <span className="caminho" title={p}>
                  {p}
                </span>
                <button onClick={() => pararDeMonitorar(p)} title="As músicas desta pasta continuam na biblioteca">
                  Parar de monitorar
                </button>
              </li>
            ))}
          </ul>
        )}
      </Cartao>

      <Cartao icone={Monitor} titulo="Sistema">
        <Linha
          titulo="Fechar a janela envia para a bandeja"
          nota="Mostra um ícone na bandeja com tocar/pausar, próxima, anterior e sair. Fechar a janela só a esconde e a música continua. Desligado, fechar a janela encerra o Skáld."
        >
          <Interruptor ligado={bandeja} rotulo="Fechar a janela envia para a bandeja" onMudar={gravarBandeja} />
        </Linha>
        <Linha titulo="Iniciar com o sistema" nota="Abre o Skáld ao entrar na sessão. Com a bandeja ligada, ele começa escondido nela.">
          <Interruptor ligado={autostart} rotulo="Iniciar com o sistema" onMudar={gravarAutostart} />
        </Linha>
        <Linha titulo="Notificar ao trocar de música" nota="Só aparece quando a janela do Skáld não está em primeiro plano.">
          <Interruptor ligado={notificar} rotulo="Notificar ao trocar de música" onMudar={gravarNotificar} />
        </Linha>
        <Linha
          titulo="Teclas de mídia e controles do sistema"
          nota={mpris ? `${mpris} Enquanto isso, o sistema não consegue controlar o Skáld; o resto funciona normalmente.` : undefined}
        >
          <strong className={"cfg-estado" + (mpris === null ? " ok" : "")}>
            {mpris === undefined ? "…" : mpris === null ? "Ativos" : "Indisponíveis"}
          </strong>
        </Linha>
      </Cartao>

      <CartaoSincronizacao />

      <Cartao icone={Database} titulo="Dados">
        <Linha
          titulo="Backup"
          nota="O backup é um arquivo .db com a biblioteca, playlists, curtidas, histórico e configurações. Não inclui as músicas (elas continuam nas suas pastas) nem as capas guardadas só no Skáld."
        >
          <button onClick={exportarBackup}>Exportar…</button>
          <button onClick={restaurarBackup}>Restaurar…</button>
        </Linha>
        <Linha
          titulo={
            <>
              Cache de capas: <strong>{cache === null ? "…" : tamanho(cache)}</strong>
            </>
          }
          nota="Miniaturas das grades e capas enviadas ao sistema. Nada nos arquivos de música muda."
        >
          <button onClick={limparCache} disabled={!cache}>
            Limpar
          </button>
        </Linha>
        {avisoDados && <p className="nota destaque">{avisoDados}</p>}
      </Cartao>
    </section>
  );
}
