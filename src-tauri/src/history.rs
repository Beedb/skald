use rusqlite::{Connection, params};
use serde::Serialize;

/// Mínimo ouvido para contar como escutada (seção 11.1).
const MINIMO_MS: u64 = 20_000;
/// Fração da duração que precisa ser ouvida.
const FRACAO: f64 = 0.6;
/// Durante a reprodução, o registro é atualizado no banco a cada este intervalo.
pub const GRAVAR_A_CADA_MS: u64 = 10_000;

/// "Escutada": mais de 60% da duração e no mínimo 20 s. Sem duração conhecida, vale só o mínimo.
pub fn escutada(ouvido_ms: u64, duracao_ms: u64) -> bool {
    ouvido_ms >= MINIMO_MS && (duracao_ms == 0 || ouvido_ms as f64 > duracao_ms as f64 * FRACAO)
}

/// Sessão de uma faixa: do momento em que começa a tocar até outra faixa começar, parar ou terminar.
/// Soma só o tempo em que estava de fato tocando (pausas e saltos na barra não contam).
#[derive(Debug)]
pub struct Sessao {
    pub registro: i64,
    pub media_id: i64,
    pub duracao_ms: u64,
    pub ouvido_ms: u64,
    pub concluida: bool,
    /// Último aviso do player: (instante em ms, estava tocando?).
    ultimo: Option<(u64, bool)>,
    gravado_em_ms: u64,
}

impl Sessao {
    pub fn nova(registro: i64, media_id: i64, duracao_ms: u64, agora_ms: u64) -> Self {
        Self { registro, media_id, duracao_ms, ouvido_ms: 0, concluida: false, ultimo: None, gravado_em_ms: agora_ms }
    }

    /// Registra um aviso do player. Devolve `true` se convém gravar agora
    /// (parou de tocar ou passou o intervalo de gravação).
    pub fn aviso(&mut self, tocando: bool, agora_ms: u64, duracao_ms: u64) -> bool {
        if let Some((antes, true)) = self.ultimo {
            self.ouvido_ms += agora_ms.saturating_sub(antes);
        }
        let parou = matches!(self.ultimo, Some((_, true))) && !tocando;
        self.ultimo = Some((agora_ms, tocando));
        if duracao_ms > 0 {
            self.duracao_ms = duracao_ms;
        }
        if parou || agora_ms.saturating_sub(self.gravado_em_ms) >= GRAVAR_A_CADA_MS {
            self.gravado_em_ms = agora_ms;
            return true;
        }
        false
    }

    pub fn escutada(&self) -> bool {
        escutada(self.ouvido_ms, self.duracao_ms)
    }
}

/// Cria o registro da reprodução, copiando os dados da música para que o histórico
/// continue legível mesmo se ela sair da biblioteca.
pub fn iniciar(conn: &Connection, media_id: i64, inicio_unix: i64) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO play_history (media_id, category, title, artist, album, genre, duration_ms, started_at)
         SELECT id, category, title, artist, album, genre, duration_ms, ?2 FROM media WHERE id = ?1",
        params![media_id, inicio_unix],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn gravar(conn: &Connection, s: &Sessao) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE play_history SET listened_ms = ?2, completed = ?3, counted = ?4,
                duration_ms = COALESCE(NULLIF(?5, 0), duration_ms)
         WHERE id = ?1",
        params![s.registro, s.ouvido_ms as i64, s.concluida, s.escutada(), s.duracao_ms as i64],
    )?;
    Ok(())
}

#[derive(Serialize, Debug)]
pub struct Entrada {
    pub id: i64,
    /// Nulo se a música saiu da biblioteca.
    pub media_id: Option<i64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub started_at: i64,
    pub listened_ms: i64,
    pub completed: bool,
    pub counted: bool,
    pub missing: bool,
}

/// Uma página do histórico, da reprodução mais recente para a mais antiga.
pub fn listar(conn: &Connection, offset: u32, limit: u32) -> rusqlite::Result<Vec<Entrada>> {
    let mut stmt = conn.prepare_cached(
        "SELECT h.id, h.media_id, COALESCE(m.title, h.title), COALESCE(m.artist, h.artist),
                COALESCE(m.album, h.album), h.started_at, h.listened_ms, h.completed, h.counted,
                COALESCE(m.missing, 1)
         FROM play_history h LEFT JOIN media m ON m.id = h.media_id
         ORDER BY h.started_at DESC, h.id DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let linhas = stmt.query_map(params![limit, offset], |r| {
        Ok(Entrada {
            id: r.get(0)?,
            media_id: r.get(1)?,
            title: r.get(2)?,
            artist: r.get(3)?,
            album: r.get(4)?,
            started_at: r.get(5)?,
            listened_ms: r.get(6)?,
            completed: r.get(7)?,
            counted: r.get(8)?,
            missing: r.get(9)?,
        })
    })?;
    linhas.collect()
}

pub fn contar(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM play_history", [], |r| r.get(0))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn criterio_de_escutada() {
        assert!(!escutada(19_999, 25_000)); // menos de 20 s, mesmo passando de 60%
        assert!(escutada(20_000, 30_000)); // 66%
        assert!(!escutada(100_000, 200_000)); // 50%
        assert!(escutada(120_001, 200_000)); // pouco mais de 60%
        assert!(!escutada(120_000, 200_000)); // exatamente 60% não basta ("mais de")
        assert!(escutada(20_000, 0)); // duração desconhecida
    }

    #[test]
    fn soma_so_o_tempo_tocando() {
        let mut s = Sessao::nova(1, 7, 100_000, 0);
        s.aviso(true, 0, 100_000); // começou a tocar
        s.aviso(true, 30_000, 0);
        assert!(s.aviso(false, 40_000, 0), "pausar deve pedir gravação");
        s.aviso(false, 90_000, 0); // pausado por 50 s: não conta
        s.aviso(true, 90_000, 0); // retomou
        s.aviso(true, 111_000, 0);
        assert_eq!(s.ouvido_ms, 61_000);
        assert!(s.escutada());
    }

    #[test]
    fn pede_gravacao_periodica() {
        let mut s = Sessao::nova(1, 7, 300_000, 0);
        assert!(!s.aviso(true, 0, 0));
        assert!(!s.aviso(true, 9_000, 0));
        assert!(s.aviso(true, 10_000, 0));
        assert!(!s.aviso(true, 15_000, 0));
    }

    #[test]
    fn grava_e_preserva_apos_remover_da_biblioteca() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        conn.execute(
            "INSERT INTO media (id, path, title, artist, album, duration_ms) VALUES (7, '/x', 'Invasão', 'Rodrigo Zin', 'A', 30000)",
            [],
        )
        .unwrap();
        let reg = iniciar(&conn, 7, 1_700_000_000).unwrap();
        let mut s = Sessao::nova(reg, 7, 30_000, 0);
        s.aviso(true, 0, 0);
        s.aviso(true, 25_000, 0);
        s.concluida = true;
        gravar(&conn, &s).unwrap();

        conn.execute("DELETE FROM media WHERE id = 7", []).unwrap();
        let e = &listar(&conn, 0, 10).unwrap()[0];
        assert_eq!(e.media_id, None);
        assert_eq!((e.title.as_deref(), e.artist.as_deref()), (Some("Invasão"), Some("Rodrigo Zin")));
        assert_eq!((e.listened_ms, e.completed, e.counted, e.missing), (25_000, true, true, true));
        let categoria: String = conn.query_row("SELECT category FROM play_history", [], |r| r.get(0)).unwrap();
        assert_eq!(categoria, "music");
        assert_eq!(contar(&conn).unwrap(), 1);
    }
}
