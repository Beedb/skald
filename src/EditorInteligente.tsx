import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, X } from "lucide-react";

type Campo =
  | "titulo"
  | "artista"
  | "album"
  | "genero"
  | "ano"
  | "categoria"
  | "curtida"
  | "escutas"
  | "adicionada_dias"
  | "tocada_dias"
  | "nao_tocada_dias"
  | "duracao_min";
type Op = "contem" | "nao_contem" | "igual" | "diferente" | "maior_igual" | "menor_igual";
type Ordem = "aleatoria" | "titulo" | "artista" | "adicionada_recente" | "mais_tocadas" | "tocada_recente" | "ano_recente";
type Regra = { campo: Campo; op: Op; valor: string };
export type Regras = { combinar: "todas" | "qualquer"; regras: Regra[]; limite: number | null; ordem: Ordem };

type Tipo = "texto" | "numero" | "categoria" | "curtida" | "dias";
const CAMPOS: { campo: Campo; nome: string; tipo: Tipo; unidade?: string }[] = [
  { campo: "artista", nome: "Artista", tipo: "texto" },
  { campo: "genero", nome: "Gênero", tipo: "texto" },
  { campo: "album", nome: "Álbum", tipo: "texto" },
  { campo: "titulo", nome: "Título", tipo: "texto" },
  { campo: "ano", nome: "Ano", tipo: "numero" },
  { campo: "categoria", nome: "Categoria", tipo: "categoria" },
  { campo: "curtida", nome: "Curtida", tipo: "curtida" },
  { campo: "escutas", nome: "Vezes escutada", tipo: "numero" },
  { campo: "duracao_min", nome: "Duração", tipo: "numero", unidade: "min" },
  { campo: "adicionada_dias", nome: "Adicionada nos últimos", tipo: "dias" },
  { campo: "tocada_dias", nome: "Tocada nos últimos", tipo: "dias" },
  { campo: "nao_tocada_dias", nome: "Não tocada nos últimos", tipo: "dias" },
];
const OPS: Record<Tipo, [Op, string][]> = {
  texto: [
    ["contem", "contém"],
    ["nao_contem", "não contém"],
    ["igual", "é"],
    ["diferente", "não é"],
  ],
  numero: [
    ["maior_igual", "pelo menos"],
    ["menor_igual", "no máximo"],
    ["igual", "igual a"],
  ],
  categoria: [
    ["igual", "é"],
    ["diferente", "não é"],
  ],
  curtida: [["igual", "é"]],
  dias: [["igual", ""]],
};
const ORDENS: [Ordem, string][] = [
  ["aleatoria", "Embaralhada"],
  ["mais_tocadas", "Mais tocadas primeiro"],
  ["tocada_recente", "Tocadas recentemente primeiro"],
  ["adicionada_recente", "Adicionadas recentemente primeiro"],
  ["artista", "Artista"],
  ["titulo", "Título"],
  ["ano_recente", "Ano (mais novas primeiro)"],
];

const tipoDe = (c: Campo) => CAMPOS.find((x) => x.campo === c)!.tipo;
const valorPadrao = (t: Tipo) => (t === "curtida" ? "sim" : t === "dias" ? "30" : "");

/** Cria ou edita uma playlist inteligente: nome, regras, limite e ordem, com a contagem ao vivo. */
export default function EditorInteligente({
  id,
  nomeInicial,
  onFechar,
  onSalvo,
  onErro,
}: {
  id: number | null;
  nomeInicial?: string;
  onFechar: () => void;
  onSalvo: (id: number) => void;
  onErro: (m: string) => void;
}) {
  const [nome, setNome] = useState(nomeInicial ?? "");
  const [regras, setRegras] = useState<Regras>({
    combinar: "todas",
    regras: [{ campo: "artista", op: "contem", valor: "" }],
    limite: null,
    ordem: "aleatoria",
  });
  const [categorias, setCategorias] = useState<string[]>([]);
  const [previa, setPrevia] = useState<{ n: number } | { erro: string } | null>(null);
  const [salvando, setSalvando] = useState(false);

  useEffect(() => {
    if (id !== null)
      invoke<Regras | null>("smart_playlist_rules", { id })
        .then((r) => r && setRegras(r))
        .catch((e) => onErro(String(e)));
    invoke<{ nome: string }[]>("categories_list")
      .then((c) => setCategorias(c.map((x) => x.nome)))
      .catch(() => {});
  }, [id]);

  // Contagem ao vivo (espera a pessoa parar de digitar).
  useEffect(() => {
    const t = window.setTimeout(() => {
      invoke<number>("smart_playlist_preview", { regras })
        .then((n) => setPrevia({ n }))
        .catch((e) => setPrevia({ erro: String(e) }));
    }, 300);
    return () => window.clearTimeout(t);
  }, [regras]);

  const mudarRegra = (i: number, r: Partial<Regra>) =>
    setRegras((x) => ({ ...x, regras: x.regras.map((v, j) => (j === i ? { ...v, ...r } : v)) }));

  function mudarCampo(i: number, campo: Campo) {
    const t = tipoDe(campo);
    mudarRegra(i, { campo, op: OPS[t][0][0], valor: valorPadrao(t) });
  }

  async function salvar() {
    setSalvando(true);
    try {
      const novo = await invoke<number>("smart_playlist_save", { id, nome: nome.trim() || "Playlist inteligente", regras });
      onSalvo(novo);
    } catch (e) {
      onErro(String(e));
    } finally {
      setSalvando(false);
    }
  }

  return (
    <div className="fundo-modal" onMouseDown={(e) => e.target === e.currentTarget && onFechar()}>
      <section className="modal editor-inteligente" role="dialog" aria-modal="true" aria-label="Playlist inteligente">
        <header>
          <h2>{id === null ? "Nova playlist inteligente" : "Editar regras"}</h2>
          <button className="fechar-modal" onClick={onFechar} aria-label="Fechar">
            ×
          </button>
        </header>
        <label className="ei-nome">
          Nome
          <input autoFocus value={nome} maxLength={200} placeholder="Ex.: Samba dos anos 70" onChange={(e) => setNome(e.target.value)} />
        </label>
        <p className="ei-combinar">
          Entram as músicas que atendem a{" "}
          <select
            value={regras.combinar}
            aria-label="Combinar regras"
            onChange={(e) => setRegras((x) => ({ ...x, combinar: e.target.value as Regras["combinar"] }))}
          >
            <option value="todas">todas as regras</option>
            <option value="qualquer">qualquer regra</option>
          </select>
          :
        </p>
        <ul className="ei-regras">
          {regras.regras.map((r, i) => {
            const t = tipoDe(r.campo);
            const unidade = CAMPOS.find((c) => c.campo === r.campo)?.unidade;
            return (
              <li key={i}>
                <select value={r.campo} aria-label="Campo" onChange={(e) => mudarCampo(i, e.target.value as Campo)}>
                  {CAMPOS.map((c) => (
                    <option key={c.campo} value={c.campo}>
                      {c.nome}
                    </option>
                  ))}
                </select>
                {OPS[t].length > 1 && (
                  <select value={r.op} aria-label="Comparação" onChange={(e) => mudarRegra(i, { op: e.target.value as Op })}>
                    {OPS[t].map(([op, n]) => (
                      <option key={op} value={op}>
                        {n}
                      </option>
                    ))}
                  </select>
                )}
                {t === "curtida" ? (
                  <select value={r.valor} aria-label="Valor" onChange={(e) => mudarRegra(i, { valor: e.target.value })}>
                    <option value="sim">sim</option>
                    <option value="nao">não</option>
                  </select>
                ) : t === "categoria" ? (
                  <select value={r.valor} aria-label="Categoria" onChange={(e) => mudarRegra(i, { valor: e.target.value })}>
                    <option value="">Escolha…</option>
                    {categorias.map((c) => (
                      <option key={c} value={c}>
                        {c}
                      </option>
                    ))}
                  </select>
                ) : (
                  <input
                    className={t === "texto" ? "ei-texto" : "ei-numero"}
                    inputMode={t === "texto" ? undefined : "decimal"}
                    value={r.valor}
                    aria-label="Valor"
                    onChange={(e) => mudarRegra(i, { valor: e.target.value })}
                  />
                )}
                {t === "dias" && <span className="detalhe">dias</span>}
                {unidade && <span className="detalhe">{unidade}</span>}
                <button
                  className="icone-botao"
                  aria-label="Tirar regra"
                  title="Tirar regra"
                  onClick={() => setRegras((x) => ({ ...x, regras: x.regras.filter((_, j) => j !== i) }))}
                >
                  <X size={16} />
                </button>
              </li>
            );
          })}
        </ul>
        <button
          className="ei-mais"
          onClick={() => setRegras((x) => ({ ...x, regras: [...x.regras, { campo: "genero", op: "contem", valor: "" }] }))}
        >
          <Plus size={16} aria-hidden="true" /> Regra
        </button>
        <div className="ei-opcoes">
          <label>
            Ordem{" "}
            <select value={regras.ordem} onChange={(e) => setRegras((x) => ({ ...x, ordem: e.target.value as Ordem }))}>
              {ORDENS.map(([o, n]) => (
                <option key={o} value={o}>
                  {n}
                </option>
              ))}
            </select>
          </label>
          <label>
            No máximo{" "}
            <input
              className="ei-numero"
              inputMode="numeric"
              placeholder="todas"
              value={regras.limite ?? ""}
              onChange={(e) => {
                const n = parseInt(e.target.value.replace(/\D/g, ""), 10);
                setRegras((x) => ({ ...x, limite: Number.isFinite(n) && n > 0 ? n : null }));
              }}
            />{" "}
            músicas
          </label>
        </div>
        <footer>
          <span className="nota ei-previa">
            {previa === null
              ? "…"
              : "erro" in previa
                ? previa.erro
                : `${previa.n} ${previa.n === 1 ? "música entra" : "músicas entram"} agora. A lista se atualiza sozinha.`}
          </span>
          <button onClick={onFechar}>Cancelar</button>
          <button className="primario" onClick={salvar} disabled={salvando || (previa !== null && "erro" in previa)}>
            Salvar
          </button>
        </footer>
      </section>
    </div>
  );
}
