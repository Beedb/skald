import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Resumo = {
  tempo_total_ms: number;
  musicas_escutadas: number;
  artistas_distintos: number;
};

/** "3 dias e 4 h", "5 h 12 min", "8 min". */
export function formatarTempoLongo(ms: number) {
  const min = Math.floor(ms / 60000);
  const h = Math.floor(min / 60);
  const dias = Math.floor(h / 24);
  if (dias > 0) return `${dias} ${dias === 1 ? "dia" : "dias"} e ${h % 24} h`;
  if (h > 0) return `${h} h ${min % 60} min`;
  return `${min} min`;
}

const numero = (n: number) => n.toLocaleString("pt-BR");

type Props = {
  /** Muda quando o histórico recebe uma reprodução nova ou atualizada. */
  versao: number;
  onErro: (mensagem: string) => void;
};

/** Painel permanente (seção 11.2): acumula desde o primeiro uso, nunca é zerado. */
export default function Estatisticas({ versao, onErro }: Props) {
  const [resumo, setResumo] = useState<Resumo | null>(null);

  useEffect(() => {
    invoke<Resumo>("stats_summary")
      .then(setResumo)
      .catch((e) => onErro(String(e)));
  }, [versao]);

  if (!resumo) return null;
  return (
    <section className="estatisticas">
      <h2>Estatísticas</h2>
      <div className="indicadores">
        <div className="indicador" title={`${numero(Math.round(resumo.tempo_total_ms / 60000))} minutos`}>
          <strong>{formatarTempoLongo(resumo.tempo_total_ms)}</strong>
          <span>de música ouvida</span>
        </div>
        <div className="indicador" title="Reproduções com mais de 60% da duração e no mínimo 20 s ouvidos">
          <strong>{numero(resumo.musicas_escutadas)}</strong>
          <span>{resumo.musicas_escutadas === 1 ? "música escutada" : "músicas escutadas"}</span>
        </div>
        <div className="indicador" title="Entre as músicas escutadas; grafias que só diferem em acentos ou maiúsculas contam uma vez">
          <strong>{numero(resumo.artistas_distintos)}</strong>
          <span>{resumo.artistas_distintos === 1 ? "artista" : "artistas"}</span>
        </div>
      </div>
      <p className="nota">Contado desde o primeiro uso, inclusive músicas que já saíram da biblioteca.</p>
    </section>
  );
}
