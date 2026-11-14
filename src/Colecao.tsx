import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useVirtualizer } from "@tanstack/react-virtual";
import Capa from "./Capa";
import { formatarDuracao } from "./Biblioteca";

/** Visões de Artistas e Álbuns (seção 4). */

export type ChaveAlbum = { album: string | null; grupo: string };
type Artista = { nome: string | null; albuns: number; musicas: number; capa_id: number };
export type Album = {
  chave: ChaveAlbum;
  artista: string | null;
  ano: number | null;
  musicas: number;
  duracao_ms: number;
  capa_id: number;
  adicionado_em: number;
};
type Faixa = {
  id: number;
  title: string | null;
  artist: string | null;
  duration_ms: number | null;
  missing: boolean;
  track_number: number | null;
  disc_number: number | null;
  artistas: string[];
};

const nomeArtista = (n: string | null) => n ?? "Desconhecido";
const nomeAlbum = (n: string | null) => n ?? "Desconhecido";
const plural = (n: number, um: string, varios: string) => `${n} ${n === 1 ? um : varios}`;

function duracaoLonga(ms: number) {
  const min = Math.round(ms / 60000);
  return min < 60 ? `${min} min` : `${Math.floor(min / 60)} h ${min % 60} min`;
}

type Erro = { onErro: (m: string) => void };

/** Lista de artistas (virtualizada). */
export function Artistas({
  versao,
  onAbrir,
  onErro,
}: { versao: number; onAbrir: (nome: string | null) => void } & Erro) {
  const [artistas, setArtistas] = useState<Artista[] | null>(null);
  const rolagem = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<Artista[]>("artists_list")
      .then(setArtistas)
      .catch((e) => onErro(String(e)));
  }, [versao]);

  const virt = useVirtualizer({
    count: artistas?.length ?? 0,
    getScrollElement: () => rolagem.current,
    estimateSize: () => 60,
    overscan: 8,
  });

  if (!artistas) return null;
  return (
    <section className="colecao">
      <h2>Artistas</h2>
      {artistas.length === 0 && <p className="vazio">Nenhum artista na biblioteca.</p>}
      <div className="colecao-rolagem" ref={rolagem}>
        <div style={{ height: virt.getTotalSize(), position: "relative" }}>
          {virt.getVirtualItems().map((item) => {
            const a = artistas[item.index];
            return (
              <button
                key={item.key}
                className="linha-artista"
                style={{ height: 60, transform: `translateY(${item.start}px)` }}
                onClick={() => onAbrir(a.nome)}
              >
                <Capa id={a.capa_id} className="redonda" />
                <span className="nome">{nomeArtista(a.nome)}</span>
                <span className="detalhe">
                  {plural(a.albuns, "álbum", "álbuns")} · {plural(a.musicas, "música", "músicas")}
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </section>
  );
}

const LARGURA_CARTAO = 176;
const ALTURA_CARTAO = 236;

export function CartaoAlbum({
  album: a,
  largura,
  onAbrir,
}: {
  album: Album;
  largura: number;
  onAbrir: (a: Album) => void;
}) {
  return (
    <button
      className="cartao-album"
      style={{ width: largura }}
      onClick={() => onAbrir(a)}
      title={`${nomeAlbum(a.chave.album)} · ${a.artista ?? "Vários artistas"}`}
    >
      <Capa id={a.capa_id} />
      <span className="nome">{nomeAlbum(a.chave.album)}</span>
      <span className="detalhe">
        {a.artista ?? "Vários artistas"}
        {a.ano ? ` · ${a.ano}` : ""}
      </span>
    </button>
  );
}

/** Grade de álbuns virtualizada por linhas (o número de colunas segue a largura). */
function GradeAlbuns({ albuns, onAbrir }: { albuns: Album[]; onAbrir: (a: Album) => void }) {
  const rolagem = useRef<HTMLDivElement>(null);
  const [colunas, setColunas] = useState(4);

  useEffect(() => {
    const el = rolagem.current;
    if (!el) return;
    const medir = () => setColunas(Math.max(1, Math.floor((el.clientWidth - 8) / LARGURA_CARTAO)));
    medir();
    const obs = new ResizeObserver(medir);
    obs.observe(el);
    return () => obs.disconnect();
  }, []);

  const linhas = Math.ceil(albuns.length / colunas);
  const virt = useVirtualizer({
    count: linhas,
    getScrollElement: () => rolagem.current,
    estimateSize: () => ALTURA_CARTAO,
    overscan: 3,
  });

  return (
    <div className="colecao-rolagem" ref={rolagem}>
      <div style={{ height: virt.getTotalSize(), position: "relative" }}>
        {virt.getVirtualItems().map((linha) => (
          <div
            key={linha.key}
            className="linha-grade"
            style={{ height: ALTURA_CARTAO, transform: `translateY(${linha.start}px)` }}
          >
            {albuns.slice(linha.index * colunas, (linha.index + 1) * colunas).map((a) => (
              <CartaoAlbum
                key={a.chave.grupo + "\u0000" + (a.chave.album ?? "")}
                album={a}
                largura={LARGURA_CARTAO - 16}
                onAbrir={onAbrir}
              />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

/** Todos os álbuns. */
export function Albuns({ versao, onAbrir, onErro }: { versao: number; onAbrir: (c: ChaveAlbum) => void } & Erro) {
  const [albuns, setAlbuns] = useState<Album[] | null>(null);
  useEffect(() => {
    invoke<Album[]>("albums_list", { porArtista: false, artista: null })
      .then(setAlbuns)
      .catch((e) => onErro(String(e)));
  }, [versao]);

  if (!albuns) return null;
  return (
    <section className="colecao">
      <h2>Álbuns</h2>
      {albuns.length === 0 && <p className="vazio">Nenhum álbum na biblioteca.</p>}
      <GradeAlbuns albuns={albuns} onAbrir={(a) => onAbrir(a.chave)} />
    </section>
  );
}

/** Um artista: seus álbuns e "Tocar tudo". */
export function ArtistaView({
  nome,
  versao,
  onAbrirAlbum,
  onVoltar,
  onErro,
}: {
  nome: string | null;
  versao: number;
  onAbrirAlbum: (c: ChaveAlbum) => void;
  onVoltar: () => void;
} & Erro) {
  const [albuns, setAlbuns] = useState<Album[] | null>(null);
  useEffect(() => {
    setAlbuns(null);
    invoke<Album[]>("albums_list", { porArtista: true, artista: nome })
      .then(setAlbuns)
      .catch((e) => onErro(String(e)));
  }, [nome, versao]);

  const musicas = albuns?.reduce((t, a) => t + a.musicas, 0) ?? 0;
  return (
    <section className="colecao">
      <button className="voltar" onClick={onVoltar}>
        ← Artistas
      </button>
      <div className="cabecalho-colecao">
        <div>
          <h2>{nomeArtista(nome)}</h2>
          {albuns && <p className="detalhe">{plural(albuns.length, "álbum", "álbuns")}</p>}
        </div>
        <button
          className="primario"
          disabled={!musicas}
          onClick={() => invoke("play_artist", { artista: nome }).catch((e) => onErro(String(e)))}
        >
          ▶ Tocar tudo
        </button>
      </div>
      {albuns && <GradeAlbuns albuns={albuns} onAbrir={(a) => onAbrirAlbum(a.chave)} />}
    </section>
  );
}

/** Um álbum: capa, informações e faixas. */
export function AlbumView({
  chave,
  versao,
  tocandoId,
  onArtista,
  onVoltar,
  onErro,
}: {
  chave: ChaveAlbum;
  versao: number;
  tocandoId: number | null;
  onArtista: (nome: string) => void;
  onVoltar: () => void;
} & Erro) {
  const [faixas, setFaixas] = useState<Faixa[] | null>(null);
  useEffect(() => {
    setFaixas(null);
    invoke<Faixa[]>("album_tracks", { chave })
      .then(setFaixas)
      .catch((e) => onErro(String(e)));
  }, [chave.album, chave.grupo, versao]);

  const tocar = (indice: number) => invoke("play_album", { chave, indice }).catch((e) => onErro(String(e)));
  const enfileirar = (aSeguir: boolean) =>
    invoke("queue_add_album", { chave, aSeguir }).catch((e) => onErro(String(e)));
  if (!faixas) return null;
  const artistas = new Set(faixas.map((f) => f.artist ?? ""));
  const artistaUnico = artistas.size === 1 ? faixas[0]?.artist : null;
  const discos = new Set(faixas.map((f) => f.disc_number ?? 1)).size;
  const duracao = faixas.reduce((t, f) => t + (f.duration_ms ?? 0), 0);

  return (
    <section className="colecao">
      <button className="voltar" onClick={onVoltar}>
        ← Voltar
      </button>
      <div className="cabecalho-colecao">
        <Capa id={faixas[0]?.id ?? null} className="grande" />
        <div className="info-album">
          <h2>{nomeAlbum(chave.album)}</h2>
          {artistaUnico ? (
            <p className="artistas-album">
              {faixas[0].artistas.map((nome, i) => (
                <span key={nome}>
                  {i > 0 && <span className="detalhe"> · </span>}
                  <button className="link" onClick={() => onArtista(nome)}>
                    {nome}
                  </button>
                </span>
              ))}
            </p>
          ) : (
            <p className="detalhe">Vários artistas</p>
          )}
          <p className="detalhe">
            {plural(faixas.length, "música", "músicas")} · {duracaoLonga(duracao)}
          </p>
          <div className="acoes-album">
            <button className="primario" disabled={faixas.length === 0} onClick={() => tocar(0)}>
              ▶ Tocar álbum
            </button>
            <button disabled={faixas.length === 0} onClick={() => enfileirar(true)}>
              Tocar em seguida
            </button>
            <button disabled={faixas.length === 0} onClick={() => enfileirar(false)}>
              Adicionar à fila
            </button>
          </div>
        </div>
      </div>
      <div className="colecao-rolagem">
        <ol className="faixas-album">
          {faixas.map((f, i) => (
            <li
              key={f.id}
              className={(f.missing ? "ausente" : "") + (f.id === tocandoId ? " atual" : "")}
              title={f.missing ? "Arquivo não encontrado" : undefined}
              onDoubleClick={() => tocar(i)}
            >
              <span className="numero">
                {discos > 1 && f.disc_number ? `${f.disc_number}.` : ""}
                {f.track_number ?? "–"}
              </span>
              <span className="titulo">
                {f.title ?? "Sem título"}
                {!artistaUnico && <span className="detalhe"> · {f.artist ?? "Desconhecido"}</span>}
              </span>
              <span className="duracao">{formatarDuracao(f.duration_ms)}</span>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}
