import { invoke } from "@tauri-apps/api/core";

/**
 * Roda `trabalho` mantendo o app acordado em segundo plano (no Android, notificação do
 * SyncService; sem ela a Samsung congela o processo e a rede cai). `avisar` troca o texto
 * da notificação. Nas outras plataformas só roda o trabalho.
 */
export async function emSegundoPlano<T>(texto: string, trabalho: (avisar: (t: string) => void) => Promise<T>): Promise<T> {
  const id = await invoke<number>("long_task_start", { texto }).catch(() => null);
  let ultimo = 0;
  const avisar = (t: string) => {
    // A notificação não precisa mudar mais que uma vez por segundo.
    if (Date.now() - ultimo < 1000) return;
    ultimo = Date.now();
    invoke("long_task_text", { texto: t }).catch(() => {});
  };
  try {
    return await trabalho(avisar);
  } finally {
    if (id !== null) invoke("long_task_end", { id }).catch(() => {});
  }
}
