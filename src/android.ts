/** Ponte com o Android (MainActivity.kt). Fora do Android `window.SkaldAndroid` não existe. */
interface PonteAndroid {
  temPermissao(): boolean;
  pedirPermissao(): void;
  temAcessoTotal(): boolean;
  pedirAcessoTotal(): void;
  pastaMusica(): string;
}

const ponte = (window as unknown as { SkaldAndroid?: PonteAndroid }).SkaldAndroid;

export const noAndroid = ponte !== undefined;

function esperar(evento: string, pedir: () => void): Promise<boolean> {
  return new Promise((resolve) => {
    const aoResponder = (e: Event) => {
      window.removeEventListener(evento, aoResponder);
      resolve(Boolean((e as CustomEvent<boolean>).detail));
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

/** Pasta Music do aparelho; no Android o seletor de pastas devolve content://, que não serve. */
export function pastaMusica(): string | null {
  return ponte ? ponte.pastaMusica() : null;
}
