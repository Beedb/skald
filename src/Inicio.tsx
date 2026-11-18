import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowRight, Play, Sparkle } from "lucide-react";
import Capa from "./Capa";
import { estadoEpisodio, type Episodio } from "./Podcasts";
import { Album, CartaoAlbum, ChaveAlbum } from "./Colecao";
import type { PlaylistResumo } from "./Lateral";
import { useMenuMusica } from "./MenuMusica";

/** Tela inicial (seção 4.2): mixes automáticos, tocadas há pouco, playlists e álbuns novos. */

type Musica = { id: number; title: string | null; artist: string | null; missing: boolean };
type Mix = {
  tipo: "geral" | "genero" | "artista";
  valor: string | null;
  nome: string;
  descricao: string;
  musicas: number;
  capas: number[];
};
type DadosInicio = { albuns_recentes: Album[]; tocadas_recentes: Musica[]; mixes: Mix[] };

/** Título de seção com o ✦ e, opcionalmente, um "Ver tudo". */
function Secao({ titulo, onVerTudo, children }: { titulo: string; onVerTudo?: () => void; children: ReactNode }) {
  return (
    <section className="secao-inicio">
      <div className="secao-topo">
        <h3>
          <Sparkle size={14} className="estrela" aria-hidden="true" />
          {titulo}
        </h3>
        {onVerTudo && (
          <button className="ver-tudo" onClick={onVerTudo}>
            Ver tudo <ArrowRight size={14} aria-hidden="true" />
          </button>
        )}
      </div>
      {children}
    </section>
  );
}

/** Cartão grande de mix: mosaico das capas, nome embaixo e botão de tocar. */
function CartaoMix({ mix, onTocar }: { mix: Mix; onTocar: () => void }) {
  const capas = mix.capas.length >= 4 ? mix.capas.slice(0, 4) : mix.capas.slice(0, 1);
  return (
    <button className="cartao-mix" onClick={onTocar} title={`Tocar ${mix.nome}`}>
      <div className={"mix-arte" + (capas.length === 4 ? " mosaico" : "")} aria-hidden="true">
        {capas.length ? capas.map((id) => <Capa key={id} id={id} />) : <Capa id={null} />}
      </div>
      <div className="mix-texto">
        <strong>{mix.nome}</strong>
        <span>{mix.descricao}</span>
      </div>
      <span className="mix-play" aria-hidden="true">
        <Play size={16} fill="currentColor" />
      </span>
    </button>
  );
}

function saudacao() {
  const h = new Date().getHours();
  return h < 5 ? "Boa noite" : h < 12 ? "Bom dia" : h < 18 ? "Boa tarde" : "Boa noite";
}

function CartaoPlaylist({ p, onAbrir, onTocar }: { p: PlaylistResumo; onAbrir: () => void; onTocar: () => void }) {
  const [capaEscolhida, setCapaEscolhida] = useState<string | null>(null);
  useEffect(() => {
    if (!p.capa_escolhida) return setCapaEscolhida(null);
    let valido = true;
    invoke<string | null>("playlist_cover", { id: p.id })
      .then((c) => valido && setCapaEscolhida(c))
      .catch(() => {});
    return () => {
      valido = false;
    };
  }, [p.id, p.capa_escolhida]);

  return (
    <div className="cartao-playlist">
      <button className="abrir" onClick={onAbrir} title={p.name}>
        {capaEscolhida ? (
          <div className="capa-colecao">
            <img src={capaEscolhida} alt="" draggable={false} />
          </div>
        ) : (
          <Capa id={p.primeira_media_id} />
        )}
        <span className="nome">
          {p.kind === "curtidas" ? "♥ " : ""}
          {p.name}
        </span>
        <span className="detalhe">
          {p.total} {p.total === 1 ? "música" : "músicas"}
        </span>
      </button>
      <button className="tocar-cartao" onClick={onTocar} disabled={!p.total} aria-label={`Tocar ${p.name}`}>
        <Play size={16} fill="currentColor" />
      </button>
    </div>
  );
}

export default function Inicio({
  versao,
  playlists,
  totalMusicas,
  onPlaylist,
  onAlbum,
  onBiblioteca,
  onAlbuns,
  onHistorico,
  onTocarMusica,
  onPrograma,
  onErro,
}: {
  versao: number;
  playlists: PlaylistResumo[];
  totalMusicas: number;
  onPlaylist: (id: number) => void;
  onAlbum: (chave: ChaveAlbum) => void;
  onBiblioteca: () => void;
  onAlbuns: () => void;
  onHistorico: () => void;
  onTocarMusica: (id: number) => void;
  onPrograma: (id: number) => void;
  onErro: (m: string) => void;
}) {
  const [dados, setDados] = useState<DadosInicio | null>(null);
  const [ouvindo, setOuvindo] = useState<Episodio[]>([]);
  useEffect(() => {
    invoke<Episodio[]>("podcast_in_progress")
      .then(setOuvindo)
      .catch(() => setOuvindo([]));
  }, [versao]);
  const abrirMenu = useMenuMusica();
  useEffect(() => {
    invoke<DadosInicio>("home_data")
      .then(setDados)
      .catch((e) => onErro(String(e)));
  }, [versao]);

  const recentes = playlists
    .filter((p) => p.tocada_em !== null)
    .sort((a, b) => (b.tocada_em ?? 0) - (a.tocada_em ?? 0))
    .slice(0, 8);
  const tocarPlaylist = (id: number) =>
    invoke("play_playlist", { id, ordem: "personalizada", indice: 0 }).catch((e) => onErro(String(e)));

  const tocarMix = (m: Mix) => invoke("play_mix", { tipo: m.tipo, valor: m.valor }).catch((e) => onErro(String(e)));

  return (
    <div className="inicio colecao-rolagem">
      <header className="inicio-topo">
        <h1>
          <Sparkle size={20} className="estrela" aria-hidden="true" />
          Início
        </h1>
        <p className="detalhe">
          {saudacao()} · {totalMusicas} {totalMusicas === 1 ? "música" : "músicas"} na biblioteca ·{" "}
          <button className="link" onClick={onBiblioteca}>
            ver todas
          </button>
        </p>
      </header>

      {totalMusicas === 0 && (
        <p className="vazio">A biblioteca está vazia. Use "Importar pasta" ou "Arquivos" no topo.</p>
      )}

      {ouvindo.length > 0 && (
        <Secao titulo="Continuar ouvindo">
          <div className="faixa-cartoes">
            {ouvindo.map((e) => (
              <div key={e.id} className="cartao-album cartao-episodio" style={{ width: 160 }}>
                <button
                  className="cartao-episodio-tocar"
                  onClick={() => invoke("play_episode", { id: e.id }).catch((x) => onErro(String(x)))}
                  title={`Continuar ${e.titulo}`}
                >
                  <Capa id={e.media_id} />
                  <span className="nome">{e.titulo}</span>
                </button>
                <button className="link detalhe" onClick={() => onPrograma(e.show_id)} title="Abrir o programa">
                  {e.programa} · {estadoEpisodio(e)}
                </button>
                {e.posicao_ms && e.duracao_ms ? (
                  <span className="progresso-episodio" aria-hidden="true">
                    <span style={{ width: `${(e.posicao_ms / e.duracao_ms) * 100}%` }} />
                  </span>
                ) : null}
              </div>
            ))}
          </div>
        </Secao>
      )}

      {dados && dados.mixes.length > 0 && (
        <Secao titulo="Seu mix">
          <div className="faixa-mixes">
            {dados.mixes.map((m) => (
              <CartaoMix key={m.tipo + (m.valor ?? "")} mix={m} onTocar={() => tocarMix(m)} />
            ))}
          </div>
        </Secao>
      )}

      {dados && dados.tocadas_recentes.length > 0 && (
        <Secao titulo="Recentes" onVerTudo={onHistorico}>
          <div className="faixa-cartoes">
            {dados.tocadas_recentes.map((m) => (
              <button
                key={m.id}
                className={"cartao-album" + (m.missing ? " ausente" : "")}
                style={{ width: 160 }}
                onClick={() => onTocarMusica(m.id)}
                onContextMenu={(e) => abrirMenu(e, m.id)}
                title={m.missing ? "Arquivo não encontrado" : `Tocar ${m.title ?? "Sem título"}`}
              >
                <Capa id={m.id} />
                <span className="nome">{m.title ?? "Sem título"}</span>
                <span className="detalhe">{m.artist ?? "Desconhecido"}</span>
              </button>
            ))}
          </div>
        </Secao>
      )}

      {recentes.length > 0 && (
        <Secao titulo="Playlists tocadas recentemente">
          <div className="faixa-cartoes">
            {recentes.map((p) => (
              <CartaoPlaylist key={p.id} p={p} onAbrir={() => onPlaylist(p.id)} onTocar={() => tocarPlaylist(p.id)} />
            ))}
          </div>
        </Secao>
      )}

      {dados && dados.albuns_recentes.length > 0 && (
        <Secao titulo="Adicionados recentemente" onVerTudo={onAlbuns}>
          <div className="faixa-cartoes">
            {dados.albuns_recentes.map((a) => (
              <CartaoAlbum
                key={a.chave.grupo + "\u0000" + (a.chave.album ?? "")}
                album={a}
                largura={160}
                onAbrir={(x) => onAlbum(x.chave)}
              />
            ))}
          </div>
        </Secao>
      )}
    </div>
  );
}
