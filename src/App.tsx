import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type DbStatus = {
  path: string;
  schema_version: number;
  media_count: number;
};

function App() {
  const [status, setStatus] = useState<DbStatus | null>(null);
  const [erro, setErro] = useState<string | null>(null);

  useEffect(() => {
    invoke<DbStatus>("db_status")
      .then(setStatus)
      .catch((e) => setErro(String(e)));
  }, []);

  return (
    <main className="container">
      <h1>Skáld</h1>
      {erro && <p className="erro">Erro ao conectar ao banco: {erro}</p>}
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
    </main>
  );
}

export default App;
