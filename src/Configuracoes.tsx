import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { aplicarTema, ModoTema, temDuasVariantes, TEMAS } from "./tema";
import { Bell, Database, FolderOpen, Library, Monitor, Palette, Play, type LucideIcon } from "lucide-react";

type Normalizacao = "desligada" | "faixa" | "album";
type Preset = "desligado" | "grave" | "agudo" | "bateria" | "voz" | "instrumental";
type Config = {
  crossfade_ms: number;
  normalizacao: Normalizacao;
  equalizador: Preset;
  tema: string;
  modo_tema: ModoTema;
  cores_dinamicas: boolean;
  bandeja: boolean;
  notificar: boolean;
  continuar_tocando: boolean;
};

const PRESETS: [Preset, string][] = [
  ["desligado", "Desligado"],
  ["grave", "Grave"],
  ["agudo", "Agudo"],
  ["bateria", "Bateria"],
  ["voz", "Voz"],
  ["instrumental", "Instrumental"],
];

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
  const [tema, setTema] = useState("carmesim");
  const [modo, setModo] = useState<ModoTema>("sistema");
  const [coresDinamicas, setCoresDinamicas] = useState(true);
  const [bandeja, setBandeja] = useState(false);
  const [notificar, setNotificar] = useState(true);
  const [continuar, setContinuar] = useState(true);
  const [autostart, setAutostart] = useState(false);
  const [cache, setCache] = useState<number | null>(null);
  const [mpris, setMpris] = useState<string | null | undefined>(undefined);
  const [analise, setAnalise] = useState<{ analisadas: number; pendentes: number } | null>(null);
  const [avisoDados, setAvisoDados] = useState<string | null>(null);
  const [pastas, setPastas] = useState<string[]>([]);
  const [musicas, setMusicas] = useState<number | null>(null);

  useEffect(() => {
    invoke<Config>("get_settings")
      .then((c) => {
        setCrossfade(c.crossfade_ms / 1000);
        setNormalizacao(c.normalizacao);
        setEqualizador(c.equalizador);
        setTema(c.tema);
        setModo(c.modo_tema);
        setCoresDinamicas(c.cores_dinamicas);
        setBandeja(c.bandeja);
        setNotificar(c.notificar);
        setContinuar(c.continuar_tocando);
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
          titulo="Aviso de novas versões por e-mail"
          nota="Abre a página de inscrição no navegador. O cadastro é feito no Buttondown, com confirmação por e-mail e descadastro em qualquer aviso; o Skáld não guarda nem envia o seu e-mail."
        >
          <button onClick={() => openUrl(PAGINA_AVISOS)}>Receber avisos…</button>
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
          nota="Grave e agudo reforçam as pontas; bateria destaca o bumbo e a caixa; voz realça os médios; instrumental dá mais clareza e corpo. O volume é compensado para não distorcer. Vale na hora."
        >
          <select value={equalizador} aria-label="Equalizador" onChange={(e) => gravarEqualizador(e.target.value as Preset)}>
            {PRESETS.map(([valor, nome]) => (
              <option key={valor} value={valor}>
                {nome}
              </option>
            ))}
          </select>
        </Linha>
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
        <Linha
          titulo={`Pastas monitoradas (${pastas.length})`}
          nota="Arquivos novos, movidos ou apagados nessas pastas são notados sozinhos. Ao importar uma pasta, ela passa a ser monitorada."
        />
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
