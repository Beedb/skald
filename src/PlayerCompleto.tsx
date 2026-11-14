import Fila, { type VisaoFila } from "./Fila";
import { Capa, Controles, Progresso, Volume, type Reproducao } from "./PlayerBar";
import { useState } from "react";
import { ArrowLeft, Ellipsis } from "lucide-react";
import { BotaoCurtir } from "./Biblioteca";
import Letra from "./Letra";
import Relacionados from "./Relacionados";
import { useMenuMusica } from "./MenuMusica";

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

type Aba = "fila" | "letra" | "relacionados" | "sobre";
const ABAS: [Aba, string][] = [
  ["fila", "Fila"],
  ["letra", "Letra"],
  ["relacionados", "Relacionados"],
  ["sobre", "Sobre"],
];

export default function PlayerCompleto({ r, fila, detalhes, onVoltar }: Props) {
  const m = r.musica;
  const [aba, setAba] = useState<Aba>("fila");
  const abrirMenu = useMenuMusica();
  const info = detalhes && detalhes.id === m?.id ? informacoes(detalhes) : [];
  return (
    <section className="player-completo" aria-label="Player completo">
      {r.capa && <div className="pc-fundo" style={{ backgroundImage: `url("${r.capa}")` }} aria-hidden="true" />}
      <button className="pc-voltar icone-botao" onClick={onVoltar} aria-label="Voltar" title="Voltar">
        <ArrowLeft size={22} />
      </button>

      <div className="pc-corpo">
        <div className="pc-principal">
          <div className="pc-topo">
            <Capa src={r.capa} className="capa-grande" />
            <div className="pc-titulos">
              <h1>{m ? (m.title ?? "Sem título") : "Nada tocando"}</h1>
              {m && <p className="pc-artista">{m.artist ?? "Artista desconhecido"}</p>}
              {m?.album && <p className="pc-album">{m.album}</p>}
              {m && (
                <div className="pc-acoes">
                  <BotaoCurtir curtida={r.curtida} onClick={r.onCurtir} />
                  <button
                    className="icone-botao"
                    onClick={(e) => abrirMenu(e, m.id)}
                    aria-label="Mais opções"
                    title="Mais opções"
                  >
                    <Ellipsis size={20} />
                  </button>
                </div>
              )}
            </div>
          </div>
          <div className="pc-base">
            <Progresso r={r} />
            <div className="pc-controles">
              <span />
              <Controles r={r} grande />
              <Volume r={r} />
            </div>
          </div>
        </div>

        <div className="pc-lateral">
          <div className="abas" role="tablist">
            {ABAS.map(([id, rotulo]) => (
              <button
                key={id}
                role="tab"
                aria-selected={aba === id}
                className={aba === id ? "ativa" : ""}
                onClick={() => setAba(id)}
              >
                {rotulo}
              </button>
            ))}
          </div>
          <div className="pc-aba">
            {aba === "fila" && <Fila fila={fila} onErro={r.onErro} titulo="Fila de reprodução" />}
            {aba === "letra" && <Letra id={m?.id ?? null} posicaoMs={r.estado?.posicao_ms ?? 0} onErro={r.onErro} />}
            {aba === "relacionados" && <Relacionados id={m?.id ?? null} onErro={r.onErro} />}
            {aba === "sobre" &&
              (info.length > 0 ? (
                <section className="pc-info">
                  <h3>Sobre esta música</h3>
                  <dl>
                    {info.map(([rotulo, valor]) => (
                      <div key={rotulo}>
                        <dt>{rotulo}</dt>
                        <dd>{valor}</dd>
                      </div>
                    ))}
                  </dl>
                </section>
              ) : (
                <p className="dica">Nada tocando.</p>
              ))}
          </div>
        </div>
      </div>
    </section>
  );
}
