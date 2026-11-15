/** Ponte com o Android (MainActivity.kt). Fora do Android `window.SkaldAndroid` não existe. */
interface PonteAndroid {
  temPermissao(): boolean;
  pedirPermissao(): void;
  temAcessoTotal(): boolean;
  pedirAcessoTotal(): void;
  pastaMusica(): string;
  insets(): string;
  escolherPasta(): void;
}

const ponte = (window as unknown as { SkaldAndroid?: PonteAndroid }).SkaldAndroid;

export const noAndroid = ponte !== undefined;

function esperar(evento: string, pedir: () => void): Promise<boolean> {
  return esperarValor<boolean>(evento, pedir).then(Boolean);
}

function esperarValor<T>(evento: string, pedir: () => void): Promise<T> {
  return new Promise((resolve) => {
    const aoResponder = (e: Event) => {
      window.removeEventListener(evento, aoResponder);
      resolve((e as CustomEvent<T>).detail);
    };
    window.addEventListener(evento, aoResponder);
    pedir();
  });
}

/**
 * Acesso às músicas. "total" = todos os arquivos (lê também os .lrc ao lado das músicas);
 * "audio" = só arquivos de mídia (o sistema esconde os .lrc); false = sem acesso.
 * Tenta o acesso total primeiro; recusado, pede só o de áudio.
 */
export async function garantirPermissao(): Promise<"total" | "audio" | false> {
  if (!ponte) return "total";
  if (ponte.temAcessoTotal()) return "total";
  if (await esperar("skald-acesso-total", () => ponte.pedirAcessoTotal())) return "total";
  if (ponte.temPermissao()) return "audio";
  if (await esperar("skald-permissao", () => ponte.pedirPermissao())) return "audio";
  return false;
}

/** Seletor de pastas do Android: devolve o caminho no aparelho, ou null se cancelado ou se a
 *  pasta não tiver caminho (ex.: nuvem). */
export function escolherPasta(): Promise<string | null> {
  if (!ponte) return Promise.resolve(null);
  return esperarValor<string | null>("skald-pasta", () => ponte.escolherPasta());
}

/** Pasta Music do aparelho; no Android o seletor de pastas devolve content://, que não serve. */
export function pastaMusica(): string | null {
  return ponte ? ponte.pastaMusica() : null;
}

/** Afasta a interface das barras do sistema: grava --seguro-topo/base/esquerda/direita no :root. */
export function acompanharMargens() {
  if (!ponte) return;
  const aplicar = () => {
    try {
      const m = JSON.parse(ponte.insets()) as Record<string, number>;
      for (const lado of ["topo", "base", "esquerda", "direita"]) {
        document.documentElement.style.setProperty(`--seguro-${lado}`, `${m[lado] ?? 0}px`);
      }
    } catch {
      /* sem margens: segue sem afastar */
    }
  };
  aplicar();
  window.addEventListener("skald-insets", aplicar);
}
