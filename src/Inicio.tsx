import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Capa from "./Capa";
import { Album, CartaoAlbum, ChaveAlbum } from "./Colecao";
import type { PlaylistResumo } from "./Lateral";

/** Tela inicial (seção 4.2): playlists tocadas há pouco e prévia da biblioteca. */

type Musica = { id: number; title: string | null; artist: string | null; missing: boolean };
type DadosInicio = { albuns_recentes: Album[]; tocadas_recentes: Musica[] };

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
          <div className="capa-mini">
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
      <button className="tocar-cartao primario" onClick={onTocar} disabled={!p.total} aria-label={`Tocar ${p.name}`}>
        ▶
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
  onTocarMusica,
  onErro,
}: {
  versao: number;
  playlists: PlaylistResumo[];
  totalMusicas: number;
  onPlaylist: (id: number) => void;
  onAlbum: (chave: ChaveAlbum) => void;
  onBiblioteca: () => void;
  onTocarMusica: (id: number) => void;
  onErro: (m: string) => void;
}) {
  const [dados, setDados] = useState<DadosInicio | null>(null);
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

  return (
    <section className="inicio colecao-rolagem">
      <h2>{saudacao()}</h2>
      <p className="detalhe">
        {totalMusicas} {totalMusicas === 1 ? "música" : "músicas"} na biblioteca ·{" "}
        <button className="link" onClick={onBiblioteca}>
          ver todas
        </button>
      </p>

      {totalMusicas === 0 && (
        <p className="vazio">A biblioteca está vazia. Use "Importar pasta" ou "Importar arquivos" no topo.</p>
      )}

      {recentes.length > 0 && (
        <>
          <h3>Playlists tocadas recentemente</h3>
          <div className="faixa-cartoes">
            {recentes.map((p) => (
              <CartaoPlaylist key={p.id} p={p} onAbrir={() => onPlaylist(p.id)} onTocar={() => tocarPlaylist(p.id)} />
            ))}
          </div>
        </>
      )}

      {dados && dados.tocadas_recentes.length > 0 && (
        <>
          <h3>Tocadas recentemente</h3>
          <ul className="tocadas-recentes">
            {dados.tocadas_recentes.map((m) => (
              <li key={m.id}>
                <button
                  className={m.missing ? "ausente" : undefined}
                  onClick={() => onTocarMusica(m.id)}
                  title={m.missing ? "Arquivo não encontrado" : "Tocar"}
                >
                  <Capa id={m.id} />
                  <span className="nome">{m.title ?? "Sem título"}</span>
                  <span className="detalhe">{m.artist ?? "Desconhecido"}</span>
                </button>
              </li>
            ))}
          </ul>
        </>
      )}

      {dados && dados.albuns_recentes.length > 0 && (
        <>
          <h3>Adicionados recentemente</h3>
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
        </>
      )}
    </section>
  );
}
