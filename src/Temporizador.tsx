import { Moon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Temporizador (seção 8.9): pausa depois de um tempo ou no fim da música atual. */

type Visao = { restante_ms: number | null; fim_da_faixa: boolean };

const OPCOES = [15, 30, 45, 60, 90];

function relogio(ms: number) {
  const s = Math.ceil(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const seg = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${seg}` : `${m}:${seg}`;
}

export default function Temporizador({ onErro }: { onErro: (m: string) => void }) {
  const [fimEm, setFimEm] = useState<number | null>(null); // Date.now() do fim
  const [fimDaFaixa, setFimDaFaixa] = useState(false);
  const [agora, setAgora] = useState(Date.now());
  const [aberto, setAberto] = useState(false);
  const caixa = useRef<HTMLDivElement>(null);

  const carregar = () =>
    invoke<Visao>("sleep_timer_get")
      .then((v) => {
        setFimEm(v.restante_ms !== null ? Date.now() + v.restante_ms : null);
        setFimDaFaixa(v.fim_da_faixa);
      })
      .catch((e) => onErro(String(e)));

  useEffect(() => {
    carregar();
    const parar = listen("temporizador-mudou", carregar);
    return () => {
      parar.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (fimEm === null) return;
    const t = setInterval(() => setAgora(Date.now()), 1000);
    return () => clearInterval(t);
  }, [fimEm]);

  // Fecha o menu ao clicar fora.
  useEffect(() => {
    if (!aberto) return;
    const fora = (e: MouseEvent) => {
      if (!caixa.current?.contains(e.target as Node)) setAberto(false);
    };
    document.addEventListener("mousedown", fora);
    return () => document.removeEventListener("mousedown", fora);
  }, [aberto]);

  const definir = (minutos: number | null, fimDaFaixa = false) => {
    setAberto(false);
    invoke("sleep_timer_set", { minutos, fimDaFaixa }).catch((e) => onErro(String(e)));
  };

  const ligado = fimEm !== null || fimDaFaixa;
  const rotulo = fimDaFaixa ? "fim da música" : fimEm !== null ? relogio(Math.max(0, fimEm - agora)) : null;

  return (
    <div className="temporizador" ref={caixa}>
      <button
        className={"icone-botao" + (ligado ? " ligado" : "")}
        onClick={() => setAberto((a) => !a)}
        aria-expanded={aberto}
        title={ligado ? `Temporizador: ${rotulo}` : "Temporizador"}
      >
        <Moon size={18} aria-hidden="true" />
        {rotulo && <span className="restante">{rotulo}</span>}
      </button>
      {aberto && (
        <div className="menu-temporizador" role="menu">
          <span className="titulo">Pausar a música</span>
          {OPCOES.map((m) => (
            <button key={m} role="menuitem" onClick={() => definir(m)}>
              Em {m} min
            </button>
          ))}
          <button role="menuitem" className={fimDaFaixa ? "atual" : undefined} onClick={() => definir(null, true)}>
            No fim desta música
          </button>
          {ligado && (
            <button role="menuitem" onClick={() => definir(null)}>
              Desligar temporizador
            </button>
          )}
        </div>
      )}
    </div>
  );
}
