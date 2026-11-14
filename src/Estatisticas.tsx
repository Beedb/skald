import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Retrospectivas from "./Retrospectivas";

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

export const numero = (n: number) => n.toLocaleString("pt-BR");

export type Item = { nome: string; detalhe: string | null; media_id: number | null; escutadas: number; ms: number };
type Ponto = { periodo: string; ms: number; escutadas: number };
type Painel = {
  periodo: string;
  tempo_ms: number;
  escutadas: number;
  artistas: Item[];
  musicas: Item[];
  albuns: Item[];
  generos: Item[];
  evolucao: Ponto[];
};

export const nomeMes = (ano: string, mes: string, formato: "long" | "short") =>
  new Date(Number(ano), Number(mes) - 1, 1).toLocaleDateString("pt-BR", { month: formato });

/** "2026-10" → "out/26"; "2026-10-05" → "5". */
function rotuloPonto(p: string) {
  const [ano, mes, dia] = p.split("-");
  if (dia) return String(Number(dia));
  return `${nomeMes(ano, mes, "short").replace(".", "")}/${ano.slice(2)}`;
}

function descricaoPeriodo(p: string) {
  if (!p) return "desde o início";
  const [ano, mes] = p.split("-");
  return mes ? `em ${nomeMes(ano, mes, "long")} de ${ano}` : `em ${ano}`;
}

function Evolucao({ pontos }: { pontos: Ponto[] }) {
  if (pontos.length === 0) return null;
  const maior = Math.max(...pontos.map((p) => p.ms), 1);
  return (
    <div className="evolucao" role="img" aria-label="Tempo ouvido ao longo do período">
      {pontos.map((p) => (
        <div
          key={p.periodo}
          className="coluna"
          title={`${p.periodo}: ${formatarTempoLongo(p.ms)}, ${numero(p.escutadas)} escutada(s)`}
        >
          <div className="barra" style={{ height: `${Math.max(2, (p.ms / maior) * 100)}%` }} />
          <span>{rotuloPonto(p.periodo)}</span>
        </div>
      ))}
    </div>
  );
}

function Ranking({ titulo, itens, onTocar }: { titulo: string; itens: Item[]; onTocar?: (id: number) => void }) {
  return (
    <section className="ranking">
      <h3>{titulo}</h3>
      {itens.length === 0 ? (
        <p className="nota">Nada escutado neste período.</p>
      ) : (
        <ol>
          {itens.map((it, i) => {
            const tocavel = onTocar && it.media_id !== null;
            return (
              <li key={i}>
                <span className="pos">{i + 1}</span>
                <span className="nome-item">
                  {tocavel ? (
                    <button className="link" onClick={() => onTocar(it.media_id!)} title="Tocar">
                      {it.nome}
                    </button>
                  ) : (
                    it.nome
                  )}
                  {it.detalhe && <small>{it.detalhe}</small>}
                </span>
                <span className="vezes" title={formatarTempoLongo(it.ms)}>
                  {numero(it.escutadas)}×
                </span>
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}

/** Máquina do Tempo (seção 11.4): os mais escutados e a evolução em um período. */
function MaquinaDoTempo({ versao, onErro, onTocar }: Props) {
  const [meses, setMeses] = useState<string[]>([]);
  const [ano, setAno] = useState("");
  const [mes, setMes] = useState("");
  const [painel, setPainel] = useState<Painel | null>(null);
  const periodo = ano && mes ? `${ano}-${mes}` : ano;

  useEffect(() => {
    invoke<string[]>("stats_months")
      .then(setMeses)
      .catch((e) => onErro(String(e)));
  }, [versao]);

  useEffect(() => {
    invoke<Painel>("stats_period", { periodo })
      .then(setPainel)
      .catch((e) => onErro(String(e)));
  }, [periodo, versao]);

  const anos = [...new Set(meses.map((m) => m.slice(0, 4)))];
  const mesesDoAno = meses.filter((m) => m.startsWith(ano)).map((m) => m.slice(5)).sort();

  return (
    <section className="maquina">
      <div className="maquina-topo">
        <h2>Máquina do Tempo</h2>
        <select
          value={ano}
          aria-label="Ano"
          onChange={(e) => {
            setAno(e.target.value);
            setMes("");
          }}
        >
          <option value="">Desde o início</option>
          {anos.map((a) => (
            <option key={a} value={a}>
              {a}
            </option>
          ))}
        </select>
        {ano && (
          <select value={mes} aria-label="Mês" onChange={(e) => setMes(e.target.value)}>
            <option value="">Ano inteiro</option>
            {mesesDoAno.map((m) => (
              <option key={m} value={m}>
                {nomeMes(ano, m, "long")}
              </option>
            ))}
          </select>
        )}
      </div>
      {painel && (
        <>
          <p className="resumo-periodo">
            <strong>{formatarTempoLongo(painel.tempo_ms)}</strong> de música e{" "}
            <strong>{numero(painel.escutadas)}</strong> {painel.escutadas === 1 ? "música escutada" : "músicas escutadas"}{" "}
            {descricaoPeriodo(painel.periodo)}.
          </p>
          <Evolucao pontos={painel.evolucao} />
          <div className="rankings">
            <Ranking titulo="Artistas" itens={painel.artistas} />
            <Ranking titulo="Músicas" itens={painel.musicas} onTocar={onTocar} />
            <Ranking titulo="Álbuns" itens={painel.albuns} />
            <Ranking titulo="Gêneros" itens={painel.generos} />
          </div>
        </>
      )}
    </section>
  );
}

type Props = {
  /** Muda quando o histórico recebe uma reprodução nova ou atualizada. */
  versao: number;
  onErro: (mensagem: string) => void;
  onTocar: (mediaId: number) => void;
};

/** Painel permanente (seção 11.2): acumula desde o primeiro uso, nunca é zerado. */
export default function Estatisticas(props: Props) {
  const { versao, onErro } = props;
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
      <MaquinaDoTempo {...props} />
      <Retrospectivas versao={versao} onErro={onErro} />
    </section>
  );
}
