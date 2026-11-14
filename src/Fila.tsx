import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { MediaRow } from "./Biblioteca";
import { formatarDuracao } from "./Biblioteca";
import { useMenuMusica } from "./MenuMusica";
import Capa from "./Capa";
import { GripVertical, Trash2, Volume2, X } from "lucide-react";

export type VisaoFila = {
  atual: MediaRow | null;
  manual: MediaRow[];
  contexto: MediaRow[];
  aleatorio: boolean;
  repetir: Repetir;
};

export type Repetir = "desligado" | "fila" | "faixa";

type Arraste = {
  draggable: true;
  onDragStart: (e: React.DragEvent) => void;
  onDragOver: (e: React.DragEvent) => void;
  onDrop: (e: React.DragEvent) => void;
  onDragEnd: () => void;
};

function Item({
  m,
  onRemover,
  arraste,
  classe = "",
  onMenu,
  atual = false,
}: {
  m: MediaRow;
  onRemover?: () => void;
  arraste?: Arraste;
  classe?: string;
  onMenu?: (e: React.MouseEvent) => void;
  atual?: boolean;
}) {
  return (
    <li
      className={(m.missing ? "ausente " : "") + (atual ? "atual " : "") + classe}
      title={m.missing ? "Arquivo não encontrado" : arraste ? "Arraste para reordenar" : undefined}
      {...arraste}
      onContextMenu={onMenu}
    >
      {arraste && (
        <span className="alca" aria-hidden="true">
          <GripVertical size={14} />
        </span>
      )}
      <Capa id={m.id} className="capa-fila" />
      <div className="faixa">
        <strong>{m.title ?? "Sem título"}</strong>
        <span>{m.artist ?? "Artista desconhecido"}</span>
      </div>
      {atual ? (
        <Volume2 size={16} className="tocando-icone" aria-label="Tocando agora" />
      ) : (
        <span className="tempo">{formatarDuracao(m.duration_ms)}</span>
      )}
      {onRemover && (
        <button className="remover" onClick={onRemover} aria-label="Remover da fila" title="Remover da fila">
          <X size={14} />
        </button>
      )}
    </li>
  );
}

export default function Fila({
  fila,
  onErro,
  titulo = "Fila",
}: {
  fila: VisaoFila | null;
  onErro: (m: string) => void;
  titulo?: string;
}) {
  const vazia = !fila || (fila.manual.length === 0 && fila.contexto.length === 0);
  const [arrastando, setArrastando] = useState<number | null>(null);
  const abrirMenu = useMenuMusica();
  const [alvo, setAlvo] = useState<number | null>(null);
  const acao = (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args).catch((e) => onErro(String(e)));

  // Soltar sobre um item põe a música arrastada no lugar dele.
  const arraste = (i: number): Arraste => ({
    draggable: true,
    onDragStart: (e) => {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", String(i));
      setArrastando(i);
    },
    onDragOver: (e) => {
      if (arrastando === null) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      if (alvo !== i) setAlvo(i);
    },
    onDrop: (e) => {
      e.preventDefault();
      if (arrastando !== null && arrastando !== i) acao("queue_move", { de: arrastando, para: i });
      setArrastando(null);
      setAlvo(null);
    },
    onDragEnd: () => {
      setArrastando(null);
      setAlvo(null);
    },
  });

  const classeArraste = (i: number) =>
    i === arrastando ? "arrastando" : i === alvo && arrastando !== null ? (i < arrastando ? "alvo-acima" : "alvo-abaixo") : "";

  return (
    <aside className="painel-fila" aria-label="Fila de reprodução">
      <div className="fila-topo">
        <h2>{titulo}</h2>
        {!vazia && (
          <button className="limpar-fila" onClick={() => acao("queue_clear")} title="Tira tudo o que viria depois da música atual">
            Limpar fila <Trash2 size={14} aria-hidden="true" />
          </button>
        )}
      </div>
      {fila?.atual && (
        <section>
          <h3>Tocando agora</h3>
          <ul>
            <Item m={fila.atual} atual onMenu={(e) => abrirMenu(e, fila.atual!.id)} />
          </ul>
        </section>
      )}
      {fila && fila.manual.length > 0 && (
        <section>
          <h3>Próximas na fila</h3>
          <ul>
            {fila.manual.map((m, i) => (
              <Item
                key={`${i}-${m.id}`}
                m={m}
                arraste={arraste(i)}
                classe={classeArraste(i)}
                onMenu={(e) => abrirMenu(e, m.id)}
                onRemover={() => acao("queue_remove", { posicao: i })}
              />
            ))}
          </ul>
        </section>
      )}
      {fila && fila.contexto.length > 0 && (
        <section>
          <h3>A seguir</h3>
          <ul>
            {fila.contexto.map((m) => (
              <Item key={m.id} m={m} onMenu={(e) => abrirMenu(e, m.id)} />
            ))}
          </ul>
        </section>
      )}
      {vazia && <p className="dica">A fila está vazia. Clique com o botão direito numa música para adicioná-la.</p>}
    </aside>
  );
}
