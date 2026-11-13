import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
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

  const carregarStatus = () =>
    invoke<DbStatus>("db_status")
      .then(setStatus)
      .catch((e) => setErro(String(e)));

  useEffect(() => {
    carregarStatus();
  }, []);

  async function importarPasta() {
    const pasta = await open({ directory: true, title: "Escolha a pasta de músicas" });
    if (typeof pasta !== "string") return;
    setImportando(true);
    setResumo(null);
    setErro(null);
    try {
      setResumo(await invoke<ImportResumo>("import_folder", { path: pasta }));
      await carregarStatus();
    } catch (e) {
      setErro(String(e));
    } finally {
      setImportando(false);
    }
  }

  return (
    <main className="container">
      <h1>Skáld</h1>
      {erro && <p className="erro">Erro: {erro}</p>}
      {!erro && !status && <p>Conectando ao banco de dados…</p>}
      {status && (
        <dl className="status">
          <dt>Banco de dados</dt>
          <dd>{status.path}</dd>
          <dt>Versão do esquema</dt>
          <dd>{status.schema_version}</dd>
          <dt>Músicas na biblioteca</dt>
          <dd>{status.media_count}</dd>
        </dl>
      )}
      <button onClick={importarPasta} disabled={importando}>
        {importando ? "Importando…" : "Importar pasta"}
      </button>
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
    </main>
  );
}

export default App;
