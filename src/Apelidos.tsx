import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Apelidos de artistas (seção 11.3): grafias diferentes contam como um artista só na
 * coleção e nas estatísticas. As tags dos arquivos não mudam.
 */

type Apelido = { alias: string; canonical: string };

export default function Apelidos({
  nomes,
  onFechar,
  onMudou,
  onErro,
}: {
  nomes: string[];
  onFechar: () => void;
  onMudou: () => void;
  onErro: (m: string) => void;
}) {
  const [lista, setLista] = useState<Apelido[]>([]);
  const [sugestoes, setSugestoes] = useState<string[][]>([]);
  const [principalSugestao, setPrincipalSugestao] = useState<Record<number, string>>({});
  const [alias, setAlias] = useState("");
  const [principal, setPrincipal] = useState("");
  const [aviso, setAviso] = useState<string | null>(null);

  const carregar = () => {
    invoke<Apelido[]>("aliases_list")
      .then(setLista)
      .catch((e) => onErro(String(e)));
    invoke<string[][]>("alias_suggestions")
      .then((s) => {
        setSugestoes(s);
        setPrincipalSugestao({});
      })
      .catch((e) => onErro(String(e)));
  };

  useEffect(carregar, []);

  useEffect(() => {
    const tecla = (e: KeyboardEvent) => e.key === "Escape" && onFechar();
    window.addEventListener("keydown", tecla);
    return () => window.removeEventListener("keydown", tecla);
  }, [onFechar]);

  async function unir(apelidos: string[], nomePrincipal: string) {
    setAviso(null);
    try {
      for (const a of apelidos) await invoke("alias_add", { alias: a, canonical: nomePrincipal });
      carregar();
      onMudou();
      return true;
    } catch (e) {
      setAviso(String(e));
      return false;
    }
  }

  async function adicionar() {
    if (await unir([alias], principal)) {
      setAlias("");
      setPrincipal("");
    }
  }

  async function remover(a: string) {
    try {
      await invoke("alias_remove", { alias: a });
      carregar();
      onMudou();
    } catch (e) {
      onErro(String(e));
    }
  }

  return (
    <div className="fundo-modal" onMouseDown={(e) => e.target === e.currentTarget && onFechar()}>
      <section className="modal apelidos" role="dialog" aria-modal="true" aria-label="Apelidos de artistas">
        <header>
          <h2>Apelidos de artistas</h2>
          <button className="fechar-modal" onClick={onFechar} aria-label="Fechar">
            ×
          </button>
        </header>
        <p className="nota">
          Um apelido faz uma grafia contar como outro artista, na coleção e nas estatísticas (também no histórico antigo).
          As tags dos arquivos não mudam.
        </p>

        <datalist id="nomes-artistas">
          {nomes.map((n) => (
            <option key={n} value={n} />
          ))}
        </datalist>
        <div className="form-apelido">
          <input
            value={alias}
            onChange={(e) => setAlias(e.target.value)}
            placeholder="Grafia alternativa"
            list="nomes-artistas"
            aria-label="Grafia alternativa"
          />
          <span aria-hidden="true">→</span>
          <input
            value={principal}
            onChange={(e) => setPrincipal(e.target.value)}
            placeholder="Nome principal"
            list="nomes-artistas"
            aria-label="Nome principal"
            onKeyDown={(e) => e.key === "Enter" && alias.trim() && principal.trim() && adicionar()}
          />
          <button className="primario" onClick={adicionar} disabled={!alias.trim() || !principal.trim()}>
            Adicionar
          </button>
        </div>
        {aviso && <p className="aviso-modal">{aviso}</p>}

        {sugestoes.length > 0 && (
          <>
            <h3>Grafias parecidas</h3>
            <ul className="lista-apelidos">
              {sugestoes.map((g, i) => {
                const escolhido = principalSugestao[i] ?? g[0];
                return (
                  <li key={g.join("\u0000")}>
                    <span>{g.join(" · ")}</span>
                    <label>
                      contar como{" "}
                      <select value={escolhido} onChange={(e) => setPrincipalSugestao({ ...principalSugestao, [i]: e.target.value })}>
                        {g.map((n) => (
                          <option key={n} value={n}>
                            {n}
                          </option>
                        ))}
                      </select>
                    </label>
                    <button onClick={() => unir(g.filter((n) => n !== escolhido), escolhido)}>Unir</button>
                  </li>
                );
              })}
            </ul>
          </>
        )}

        <h3>Apelidos ({lista.length})</h3>
        {lista.length === 0 ? (
          <p className="nota">Nenhum apelido ainda.</p>
        ) : (
          <ul className="lista-apelidos">
            {lista.map((a) => (
              <li key={a.alias}>
                <span>
                  {a.alias} <span className="detalhe">→</span> <strong>{a.canonical}</strong>
                </span>
                <button onClick={() => remover(a.alias)} title="Voltar a contar separado">
                  Remover
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
