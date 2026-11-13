import { invoke } from "@tauri-apps/api/core";
import type { MediaRow } from "./Biblioteca";
import { formatarDuracao } from "./Biblioteca";

export type VisaoFila = {
  atual: MediaRow | null;
  manual: MediaRow[];
  contexto: MediaRow[];
  aleatorio: boolean;
  repetir: Repetir;
};

export type Repetir = "desligado" | "fila" | "faixa";

function Item({ m, onRemover }: { m: MediaRow; onRemover?: () => void }) {
  return (
    <li className={m.missing ? "ausente" : undefined} title={m.missing ? "Arquivo não encontrado" : undefined}>
      <div className="faixa">
        <strong>{m.title ?? "Sem título"}</strong>
        <span>{m.artist ?? "Artista desconhecido"}</span>
      </div>
      <span className="tempo">{formatarDuracao(m.duration_ms)}</span>
      {onRemover && (
        <button className="remover" onClick={onRemover} aria-label="Remover da fila" title="Remover da fila">
          ×
        </button>
      )}
    </li>
  );
}

export default function Fila({ fila }: { fila: VisaoFila | null }) {
  const vazia = !fila || (fila.manual.length === 0 && fila.contexto.length === 0);
  return (
    <aside className="painel-fila" aria-label="Fila de reprodução">
      <h2>Fila</h2>
      {fila?.atual && (
        <section>
          <h3>Tocando agora</h3>
          <ul>
            <Item m={fila.atual} />
          </ul>
        </section>
      )}
      {fila && fila.manual.length > 0 && (
        <section>
          <h3>Próximas na fila</h3>
          <ul>
            {fila.manual.map((m, i) => (
              <Item key={`${i}-${m.id}`} m={m} onRemover={() => invoke("queue_remove", { posicao: i })} />
            ))}
          </ul>
        </section>
      )}
      {fila && fila.contexto.length > 0 && (
        <section>
          <h3>A seguir na biblioteca</h3>
          <ul>
            {fila.contexto.map((m) => (
              <Item key={m.id} m={m} />
            ))}
          </ul>
        </section>
      )}
      {vazia && <p className="dica">A fila está vazia. Clique com o botão direito numa música para adicioná-la.</p>}
    </aside>
  );
}
