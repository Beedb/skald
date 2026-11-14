import Fila, { type VisaoFila } from "./Fila";
import { Capa, Controles, Progresso, Volume, type Reproducao } from "./PlayerBar";
import { useState } from "react";
import { BotaoCurtir } from "./Biblioteca";
import Letra from "./Letra";

export type Detalhes = {
  id: number;
  album_artist: string | null;
  genre: string | null;
  year: number | null;
  track_number: number | null;
  disc_number: number | null;
  path: string;
  file_size: number | null;
  formato: string | null;
  bitrate_kbps: number | null;
  sample_rate: number | null;
  bit_depth: number | null;
  canais: number | null;
  capa: string | null;
};

function tamanho(bytes: number) {
  return bytes >= 1024 * 1024 ? `${(bytes / 1024 / 1024).toFixed(1)} MB` : `${Math.round(bytes / 1024)} KB`;
}

function canais(n: number) {
  return n === 1 ? "mono" : n === 2 ? "estéreo" : `${n} canais`;
}

/** Linhas de "Informações": só as que existem. */
function informacoes(d: Detalhes): [string, string][] {
  const qualidade = [
    d.formato,
    d.bitrate_kbps && `${d.bitrate_kbps} kbps`,
    d.sample_rate && `${(d.sample_rate / 1000).toLocaleString("pt-BR")} kHz`,
    d.bit_depth && `${d.bit_depth} bits`,
    d.canais && canais(d.canais),
  ].filter(Boolean);
  const linhas: [string, string | null | false | undefined][] = [
    ["Artista do álbum", d.album_artist],
    ["Gênero", d.genre],
    ["Ano", d.year?.toString()],
    ["Faixa", d.track_number ? `${d.track_number}${d.disc_number ? ` (disco ${d.disc_number})` : ""}` : null],
    ["Qualidade", qualidade.length > 0 && qualidade.join(" · ")],
    ["Tamanho", d.file_size ? tamanho(d.file_size) : null],
    ["Arquivo", d.path],
  ];
  return linhas.filter((l): l is [string, string] => Boolean(l[1]));
}

type Props = {
  r: Reproducao;
  fila: VisaoFila | null;
  detalhes: Detalhes | null;
  onVoltar: () => void;
};

export default function PlayerCompleto({ r, fila, detalhes, onVoltar }: Props) {
  const m = r.musica;
  const [aba, setAba] = useState<"fila" | "letra">("fila");
  return (
    <section className="player-completo" aria-label="Player completo">
      <button className="voltar" onClick={onVoltar}>
        ← Voltar à biblioteca
      </button>

      <div className="pc-corpo">
        <div className="pc-principal">
          <Capa src={r.capa} className="capa-grande" />
          <div className="pc-titulos">
            <h2>
              {m ? (m.title ?? "Sem título") : "Nada tocando"}
              {m && <BotaoCurtir curtida={r.curtida} onClick={r.onCurtir} />}
            </h2>
            {m && <p className="pc-artista">{m.artist ?? "Artista desconhecido"}</p>}
            {m?.album && <p className="pc-album">{m.album}</p>}
          </div>
          <Progresso r={r} />
          <div className="pc-controles">
            <Controles r={r} />
            <Volume r={r} />
          </div>
        </div>

        <div className="pc-lateral">
          <div className="abas" role="tablist">
            <button role="tab" aria-selected={aba === "fila"} className={aba === "fila" ? "ativa" : ""} onClick={() => setAba("fila")}>
              Fila
            </button>
            <button role="tab" aria-selected={aba === "letra"} className={aba === "letra" ? "ativa" : ""} onClick={() => setAba("letra")}>
              Letra
            </button>
          </div>
          {aba === "fila" ? (
            <Fila fila={fila} onErro={r.onErro} />
          ) : (
            <Letra id={m?.id ?? null} posicaoMs={r.estado?.posicao_ms ?? 0} onErro={r.onErro} />
          )}
          {detalhes && detalhes.id === m?.id && (
            <section className="pc-info">
              <h3>Informações</h3>
              <dl>
                {informacoes(detalhes).map(([rotulo, valor]) => (
                  <div key={rotulo}>
                    <dt>{rotulo}</dt>
                    <dd>{valor}</dd>
                  </div>
                ))}
              </dl>
            </section>
          )}
        </div>
      </div>
    </section>
  );
}
