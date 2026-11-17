//! Configurações (seção 12), guardadas na tabela `setting` como chave/valor.

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

const CROSSFADE: &str = "crossfade_ms";
const NORMALIZACAO: &str = "normalizacao";
const EQUALIZADOR: &str = "equalizador";
const TEMA: &str = "tema";
const MODO_TEMA: &str = "modo_tema";
const CORES_DINAMICAS: &str = "cores_dinamicas";
const BANDEJA: &str = "bandeja";
const NOTIFICAR: &str = "notificar";
const ALEATORIO: &str = "aleatorio";
const REPETIR: &str = "repetir";
const TEMPORIZADOR: &str = "temporizador";
const RETOMAR: &str = "retomar";
const CONTINUAR: &str = "continuar_tocando";
const VERIFICAR_VERSAO: &str = "verificar_versao";
/// Tema padrão do Skáld (docs/temas.css).
const TEMA_PADRAO: &str = "carmesim";
/// Crossfade padrão: ligado, 5 s (desligado entre faixas do mesmo álbum).
const CROSSFADE_PADRAO: u64 = 5000;
pub const CROSSFADE_MAX: u64 = 12_000;

#[derive(Serialize)]
pub struct Configuracoes {
    pub crossfade_ms: u64,
    /// ReplayGain: desligada, faixa (padrão) ou álbum.
    pub normalizacao: crate::ganho::Modo,
    pub equalizador: crate::eq::Preset,
    /// Família de cores de docs/temas.css (ex.: "carmesim", "fiorde").
    pub tema: String,
    /// "claro", "escuro" ou "sistema" (padrão).
    pub modo_tema: String,
    /// Acento e fundo do player tirados da capa atual (padrão: ligado).
    pub cores_dinamicas: bool,
    /// Fechar a janela envia para a bandeja (padrão: desligado, fechar encerra).
    pub bandeja: bool,
    /// Notificação ao trocar de música com a janela fora de foco (padrão: ligada).
    pub notificar: bool,
    /// No fim da fila ou playlist, tocar relacionadas (padrão: ligado).
    pub continuar_tocando: bool,
    /// Consultar o GitHub ao abrir para avisar de versão nova (padrão: desligado).
    pub verificar_versao: bool,
}

fn ler(conn: &Connection, chave: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM setting WHERE key = ?1", [chave], |r| r.get(0)).optional()
}

fn gravar(conn: &Connection, chave: &str, valor: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO setting (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [chave, valor],
    )?;
    Ok(())
}

pub fn carregar(conn: &Connection) -> rusqlite::Result<Configuracoes> {
    let crossfade_ms = ler(conn, CROSSFADE)?
        .and_then(|v| v.parse().ok())
        .unwrap_or(CROSSFADE_PADRAO)
        .min(CROSSFADE_MAX);
    let normalizacao = ler(conn, NORMALIZACAO)?
        .and_then(|v| crate::ganho::Modo::de_texto(&v))
        .unwrap_or_default();
    let equalizador = ler(conn, EQUALIZADOR)?
        .and_then(|v| crate::eq::Preset::de_texto(&v))
        .unwrap_or_default();
    let tema = ler(conn, TEMA)?.unwrap_or_else(|| TEMA_PADRAO.into());
    // Temas renomeados (nomes de outros projetos trocados por nomes próprios).
    let tema = match tema.as_str() {
        "catppuccin" => "ametista".into(),
        "nord" => "fiorde".into(),
        "yotsuba" => "pessego".into(),
        _ => tema,
    };
    let modo_tema = ler(conn, MODO_TEMA)?.unwrap_or_else(|| "sistema".into());
    let cores_dinamicas = ler(conn, CORES_DINAMICAS)?.is_none_or(|v| v == "1");
    let bandeja = ler(conn, BANDEJA)?.is_some_and(|v| v == "1");
    let notificar = ler(conn, NOTIFICAR)?.is_none_or(|v| v == "1");
    let continuar_tocando = ler(conn, CONTINUAR)?.is_none_or(|v| v == "1");
    let verificar_versao = verificar_versao(conn)?;
    Ok(Configuracoes {
        crossfade_ms,
        normalizacao,
        equalizador,
        tema,
        modo_tema,
        cores_dinamicas,
        bandeja,
        notificar,
        continuar_tocando,
        verificar_versao,
    })
}

/// Modos da fila guardados entre execuções.
pub fn modos_fila(conn: &Connection) -> rusqlite::Result<(bool, crate::queue::Repetir)> {
    let aleatorio = ler(conn, ALEATORIO)?.is_some_and(|v| v == "1");
    let repetir = ler(conn, REPETIR)?
        .and_then(|v| serde_json::from_value(serde_json::Value::String(v)).ok())
        .unwrap_or_default();
    Ok((aleatorio, repetir))
}

pub fn definir_aleatorio(conn: &Connection, ativo: bool) -> rusqlite::Result<()> {
    gravar(conn, ALEATORIO, if ativo { "1" } else { "0" })
}

pub fn definir_repetir(conn: &Connection, modo: crate::queue::Repetir) -> rusqlite::Result<()> {
    let texto = serde_json::to_value(modo).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    gravar(conn, REPETIR, &texto)
}

/// Temporizador guardado: `Fim` (fim da música) ou o instante final em ms desde 1970.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TemporizadorSalvo {
    FimDaFaixa,
    Ate(i64),
}

/// Fila, posição e origem da última reprodução (JSON), para retomar ao reabrir.
pub fn retomar(conn: &Connection) -> rusqlite::Result<Option<String>> {
    Ok(ler(conn, RETOMAR)?.filter(|v| !v.is_empty()))
}

pub fn definir_retomar(conn: &Connection, json: &str) -> rusqlite::Result<()> {
    gravar(conn, RETOMAR, json)
}

pub fn temporizador(conn: &Connection) -> rusqlite::Result<Option<TemporizadorSalvo>> {
    Ok(ler(conn, TEMPORIZADOR)?.and_then(|v| match v.as_str() {
        "fim" => Some(TemporizadorSalvo::FimDaFaixa),
        n => n.parse().ok().map(TemporizadorSalvo::Ate),
    }))
}

pub fn definir_temporizador(conn: &Connection, t: Option<TemporizadorSalvo>) -> rusqlite::Result<()> {
    let texto = match t {
        None => String::new(),
        Some(TemporizadorSalvo::FimDaFaixa) => "fim".into(),
        Some(TemporizadorSalvo::Ate(ms)) => ms.to_string(),
    };
    gravar(conn, TEMPORIZADOR, &texto)
}

pub fn definir_bandeja(conn: &Connection, ativo: bool) -> rusqlite::Result<()> {
    gravar(conn, BANDEJA, if ativo { "1" } else { "0" })
}

pub fn definir_continuar(conn: &Connection, ativo: bool) -> rusqlite::Result<()> {
    gravar(conn, CONTINUAR, if ativo { "1" } else { "0" })
}

pub fn verificar_versao(conn: &Connection) -> rusqlite::Result<bool> {
    Ok(ler(conn, VERIFICAR_VERSAO)?.is_some_and(|v| v == "1"))
}

pub fn definir_verificar_versao(conn: &Connection, ativo: bool) -> rusqlite::Result<()> {
    gravar(conn, VERIFICAR_VERSAO, if ativo { "1" } else { "0" })
}

pub fn definir_notificar(conn: &Connection, ativo: bool) -> rusqlite::Result<()> {
    gravar(conn, NOTIFICAR, if ativo { "1" } else { "0" })
}

pub fn definir_cores_dinamicas(conn: &Connection, ativo: bool) -> rusqlite::Result<()> {
    gravar(conn, CORES_DINAMICAS, if ativo { "1" } else { "0" })
}

/// Aparência (seção 12). O tema vira o atributo `data-tema`, então só aceita letras e hífen.
pub fn definir_aparencia(conn: &Connection, tema: &str, modo: &str) -> Result<(), String> {
    let valido = !tema.is_empty() && tema.len() <= 40 && tema.chars().all(|c| c.is_ascii_lowercase() || c == '-');
    if !valido {
        return Err("Tema inválido.".into());
    }
    if !["claro", "escuro", "sistema"].contains(&modo) {
        return Err("Modo de tema inválido.".into());
    }
    gravar(conn, TEMA, tema).map_err(|e| e.to_string())?;
    gravar(conn, MODO_TEMA, modo).map_err(|e| e.to_string())
}

pub fn definir_equalizador(conn: &Connection, preset: crate::eq::Preset) -> rusqlite::Result<()> {
    gravar(conn, EQUALIZADOR, &preset.texto())
}

pub fn definir_normalizacao(conn: &Connection, modo: crate::ganho::Modo) -> rusqlite::Result<()> {
    gravar(conn, NORMALIZACAO, modo.texto())
}

/// Grava e devolve o valor efetivo (limitado a `CROSSFADE_MAX`).
pub fn definir_crossfade(conn: &Connection, ms: u64) -> rusqlite::Result<u64> {
    let ms = ms.min(CROSSFADE_MAX);
    gravar(conn, CROSSFADE, &ms.to_string())?;
    Ok(ms)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn crossfade_padrao_e_gravado() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(carregar(&conn).unwrap().crossfade_ms, CROSSFADE_PADRAO);
        assert_eq!(definir_crossfade(&conn, 0).unwrap(), 0);
        assert_eq!(carregar(&conn).unwrap().crossfade_ms, 0);
        assert_eq!(definir_crossfade(&conn, 60_000).unwrap(), CROSSFADE_MAX);
        assert_eq!(carregar(&conn).unwrap().crossfade_ms, CROSSFADE_MAX);
    }

    #[test]
    fn normalizacao_padrao_e_gravada() {
        use crate::ganho::Modo;
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(carregar(&conn).unwrap().normalizacao, Modo::Faixa);
        definir_normalizacao(&conn, Modo::Album).unwrap();
        assert_eq!(carregar(&conn).unwrap().normalizacao, Modo::Album);
        definir_normalizacao(&conn, Modo::Desligada).unwrap();
        assert_eq!(carregar(&conn).unwrap().normalizacao, Modo::Desligada);
    }

    #[test]
    fn modos_e_temporizador_persistem() {
        use crate::queue::Repetir;
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(modos_fila(&conn).unwrap(), (false, Repetir::Desligado));
        definir_aleatorio(&conn, true).unwrap();
        definir_repetir(&conn, Repetir::Faixa).unwrap();
        assert_eq!(modos_fila(&conn).unwrap(), (true, Repetir::Faixa));
        assert_eq!(temporizador(&conn).unwrap(), None);
        definir_temporizador(&conn, Some(TemporizadorSalvo::Ate(1_800_000_000_000))).unwrap();
        assert_eq!(temporizador(&conn).unwrap(), Some(TemporizadorSalvo::Ate(1_800_000_000_000)));
        definir_temporizador(&conn, Some(TemporizadorSalvo::FimDaFaixa)).unwrap();
        assert_eq!(temporizador(&conn).unwrap(), Some(TemporizadorSalvo::FimDaFaixa));
        definir_temporizador(&conn, None).unwrap();
        assert_eq!(temporizador(&conn).unwrap(), None);
    }

    #[test]
    fn bandeja_e_notificacao() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        let c = carregar(&conn).unwrap();
        assert!(!c.bandeja && c.notificar);
        definir_bandeja(&conn, true).unwrap();
        definir_notificar(&conn, false).unwrap();
        let c = carregar(&conn).unwrap();
        assert!(c.bandeja && !c.notificar);
    }

    #[test]
    fn aparencia_padrao_e_gravada() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        let c = carregar(&conn).unwrap();
        assert_eq!((c.tema.as_str(), c.modo_tema.as_str()), ("carmesim", "sistema"));
        assert!(c.cores_dinamicas);
        definir_cores_dinamicas(&conn, false).unwrap();
        assert!(!carregar(&conn).unwrap().cores_dinamicas);
        definir_aparencia(&conn, "fiorde", "claro").unwrap();
        let c = carregar(&conn).unwrap();
        assert_eq!((c.tema.as_str(), c.modo_tema.as_str()), ("fiorde", "claro"));
        // Nome antigo salvo vira o novo.
        definir_aparencia(&conn, "nord", "claro").unwrap();
        assert_eq!(carregar(&conn).unwrap().tema, "fiorde");
        assert!(definir_aparencia(&conn, "x\" onload", "claro").is_err());
        assert!(definir_aparencia(&conn, "nord", "roxo").is_err());
    }

    #[test]
    fn equalizador_padrao_e_gravado() {
        use crate::eq::Preset;
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(carregar(&conn).unwrap().equalizador, Preset::Desligado);
        definir_equalizador(&conn, Preset::Voz).unwrap();
        assert_eq!(carregar(&conn).unwrap().equalizador, Preset::Voz);
    }
}

/// Identificador deste aparelho na sincronização (nome do arquivo dele no Drive).
pub fn aparelho_id(conn: &Connection) -> rusqlite::Result<String> {
    if let Some(id) = ler(conn, "aparelho_id")? {
        return Ok(id);
    }
    let id: String = conn.query_row("SELECT lower(hex(randomblob(8)))", [], |r| r.get(0))?;
    gravar(conn, "aparelho_id", &id)?;
    Ok(id)
}
