import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Miniaturas já pedidas nesta sessão (id da música → data URL ou null se não tem capa). */
const cache = new Map<number, string | null>();
const pendentes = new Map<number, Promise<string | null>>();

export function miniatura(id: number): Promise<string | null> {
  if (cache.has(id)) return Promise.resolve(cache.get(id)!);
  let p = pendentes.get(id);
  if (!p) {
    p = invoke<string | null>("cover_thumb", { id })
      .catch(() => null)
      .then((url) => {
        cache.set(id, url);
        pendentes.delete(id);
        return url;
      });
    pendentes.set(id, p);
  }
  return p;
}

/** Capa pequena de uma música, carregada só quando aparece na tela. */
export default function Capa({ id, className = "" }: { id: number | null; className?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [url, setUrl] = useState<string | null>(id !== null ? (cache.get(id) ?? null) : null);

  useEffect(() => {
    if (id === null) return setUrl(null);
    if (cache.has(id)) return setUrl(cache.get(id)!);
    setUrl(null);
    const el = ref.current;
    if (!el) return;
    let valido = true;
    const obs = new IntersectionObserver((entradas) => {
      if (!entradas.some((e) => e.isIntersecting)) return;
      obs.disconnect();
      miniatura(id).then((u) => valido && setUrl(u));
    });
    obs.observe(el);
    return () => {
      valido = false;
      obs.disconnect();
    };
  }, [id]);

  return (
    <div ref={ref} className={"capa-colecao " + className}>
      {url ? <img src={url} alt="" draggable={false} /> : <span aria-hidden="true">♪</span>}
    </div>
  );
}
