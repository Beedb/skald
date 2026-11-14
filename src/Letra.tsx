import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Linha = { ms: number; texto: string };
type Letra =
  | { tipo: "sincronizada"; linhas: Linha[]; origem: string }
  | { tipo: "estatica"; texto: string; origem: string };

/** Índice da última linha que já começou (-1 antes da primeira). */
function linhaAtual(linhas: Linha[], posicao: number) {
  let ini = 0;
  let fim = linhas.length - 1;
  let achada = -1;
  while (ini <= fim) {
    const meio = (ini + fim) >> 1;
    if (linhas[meio].ms <= posicao) {
      achada = meio;
      ini = meio + 1;
    } else {
      fim = meio - 1;
    }
  }
  return achada;
}

type Props = {
  id: number | null;
  posicaoMs: number;
  onErro: (m: string) => void;
};

/** Letra da música atual: sincronizada (linha atual em destaque) ou estática. */
export default function Letra({ id, posicaoMs, onErro }: Props) {
  const [letra, setLetra] = useState<Letra | null | undefined>(undefined);
  const caixa = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (id === null) return setLetra(null);
    let valido = true;
    setLetra(undefined);
    invoke<Letra | null>("get_lyrics", { id })
      .then((l) => valido && setLetra(l))
      .catch((e) => valido && onErro(String(e)));
    return () => {
      valido = false;
    };
  }, [id]);

  const atual = letra?.tipo === "sincronizada" ? linhaAtual(letra.linhas, posicaoMs) : -1;

  // Mantém a linha atual no meio da caixa (sem rolar a página inteira).
  useEffect(() => {
    const c = caixa.current;
    const el = c?.querySelector<HTMLElement>(`[data-linha="${atual}"]`);
    if (!c || !el) return;
    c.scrollTo({ top: el.offsetTop - c.clientHeight / 2 + el.clientHeight / 2, behavior: "smooth" });
  }, [atual]);

  if (letra === undefined) return <p className="letra-vazia">Carregando a letra…</p>;
  if (letra === null)
    return (
      <p className="letra-vazia">
        Sem letra para esta música. Para letra sincronizada, coloque um arquivo <code>.lrc</code> com o mesmo nome
        da música na mesma pasta.
      </p>
    );

  return (
    <div className="letra" ref={caixa}>
      {letra.tipo === "sincronizada" ? (
        letra.linhas.map((l, i) => (
          <button
            key={i}
            data-linha={i}
            className={"linha-letra" + (i === atual ? " atual" : i < atual ? " passada" : "")}
            onClick={() => invoke("seek", { posicaoMs: l.ms }).catch((e) => onErro(String(e)))}
            title="Ir para este trecho"
          >
            {l.texto || "♪"}
          </button>
        ))
      ) : (
        <p className="letra-estatica">{letra.texto}</p>
      )}
      <p className="letra-origem">Fonte: {letra.origem}</p>
    </div>
  );
}
