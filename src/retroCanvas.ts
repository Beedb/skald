import type { Item } from "./Estatisticas";

/** Dados que o desenho precisa (os mesmos da composição na tela). */
export type DadosRetro = {
  titulo: string;
  tempo: string;
  escutadas: string;
  dominante: { nome: string; detalhe: string; capa: string | null } | null;
  repetida: { titulo: string; detalhe: string } | null;
  genero: { nome: string; detalhe: string } | null;
  artistas: Item[];
  musicas: Item[];
  albuns: Item[];
};

const L = 1080; // largura do PNG
const M = 64; // margem
const G = 24; // espaço entre blocos
const COL = (L - 2 * M - G) / 2;
const COR = {
  fundo: "#0d0f14",
  bloco: "#1a1e27",
  destaque: "#24365e",
  texto: "#f2f2f2",
  suave: "#b9c0cf",
  rotulo: "#9aa3b5",
  marca: "#7aa7ff",
};
const FONTE = '"Cantarell", "Noto Sans", "DejaVu Sans", sans-serif';

function carregar(src: string | null): Promise<HTMLImageElement | null> {
  if (!src) return Promise.resolve(null);
  return new Promise((ok) => {
    const img = new Image();
    img.onload = () => ok(img);
    img.onerror = () => ok(null);
    img.src = src;
  });
}

/** Corta o texto com "…" para caber em `max` px. */
function caber(ctx: CanvasRenderingContext2D, texto: string, max: number) {
  if (ctx.measureText(texto).width <= max) return texto;
  let t = texto;
  while (t.length > 1 && ctx.measureText(t + "…").width > max) t = t.slice(0, -1);
  return t.trimEnd() + "…";
}

/** Retângulo arredondado (com alternativa se o WebKit não tiver `roundRect`). */
function arredondado(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  if (typeof ctx.roundRect === "function") {
    ctx.roundRect(x, y, w, h, r);
    return;
  }
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function retangulo(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number, cor: string) {
  ctx.fillStyle = cor;
  arredondado(ctx, x, y, w, h, r);
  ctx.fill();
}

function texto(
  ctx: CanvasRenderingContext2D,
  t: string,
  x: number,
  y: number,
  max: number,
  tamanho: number,
  cor: string,
  peso = 400,
) {
  ctx.font = `${peso} ${tamanho}px ${FONTE}`;
  ctx.fillStyle = cor;
  ctx.fillText(caber(ctx, t, max), x, y);
}

/** Capa quadrada (recorte central) ou um quadrado com ♪ se não houver. */
function capa(ctx: CanvasRenderingContext2D, img: HTMLImageElement | null, x: number, y: number, lado: number) {
  ctx.save();
  arredondado(ctx, x, y, lado, lado, 8);
  ctx.clip();
  if (img) {
    const s = Math.min(img.naturalWidth, img.naturalHeight);
    ctx.drawImage(img, (img.naturalWidth - s) / 2, (img.naturalHeight - s) / 2, s, s, x, y, lado, lado);
  } else {
    ctx.fillStyle = "#2a3040";
    ctx.fillRect(x, y, lado, lado);
    ctx.fillStyle = COR.rotulo;
    ctx.font = `${lado / 2}px ${FONTE}`;
    ctx.textAlign = "center";
    ctx.fillText("♪", x + lado / 2, y + lado * 0.68);
    ctx.textAlign = "left";
  }
  ctx.restore();
}

const ALT_LINHA_IMG = 76;
const ALT_LINHA = 52;
const CAB_LISTA = 56;

/** Desenha uma lista dentro de um bloco; devolve a altura usada. */
function lista(
  ctx: CanvasRenderingContext2D,
  titulo: string,
  itens: Item[],
  imagens: (HTMLImageElement | null)[] | null,
  x: number,
  y: number,
  desenhar: boolean,
) {
  const linha = imagens ? ALT_LINHA_IMG : ALT_LINHA;
  const altura = CAB_LISTA + itens.length * linha + 16;
  if (!desenhar) return altura;
  retangulo(ctx, x, y, COL, altura, 14, COR.bloco);
  texto(ctx, titulo.toUpperCase(), x + 24, y + 38, COL - 48, 18, COR.rotulo, 600);
  itens.forEach((it, i) => {
    const ly = y + CAB_LISTA + i * linha;
    texto(ctx, String(i + 1), x + 24, ly + linha / 2 + 8, 30, 22, COR.marca, 700);
    let tx = x + 60;
    if (imagens) {
      capa(ctx, imagens[i], tx, ly + 6, linha - 12);
      tx += linha + 4;
    }
    const vezes = `${it.escutadas.toLocaleString("pt-BR")}×`;
    ctx.font = `400 20px ${FONTE}`;
    const largVezes = ctx.measureText(vezes).width;
    const maxNome = x + COL - 24 - largVezes - 16 - tx;
    if (it.detalhe) {
      texto(ctx, it.nome, tx, ly + linha / 2 - 2, maxNome, 22, COR.texto, 600);
      texto(ctx, it.detalhe, tx, ly + linha / 2 + 22, maxNome, 18, COR.rotulo);
    } else {
      texto(ctx, it.nome, tx, ly + linha / 2 + 8, maxNome, 22, COR.texto, 600);
    }
    texto(ctx, vezes, x + COL - 24 - largVezes, ly + linha / 2 + 8, largVezes + 1, 20, COR.suave);
  });
  return altura;
}

/** Desenha a retrospectiva e devolve o PNG como data URL. */
export async function gerarPng(d: DadosRetro): Promise<string> {
  const [imgDominante, ...resto] = await Promise.all([
    carregar(d.dominante?.capa ?? null),
    ...d.artistas.map((a) => carregar(a.capa ?? null)),
    ...d.albuns.map((a) => carregar(a.capa ?? null)),
  ]);
  const imgArtistas = resto.slice(0, d.artistas.length);
  const imgAlbuns = resto.slice(d.artistas.length);

  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d")!;

  // Medidas antes de desenhar (a altura depende das listas).
  const DEST = 170;
  const destaques = [d.dominante, d.repetida, d.genero].filter(Boolean).length + 1;
  const linhasDest = Math.ceil(destaques / 2);
  const topoListas = M + 150 + linhasDest * (DEST + G);
  const esquerda =
    (d.artistas.length ? lista(ctx, "", d.artistas, [], 0, 0, false) + G : 0) +
    (d.albuns.length ? lista(ctx, "", d.albuns, [], 0, 0, false) : 0);
  const direita = d.musicas.length ? lista(ctx, "", d.musicas, null, 0, 0, false) : 0;
  canvas.width = L;
  canvas.height = topoListas + Math.max(esquerda, direita) + M + 30;

  ctx.fillStyle = COR.fundo;
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.textBaseline = "alphabetic";

  texto(ctx, "SKÁLD · RETROSPECTIVA", M, M + 20, L - 2 * M, 20, COR.marca, 600);
  texto(ctx, d.titulo, M, M + 96, L - 2 * M, 64, COR.texto, 800);

  // Destaques em grade de 2 colunas.
  const blocos: ((x: number, y: number) => void)[] = [
    (x, y) => {
      retangulo(ctx, x, y, COL, DEST, 14, COR.destaque);
      texto(ctx, d.tempo, x + 28, y + 82, COL - 56, 46, COR.texto, 800);
      texto(ctx, `de música · ${d.escutadas} escutadas`, x + 28, y + 124, COL - 56, 22, COR.suave);
    },
  ];
  if (d.dominante) {
    const dom = d.dominante;
    blocos.push((x, y) => {
      retangulo(ctx, x, y, COL, DEST, 14, COR.bloco);
      capa(ctx, imgDominante, x + 24, y + 24, DEST - 48);
      const tx = x + DEST;
      texto(ctx, "ARTISTA DOMINANTE", tx, y + 50, COL - DEST - 24, 16, COR.rotulo, 600);
      texto(ctx, dom.nome, tx, y + 92, COL - DEST - 24, 30, COR.texto, 700);
      texto(ctx, dom.detalhe, tx, y + 126, COL - DEST - 24, 20, COR.suave);
    });
  }
  for (const [rotulo, b] of [
    ["MÚSICA MAIS REPETIDA", d.repetida && { nome: d.repetida.titulo, detalhe: d.repetida.detalhe }],
    ["GÊNERO PREDOMINANTE", d.genero],
  ] as const) {
    if (!b) continue;
    blocos.push((x, y) => {
      retangulo(ctx, x, y, COL, DEST, 14, COR.bloco);
      texto(ctx, rotulo, x + 28, y + 50, COL - 56, 16, COR.rotulo, 600);
      texto(ctx, b.nome, x + 28, y + 92, COL - 56, 30, COR.texto, 700);
      texto(ctx, b.detalhe, x + 28, y + 126, COL - 56, 20, COR.suave);
    });
  }
  blocos.forEach((desenhar, i) => desenhar(M + (i % 2) * (COL + G), M + 150 + Math.floor(i / 2) * (DEST + G)));

  // Listas: artistas e álbuns à esquerda, músicas à direita.
  let y = topoListas;
  if (d.artistas.length) y += lista(ctx, "Top 5 artistas", d.artistas, imgArtistas, M, y, true) + G;
  if (d.albuns.length) lista(ctx, "Top 5 álbuns", d.albuns, imgAlbuns, M, y, true);
  if (d.musicas.length) lista(ctx, "Top 10 músicas", d.musicas, null, M + COL + G, topoListas, true);

  texto(ctx, "Gerado pelo Skáld", M, canvas.height - M / 2, L - 2 * M, 16, COR.rotulo);
  return canvas.toDataURL("image/png");
}
