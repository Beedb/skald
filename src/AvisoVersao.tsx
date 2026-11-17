import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

type NovaVersao = { versao: string; url: string };

/** Versão cujo aviso a pessoa fechou: só volta a avisar quando sair outra. */
const CHAVE = "skald-versao-dispensada";

function dispensada(): string | null {
  try {
    return localStorage.getItem(CHAVE);
  } catch {
    return null;
  }
}

/**
 * Faixa "Saiu a versão X" no topo. O comando só consulta o GitHub com a opção
 * ligada (Configurações → Novidades); sem internet não mostra nada.
 */
export default function AvisoVersao() {
  const [nova, setNova] = useState<NovaVersao | null>(null);

  useEffect(() => {
    const verificar = () =>
      invoke<NovaVersao | null>("update_check")
        .then((v) => setNova(v && v.versao !== dispensada() ? v : null))
        .catch(() => {});
    verificar();
    // Ligar a opção em Configurações verifica na hora.
    window.addEventListener("verificar-versao", verificar);
    return () => window.removeEventListener("verificar-versao", verificar);
  }, []);

  if (!nova) return null;

  function fechar() {
    try {
      localStorage.setItem(CHAVE, nova!.versao);
    } catch {
      // sem armazenamento: o aviso só some até reabrir
    }
    setNova(null);
  }

  return (
    <p className="aviso" role="status">
      Saiu a versão {nova.versao} do Skáld.{" "}
      <button className="link" onClick={() => openUrl(nova.url).catch(() => {})}>
        Ver novidades e baixar
      </button>{" "}
      <button className="fechar-aviso" onClick={fechar} aria-label="Fechar aviso" title="Fechar aviso">
        ✕
      </button>
    </p>
  );
}
