//! Playlists inteligentes: regras (JSON em `playlist.regras`) que escolhem as músicas sozinhas.
//! O resultado é gravado nos itens da playlist (`atualizar`), então tocar, fila, exportar M3U,
//! widget e Android Auto funcionam igual às outras; os itens não podem ser editados à mão.
//! Só músicas presentes da categoria Música entram.

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::search::normalizar;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Combinar {
    #[default]
    Todas,
    Qualquer,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Campo {
    Titulo,
    Artista,
    Album,
    Genero,
    Ano,
    Categoria,
    Curtida,
    /// Vezes escutada (critério de 60%).
    Escutas,
    /// Adicionada nos últimos N dias.
    AdicionadaDias,
    /// Tocada nos últimos N dias.
    TocadaDias,
    /// Não tocada nos últimos N dias (inclui as nunca tocadas).
    NaoTocadaDias,
    /// Duração em minutos.
    DuracaoMin,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Contem,
    NaoContem,
    Igual,
    Diferente,
    MaiorIgual,
    MenorIgual,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Regra {
    pub campo: Campo,
    pub op: Op,
    pub valor: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrdemRegras {
    /// Embaralhada, mas sempre igual enquanto as músicas forem as mesmas.
    #[default]
    Aleatoria,
    Titulo,
    Artista,
    AdicionadaRecente,
    MaisTocadas,
    TocadaRecente,
    AnoRecente,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Regras {
    #[serde(default)]
    pub combinar: Combinar,
    #[serde(default)]
    pub regras: Vec<Regra>,
    /// Máximo de músicas (nenhum = todas).
    #[serde(default)]
    pub limite: Option<u32>,
    #[serde(default)]
    pub ordem: OrdemRegras,
}

const ESCUTAS: &str = "(SELECT COUNT(*) FROM play_history h WHERE h.media_id = m.id AND h.counted = 1)";
const ULTIMA: &str = "(SELECT MAX(h.started_at) FROM play_history h WHERE h.media_id = m.id)";

fn numero(r: &Regra) -> Result<f64, String> {
    r.valor
        .trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or_else(|| format!("“{}” não é um número válido.", r.valor.trim()))
}

fn comparar(op: Op) -> Result<&'static str, String> {
    match op {
        Op::Igual => Ok("="),
        Op::Diferente => Ok("<>"),
        Op::MaiorIgual => Ok(">="),
        Op::MenorIgual => Ok("<="),
        _ => Err("Comparação inválida para um número.".into()),
    }
}

/// Uma regra em SQL (sobre `media m`), com os valores em `p`.
fn condicao(r: &Regra, p: &mut Vec<Value>) -> Result<String, String> {
    let texto = |coluna: &str, p: &mut Vec<Value>| -> Result<String, String> {
        let v = normalizar(r.valor.trim());
        if v.is_empty() {
            return Err("Preencha o texto da regra.".into());
        }
        p.push(Value::Text(v));
        let c = format!("NORMALIZAR(COALESCE({coluna}, ''))");
        Ok(match r.op {
            Op::Contem => format!("instr({c}, ?) > 0"),
            Op::NaoContem => format!("instr({c}, ?) = 0"),
            Op::Igual => format!("{c} = ?"),
            Op::Diferente => format!("{c} <> ?"),
            _ => return Err("Comparação inválida para um texto.".into()),
        })
    };
    let dias = |p: &mut Vec<Value>| -> Result<i64, String> {
        let n = numero(r)?;
        p.push(Value::Integer((n * 86_400.0) as i64));
        Ok(0)
    };
    Ok(match r.campo {
        Campo::Titulo => texto("m.title", p)?,
        Campo::Album => texto("m.album", p)?,
        Campo::Genero => texto("m.genre", p)?,
        // "É" vale para cada participante ("A feat. B" é de A e de B).
        Campo::Artista if matches!(r.op, Op::Igual | Op::Diferente) => {
            let v = normalizar(r.valor.trim());
            if v.is_empty() {
                return Err("Preencha o artista.".into());
            }
            p.push(Value::Text(v));
            let existe = "EXISTS (SELECT 1 FROM json_each(ARTISTAS(m.artist)) j WHERE NORMALIZAR(COALESCE(j.value, '')) = ?)";
            if r.op == Op::Igual { existe.to_string() } else { format!("NOT {existe}") }
        }
        Campo::Artista => texto("m.artist", p)?,
        Campo::Ano => {
            p.push(Value::Integer(numero(r)? as i64));
            format!("m.year {} ?", comparar(r.op)?)
        }
        Campo::DuracaoMin => {
            p.push(Value::Integer((numero(r)? * 60_000.0) as i64));
            format!("m.duration_ms {} ?", comparar(r.op)?)
        }
        Campo::Escutas => {
            p.push(Value::Integer(numero(r)? as i64));
            format!("{ESCUTAS} {} ?", comparar(r.op)?)
        }
        Campo::Categoria => {
            let v = normalizar(r.valor.trim());
            if v.is_empty() {
                return Err("Escolha a categoria.".into());
            }
            p.push(Value::Text(v));
            let existe = "EXISTS (SELECT 1 FROM media_tag mt JOIN tag t ON t.id = mt.tag_id
                          WHERE mt.media_id = m.id AND NORMALIZAR(t.nome) = ?)";
            match r.op {
                Op::Igual => existe.to_string(),
                Op::Diferente => format!("NOT {existe}"),
                _ => return Err("Comparação inválida para categoria.".into()),
            }
        }
        Campo::Curtida => {
            let existe = "EXISTS (SELECT 1 FROM playlist_item pi JOIN playlist c ON c.id = pi.playlist_id
                          WHERE c.kind = 'curtidas' AND pi.media_id = m.id)";
            let sim = !matches!(normalizar(r.valor.trim()).as_str(), "nao" | "false" | "0");
            if sim == (r.op != Op::Diferente) { existe.to_string() } else { format!("NOT {existe}") }
        }
        Campo::AdicionadaDias => {
            dias(p)?;
            "m.added_at >= unixepoch() - ?".to_string()
        }
        Campo::TocadaDias => {
            dias(p)?;
            "EXISTS (SELECT 1 FROM play_history h WHERE h.media_id = m.id AND h.started_at >= unixepoch() - ?)".to_string()
        }
        Campo::NaoTocadaDias => {
            dias(p)?;
            "NOT EXISTS (SELECT 1 FROM play_history h WHERE h.media_id = m.id AND h.started_at >= unixepoch() - ?)".to_string()
        }
    })
}

/// Consulta SQL (e valores) que devolve os ids das músicas na ordem das regras. `semente`
/// fixa o embaralhamento (o id da playlist).
fn consulta(r: &Regras, semente: i64) -> Result<(String, Vec<Value>), String> {
    let mut p = Vec::new();
    let partes = r.regras.iter().map(|x| condicao(x, &mut p)).collect::<Result<Vec<_>, _>>()?;
    let juntar = if r.combinar == Combinar::Todas { " AND " } else { " OR " };
    let filtro = if partes.is_empty() { "1".to_string() } else { format!("({})", partes.join(juntar)) };
    let ordem = match r.ordem {
        // Embaralhamento estável: muda só quando o conjunto muda.
        OrdemRegras::Aleatoria => format!("((m.id * 2654435761 + {}) % 4294967291)", semente.rem_euclid(1 << 30)),
        OrdemRegras::Titulo => "m.title IS NULL, m.title COLLATE SEMACENTO".into(),
        OrdemRegras::Artista => {
            "m.artist IS NULL, m.artist COLLATE SEMACENTO, m.album COLLATE SEMACENTO, m.disc_number, m.track_number".into()
        }
        OrdemRegras::AdicionadaRecente => "m.added_at DESC".into(),
        OrdemRegras::MaisTocadas => format!("{ESCUTAS} DESC, {ULTIMA} DESC"),
        OrdemRegras::TocadaRecente => format!("{ULTIMA} IS NULL, {ULTIMA} DESC"),
        OrdemRegras::AnoRecente => "m.year IS NULL, m.year DESC, m.album COLLATE SEMACENTO, m.track_number".into(),
    };
    let limite = match r.limite {
        Some(n) if n > 0 => format!(" LIMIT {n}"),
        _ => String::new(),
    };
    let sql = format!(
        "SELECT m.id FROM media m WHERE m.category = 'music' AND m.missing = 0 AND {filtro}
         ORDER BY {ordem}, m.id{limite}"
    );
    Ok((sql, p))
}

/// Ids das músicas que as regras escolhem agora.
pub fn escolher(conn: &Connection, r: &Regras, semente: i64) -> Result<Vec<i64>, String> {
    let (sql, p) = consulta(r, semente)?;
    let mut st = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let ids = st
        .query_map(rusqlite::params_from_iter(p), |l| l.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<i64>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ids)
}

pub fn regras(conn: &Connection, id: i64) -> Result<Option<Regras>, String> {
    let texto: Option<Option<String>> = conn
        .query_row("SELECT regras FROM playlist WHERE id = ?1", [id], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(texto.flatten().and_then(|t| serde_json::from_str(&t).ok()))
}

/// Refaz os itens da playlist inteligente `id` se o resultado das regras mudou.
pub fn atualizar(conn: &Connection, id: i64) -> Result<(), String> {
    let Some(r) = regras(conn, id)? else { return Ok(()) };
    let novos = escolher(conn, &r, id)?;
    let atuais: Vec<i64> = {
        let mut st = conn
            .prepare_cached("SELECT media_id FROM playlist_item WHERE playlist_id = ?1 ORDER BY position, id")
            .map_err(|e| e.to_string())?;
        st.query_map([id], |l| l.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    if novos == atuais {
        return Ok(());
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM playlist_item WHERE playlist_id = ?1", [id]).map_err(|e| e.to_string())?;
    {
        let mut ins = tx
            .prepare("INSERT INTO playlist_item (playlist_id, media_id, position) VALUES (?1, ?2, ?3)")
            .map_err(|e| e.to_string())?;
        for (i, m) in novos.iter().enumerate() {
            ins.execute(params![id, m, i as i64]).map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Atualiza todas as playlists inteligentes.
pub fn atualizar_todas(conn: &Connection) -> Result<(), String> {
    let ids: Vec<i64> = {
        let mut st = conn.prepare_cached("SELECT id FROM playlist WHERE regras IS NOT NULL").map_err(|e| e.to_string())?;
        st.query_map([], |l| l.get(0)).map_err(|e| e.to_string())?.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    ids.into_iter().try_for_each(|id| atualizar(conn, id))
}

/// Cria (`id` nulo) ou altera as regras de uma playlist inteligente; devolve o id.
pub fn salvar(conn: &Connection, id: Option<i64>, nome: &str, r: &Regras) -> Result<i64, String> {
    consulta(r, 0)?; // valida antes de gravar
    let json = serde_json::to_string(r).map_err(|e| e.to_string())?;
    let id = match id {
        Some(id) => {
            let n = conn
                .execute("UPDATE playlist SET regras = ?2 WHERE id = ?1 AND regras IS NOT NULL", params![id, json])
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("Playlist inteligente não encontrada.".into());
            }
            crate::playlists::renomear(conn, id, nome)?;
            id
        }
        None => {
            let id = crate::playlists::criar(conn, nome)?;
            conn.execute("UPDATE playlist SET regras = ?2 WHERE id = ?1", params![id, json]).map_err(|e| e.to_string())?;
            id
        }
    };
    atualizar(conn, id)?;
    Ok(id)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn banco() -> Connection {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        conn.execute_batch(
            "INSERT INTO media (id, path, title, artist, album, genre, year, duration_ms, added_at) VALUES
             (1, '/1', 'Ágape', 'Cartola', 'Verde', 'Samba', 1976, 180000, unixepoch()),
             (2, '/2', 'Bola', 'Cartola feat. Nelson', 'Outro', 'MPB', 1974, 240000, unixepoch() - 90 * 86400),
             (3, '/3', 'Céu', 'Rush', 'Moving', 'Rock', 1981, 300000, unixepoch()),
             (4, '/4', 'Dia', 'Rush', 'Moving', 'Rock', 1981, 200000, unixepoch());
             UPDATE media SET missing = 1 WHERE id = 4;
             INSERT INTO play_history (media_id, started_at, counted) VALUES
             (3, unixepoch() - 86400, 1), (3, unixepoch() - 2 * 86400, 1), (1, unixepoch() - 400 * 86400, 1);",
        )
        .unwrap();
        conn
    }

    fn r(campo: Campo, op: Op, valor: &str) -> Regra {
        Regra { campo, op, valor: valor.into() }
    }

    fn ids(conn: &Connection, regras: Vec<Regra>, combinar: Combinar, ordem: OrdemRegras) -> Vec<i64> {
        escolher(conn, &Regras { combinar, regras, limite: None, ordem }, 1).unwrap()
    }

    #[test]
    fn regras_escolhem() {
        let c = banco();
        let t = |regras, ordem| ids(&c, regras, Combinar::Todas, ordem);
        // Artista "é" vale para participante; ausente (4) nunca entra.
        assert_eq!(t(vec![r(Campo::Artista, Op::Igual, "nelson")], OrdemRegras::Titulo), [2]);
        assert_eq!(t(vec![r(Campo::Artista, Op::Contem, "CARTO")], OrdemRegras::Titulo), [1, 2]);
        assert_eq!(t(vec![r(Campo::Titulo, Op::Igual, "agape")], OrdemRegras::Titulo), [1]);
        assert_eq!(t(vec![r(Campo::Ano, Op::MenorIgual, "1976")], OrdemRegras::AnoRecente), [1, 2]);
        assert_eq!(t(vec![r(Campo::DuracaoMin, Op::MaiorIgual, "4")], OrdemRegras::Titulo), [2, 3]);
        assert_eq!(t(vec![r(Campo::Escutas, Op::MaiorIgual, "1")], OrdemRegras::MaisTocadas), [3, 1]);
        assert_eq!(t(vec![r(Campo::TocadaDias, Op::Igual, "30")], OrdemRegras::Titulo), [3]);
        assert_eq!(t(vec![r(Campo::NaoTocadaDias, Op::Igual, "30")], OrdemRegras::Titulo), [1, 2]);
        assert_eq!(t(vec![r(Campo::AdicionadaDias, Op::Igual, "30")], OrdemRegras::Titulo), [1, 3]);
        assert_eq!(
            ids(&c, vec![r(Campo::Genero, Op::Igual, "rock"), r(Campo::Genero, Op::Igual, "mpb")], Combinar::Qualquer, OrdemRegras::Titulo),
            [2, 3]
        );
        assert_eq!(t(vec![], OrdemRegras::Titulo), [1, 2, 3]);
        assert!(escolher(&c, &Regras { regras: vec![r(Campo::Ano, Op::Contem, "1")], ..Default::default() }, 1).is_err());
        assert!(escolher(&c, &Regras { regras: vec![r(Campo::Ano, Op::Igual, "x")], ..Default::default() }, 1).is_err());
        let lim = Regras { limite: Some(2), ordem: OrdemRegras::Titulo, ..Default::default() };
        assert_eq!(escolher(&c, &lim, 1).unwrap(), [1, 2]);
    }

    #[test]
    fn curtida_e_categoria() {
        let mut c = banco();
        crate::playlists::alternar_curtida(&mut c, 2).unwrap();
        c.execute_batch("INSERT INTO tag (id, nome) VALUES (1, 'Anos 70'); INSERT INTO media_tag (media_id, tag_id, origem) VALUES (1, 1, 'manual');").unwrap();
        let t = |regras| ids(&c, regras, Combinar::Todas, OrdemRegras::Titulo);
        assert_eq!(t(vec![r(Campo::Curtida, Op::Igual, "sim")]), [2]);
        assert_eq!(t(vec![r(Campo::Curtida, Op::Igual, "não")]), [1, 3]);
        assert_eq!(t(vec![r(Campo::Categoria, Op::Igual, "anos 70")]), [1]);
        assert_eq!(t(vec![r(Campo::Categoria, Op::Diferente, "Anos 70")]), [2, 3]);
    }

    #[test]
    fn salva_e_atualiza() {
        let c = banco();
        let regras = Regras { regras: vec![r(Campo::Artista, Op::Contem, "rush")], ..Default::default() };
        let id = salvar(&c, None, "Rush", &regras).unwrap();
        let itens = |c: &Connection| crate::playlists::itens(c, id, crate::playlists::Ordem::Personalizada).unwrap().len();
        assert_eq!(itens(&c), 1);
        c.execute("UPDATE media SET missing = 0 WHERE id = 4", []).unwrap();
        atualizar_todas(&c).unwrap();
        assert_eq!(itens(&c), 2);
        // Embaralhada é estável: atualizar de novo não muda nada.
        let ordem = |c: &Connection| escolher(c, &Regras::default(), id).unwrap();
        assert_eq!(ordem(&c), ordem(&c));
        let mut c2 = c;
        assert!(crate::playlists::adicionar(&mut c2, id, &[1]).is_err(), "itens de inteligente não se editam");
        let c = c2;
        assert!(salvar(&c, Some(999), "x", &regras).is_err());
        let outra = Regras { regras: vec![r(Campo::Titulo, Op::Contem, "a")], ..Default::default() };
        salvar(&c, Some(id), "Com A", &outra).unwrap();
        assert_eq!(itens(&c), 3);
    }
}
