/** Ponte com o Android (MainActivity.kt). Fora do Android `window.SkaldAndroid` não existe. */
interface PonteAndroid {
  temPermissao(): boolean;
  pedirPermissao(): void;
  pastaMusica(): string;
}

const ponte = (window as unknown as { SkaldAndroid?: PonteAndroid }).SkaldAndroid;

export const noAndroid = ponte !== undefined;

/** Garante a permissão de ler músicas, mostrando o pedido do sistema se preciso. */
export function garantirPermissao(): Promise<boolean> {
  if (!ponte) return Promise.resolve(true);
  if (ponte.temPermissao()) return Promise.resolve(true);
  return new Promise((resolve) => {
    const aoResponder = (e: Event) => {
      window.removeEventListener("skald-permissao", aoResponder);
      resolve(Boolean((e as CustomEvent<boolean>).detail));
    };
    window.addEventListener("skald-permissao", aoResponder);
    ponte.pedirPermissao();
  });
}

/** Pasta Music do aparelho; no Android o seletor de pastas devolve content://, que não serve. */
export function pastaMusica(): string | null {
  return ponte ? ponte.pastaMusica() : null;
}
