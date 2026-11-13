import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import Biblioteca from "./Biblioteca";
import PlayerBar, { type EstadoPlayer } from "./PlayerBar";
import Fila, { type VisaoFila } from "./Fila";
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
};

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
    const ouvintes = [
      listen<EstadoPlayer>("player-estado", (e) => {
        setEstado(e.payload);
        if (e.payload.erro) setErro(e.payload.erro);
      }),
      listen("fila-mudou", carregarFila),
      listen<string>("player-erro", (e) => mostrarErro(e.payload)),
    ];
    return () => {
      ouvintes.forEach((o) => o.then((parar) => parar()));
    };
  }, []);

  async function tocar(id: number) {
    setErro(null);
    try {
      await invoke("play_media", { id });
    } catch (e) {
      mostrarErro(String(e));
    }
  }

  function enfileirar(id: number, aSeguir: boolean) {
    invoke("queue_add", { id, aSeguir }).catch((e) => setErro(String(e)));
  }

  async function importarPasta() {
    const pasta = await open({ directory: true, title: "Escolha a pasta de músicas" });
    if (typeof pasta !== "string") return;
    setImportando(true);
    setResumo(null);
    setErro(null);
    try {
      setResumo(await invoke<ImportResumo>("import_folder", { path: pasta }));
      await carregarStatus();
      setVersao((v) => v + 1);
    } catch (e) {
      setErro(String(e));
    } finally {
      setImportando(false);
    }
  }

  return (
    <main className="app">
      <header className="topo">
        <h1>Skáld</h1>
        {status && (
          <span className="contagem" title={`Banco: ${status.path} (esquema v${status.schema_version})`}>
            {status.media_count} {status.media_count === 1 ? "música" : "músicas"}
          </span>
        )}
        <button onClick={importarPasta} disabled={importando}>
          {importando ? "Importando…" : "Importar pasta"}
        </button>
      </header>
      {erro && <p className="erro">Erro: {erro}</p>}
      {!erro && !status && <p>Conectando ao banco de dados…</p>}
      {resumo && (
        <div className="resumo">
          <p>
            {resumo.encontrados} arquivo(s) de áudio encontrado(s): {resumo.adicionados} adicionado(s),{" "}
            {resumo.existentes} já estava(m) na biblioteca.
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
      <div className="conteudo">
        {status && (
          <Biblioteca
            total={status.media_count}
            versao={versao}
            tocandoId={estado && estado.estado !== "parado" ? estado.id : null}
            onTocar={tocar}
            onEnfileirar={enfileirar}
          />
        )}
        {filaAberta && <Fila fila={fila} />}
      </div>
      <PlayerBar
        musica={fila?.atual ?? null}
        estado={estado}
        filaAberta={filaAberta}
        onAlternarFila={() => setFilaAberta((a) => !a)}
        onErro={mostrarErro}
      />
    </main>
  );
}

export default App;
