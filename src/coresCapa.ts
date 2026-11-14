/**
 * Cores dinâmicas da capa (seção 3.2): extrai a cor predominante da capa da música atual e
 * a aplica como acento (`--acento`, `--acento-txt`) e cor de fundo do player (`--capa`),
 * sobrescrevendo o tema. A cor é ajustada até ter contraste suficiente com o fundo do tema.
 * A transição é gradual (`@property` em App.css).
 */

type RGB = [number, number, number];

const PROPRIEDADES = ["--acento", "--acento-txt", "--capa"];

function rgbParaHsl([r, g, b]: RGB): [number, number, number] {
  r /= 255;
  g /= 255;
  b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, s, l];
}

function hslParaRgb(h: number, s: number, l: number): RGB {
  const k = (n: number) => (n + h / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => l - a * Math.max(-1, Math.min(k(n) - 3, 9 - k(n), 1));
  return [f(0), f(8), f(4)].map((v) => Math.round(v * 255)) as RGB;
}

function luminancia([r, g, b]: RGB) {
  const c = (v: number) => {
    v /= 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * c(r) + 0.7152 * c(g) + 0.0722 * c(b);
}

export function contraste(a: RGB, b: RGB) {
  const [x, y] = [luminancia(a), luminancia(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

/** Converte qualquer cor CSS (inclusive variáveis já resolvidas) em RGB. */
function corCss(valor: string): RGB | null {
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx || !valor) return null;
  ctx.fillStyle = valor;
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
  return a === 0 ? null : [r, g, b];
}

/** Cor predominante "viva": agrupa os pixels em caixas de cor e pontua por quantidade e saturação. */
export function corPredominante(img: HTMLImageElement): RGB | null {
  const lado = 40;
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = lado;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  ctx.drawImage(img, 0, 0, lado, lado);
  const dados = ctx.getImageData(0, 0, lado, lado).data;
  const caixas = new Map<number, { n: number; r: number; g: number; b: number }>();
  for (let i = 0; i < dados.length; i += 4) {
    const [r, g, b] = [dados[i], dados[i + 1], dados[i + 2]];
    const chave = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4);
    const c = caixas.get(chave) ?? { n: 0, r: 0, g: 0, b: 0 };
    c.n++;
    c.r += r;
    c.g += g;
    c.b += b;
    caixas.set(chave, c);
  }
  let melhor: RGB | null = null;
  let pontos = -1;
  let maisComum: RGB | null = null;
  let maior = 0;
  for (const c of caixas.values()) {
    const cor: RGB = [c.r / c.n, c.g / c.n, c.b / c.n];
    if (c.n > maior) {
      maior = c.n;
      maisComum = cor;
    }
    const [, s, l] = rgbParaHsl(cor);
    if (l < 0.12 || l > 0.92) continue; // quase preto ou branco não serve de acento
    const p = c.n * (0.3 + s) ** 2;
    if (p > pontos) {
      pontos = p;
      melhor = cor;
    }
  }
  return melhor ?? maisComum;
}

/** Ajusta a luminosidade até a cor ter contraste mínimo com o fundo. */
export function ajustarContraste(cor: RGB, fundo: RGB, minimo = 3): RGB {
  const [h, s0, l0] = rgbParaHsl(cor);
  const s = Math.min(s0, 0.85);
  const fundoEscuro = luminancia(fundo) < 0.2;
  let l = l0;
  for (let i = 0; i < 40; i++) {
    const c = hslParaRgb(h, s, l);
    if (contraste(c, fundo) >= minimo) return c;
    l = fundoEscuro ? Math.min(1, l + 0.025) : Math.max(0, l - 0.025);
  }
  return hslParaRgb(h, s, l);
}

const css = (c: RGB) => `rgb(${c.map(Math.round).join(" ")})`;

/** Valor de uma variável do tema (`--fundo` e `--forte` nunca são sobrescritas aqui). */
function variavelDoTema(nome: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(nome).trim();
}

let corAtual: RGB | null = null;

/** Aplica (ou, com `null`, remove) as cores tiradas da capa. */
export function aplicarCorCapa(cor: RGB | null) {
  corAtual = cor;
  const raiz = document.documentElement;
  if (!cor) {
    PROPRIEDADES.forEach((p) => raiz.style.removeProperty(p));
    return;
  }
  const fundo = corCss(variavelDoTema("--fundo")) ?? [0, 0, 0];
  const acento = ajustarContraste(cor, fundo);
  // Texto sobre o acento: a cor do tema (fundo ou forte) que contrasta mais.
  const forte = corCss(variavelDoTema("--forte")) ?? [255, 255, 255];
  const txt = contraste(acento, fundo) >= contraste(acento, forte) ? "var(--fundo)" : "var(--forte)";
  raiz.style.setProperty("--acento", css(acento));
  raiz.style.setProperty("--acento-txt", txt);
  raiz.style.setProperty("--capa", css(cor));
}

/** Recalcula o contraste depois de trocar de tema. */
export function reaplicarCorCapa() {
  aplicarCorCapa(corAtual);
}

/** Lê a cor predominante de uma imagem (data URL). */
export function corDaImagem(url: string): Promise<RGB | null> {
  return new Promise((ok) => {
    const img = new Image();
    img.onload = () => ok(corPredominante(img));
    img.onerror = () => ok(null);
    img.src = url;
  });
}
