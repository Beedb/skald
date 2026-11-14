/** Temas de docs/temas.css: o atributo `data-tema` no :root escolhe as variáveis de cor. */

export type ModoTema = "claro" | "escuro" | "sistema";

/** Famílias com uma variante só (o modo não muda nada). */
const UNICOS: Record<string, string> = {
  carmesim: "Carmesim",
  brasa: "Brasa",
  noite: "Noite",
  papel: "Papel",
  vidro: "Vidro",
};

/** Famílias com variante clara e escura (`<id>-claro` / `<id>-escuro`). */
const DUPLOS: Record<string, string> = {
  ametista: "Ametista",
  maca: "Maçã",
  lavanda: "Lavanda",
  yinyang: "Yin-yang",
  pessego: "Pêssego",
  mono: "Mono",
  aurora: "Aurora",
  fiorde: "Fiorde",
  rosa: "Rosa",
  rubi: "Rubi",
  oceano: "Oceano",
  floresta: "Floresta",
  cafe: "Café",
  synthwave: "Synthwave",
  terminal: "Terminal",
  solar: "Solar",
  artico: "Ártico",
  vinho: "Vinho",
  areia: "Areia",
  grafite: "Grafite",
  menta: "Menta",
  contraste: "Alto contraste",
};

export const TEMA_PADRAO = "carmesim";

/** Nomes antigos de temas (trocados por nomes próprios); o salvo passa para o novo. */
const RENOMEADOS: Record<string, string> = { catppuccin: "ametista", nord: "fiorde", yotsuba: "pessego" };

export function temaAtual(tema: string): string {
  return RENOMEADOS[tema] ?? tema;
}

export const TEMAS: { id: string; nome: string; duplo: boolean }[] = [
  ...Object.entries(UNICOS).map(([id, nome]) => ({ id, nome, duplo: false })),
  ...Object.entries(DUPLOS).map(([id, nome]) => ({ id, nome, duplo: true })),
];

export function temDuasVariantes(tema: string) {
  return tema in DUPLOS;
}

const escuroNoSistema = () => window.matchMedia("(prefers-color-scheme: dark)").matches;

/** Valor final de `data-tema` para a família e o modo escolhidos. */
export function resolverTema(tema: string, modo: ModoTema): string {
  tema = temaAtual(tema);
  if (tema in UNICOS) return tema;
  if (!(tema in DUPLOS)) return TEMA_PADRAO;
  const escuro = modo === "escuro" || (modo === "sistema" && escuroNoSistema());
  return `${tema}-${escuro ? "escuro" : "claro"}`;
}

const CHAVE = "skald-tema";
let atual: { tema: string; modo: ModoTema } = { tema: TEMA_PADRAO, modo: "sistema" };

/** Aplica no :root e guarda uma cópia local para a próxima abertura já começar no tema certo. */
export function aplicarTema(tema: string, modo: ModoTema) {
  atual = { tema, modo };
  document.documentElement.dataset.tema = resolverTema(tema, modo);
  // As cores dinâmicas da capa recalculam o contraste com o novo fundo.
  window.dispatchEvent(new Event("tema-mudou"));
  try {
    localStorage.setItem(CHAVE, JSON.stringify(atual));
  } catch {
    /* sem armazenamento local: o tema vem das configurações ao carregar */
  }
}

/** Chamado antes do primeiro desenho: usa a cópia local e passa a seguir o sistema. */
export function iniciarTema() {
  try {
    const salvo = JSON.parse(localStorage.getItem(CHAVE) ?? "null");
    if (salvo?.tema) atual = salvo;
  } catch {
    /* ignora cópia ilegível */
  }
  aplicarTema(atual.tema, atual.modo);
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (atual.modo === "sistema") aplicarTema(atual.tema, atual.modo);
  });
}
