use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Repetir {
    #[default]
    Desligado,
    Fila,
    Faixa,
}

/// Fila de reprodução.
///
/// - `manual`: músicas adicionadas pelo usuário; sempre tocam antes do contexto.
/// - `contexto`: a lista de onde a música foi escolhida (biblioteca, busca ou playlist).
/// - `ordem`: a sequência de posições do contexto a tocar. Em ordem normal é 0..n;
///   com aleatório é um Shuffle Bag: cada música uma vez por ciclo, a primeira protegida.
/// - `indice`: posição atual em `ordem`; continua de onde parou depois da fila manual.
#[derive(Default, Debug)]
pub struct Fila {
    manual: VecDeque<i64>,
    contexto: Vec<i64>,
    ordem: Vec<usize>,
    indice: Option<usize>,
    atual: Option<i64>,
    aleatorio: bool,
    repetir: Repetir,
}

impl Fila {
    pub fn atual(&self) -> Option<i64> {
        self.atual
    }

    pub fn aleatorio(&self) -> bool {
        self.aleatorio
    }

    pub fn repetir(&self) -> Repetir {
        self.repetir
    }

    pub fn definir_repetir(&mut self, modo: Repetir) {
        self.repetir = modo;
    }

    /// Liga/desliga o aleatório. Ligar (de novo) sempre sorteia uma nova sequência,
    /// com a música atual protegida no início do ciclo.
    pub fn definir_aleatorio(&mut self, ativo: bool) {
        let posicao = self.posicao_no_contexto();
        self.aleatorio = ativo;
        self.reordenar(posicao);
    }

    fn posicao_no_contexto(&self) -> Option<usize> {
        self.indice.and_then(|i| self.ordem.get(i).copied())
    }

    /// Refaz `ordem` mantendo `posicao` (do contexto) como a atual.
    fn reordenar(&mut self, posicao: Option<usize>) {
        let n = self.contexto.len();
        if self.aleatorio {
            let mut resto: Vec<usize> = (0..n).filter(|&p| Some(p) != posicao).collect();
            fastrand::shuffle(&mut resto);
            self.ordem = posicao.into_iter().chain(resto).collect();
            self.indice = posicao.map(|_| 0);
        } else {
            self.ordem = (0..n).collect();
            self.indice = posicao;
        }
    }

    /// Novo ciclo do Shuffle Bag. A última tocada não abre o ciclo seguinte (se houver outra).
    fn novo_ciclo(&mut self) {
        let ultima = self.posicao_no_contexto();
        fastrand::shuffle(&mut self.ordem);
        if self.ordem.len() > 1 && self.ordem.first().copied() == ultima {
            let j = fastrand::usize(1..self.ordem.len());
            self.ordem.swap(0, j);
        }
    }

    /// Começa a tocar `id` dentro de `contexto`. A fila manual é mantida.
    pub fn tocar_do_contexto(&mut self, contexto: Vec<i64>, id: i64) -> i64 {
        let posicao = contexto.iter().position(|&x| x == id);
        self.contexto = contexto;
        self.reordenar(posicao);
        self.atual = Some(id);
        id
    }

    /// Como `tocar_do_contexto`, mas pela posição (útil quando a mesma música aparece duas vezes).
    pub fn tocar_do_contexto_em(&mut self, contexto: Vec<i64>, indice: usize) -> Option<i64> {
        let id = *contexto.get(indice)?;
        self.contexto = contexto;
        self.reordenar(Some(indice));
        self.atual = Some(id);
        Some(id)
    }

    /// Acrescenta ao contexto as músicas de `ids` que ainda não estão nele. Com aleatório,
    /// entram no ciclo atual em posição aleatória entre as que ainda vão tocar.
    pub fn incluir(&mut self, ids: &[i64]) {
        for &id in ids {
            if self.contexto.contains(&id) {
                continue;
            }
            self.contexto.push(id);
            let p = self.contexto.len() - 1;
            if self.aleatorio {
                let inicio = self.indice.map_or(0, |i| i + 1);
                let lugar = fastrand::usize(inicio..=self.ordem.len());
                self.ordem.insert(lugar, p);
            } else {
                self.ordem.push(p);
            }
        }
    }

    /// Fim natural da faixa: com "repetir faixa" toca a mesma de novo; senão, `proxima`.
    pub fn proxima_automatica(&mut self) -> Option<i64> {
        if self.repetir == Repetir::Faixa && self.atual.is_some() {
            return self.atual;
        }
        self.proxima()
    }

    /// Avança: primeiro a fila manual, depois o contexto. No fim, recomeça se
    /// "repetir fila" estiver ligado (com aleatório, num novo ciclo); senão `None`.
    pub fn proxima(&mut self) -> Option<i64> {
        let id = if let Some(id) = self.manual.pop_front() {
            id
        } else {
            let mut i = self.indice.map_or(0, |i| i + 1);
            if i >= self.ordem.len() {
                if self.repetir == Repetir::Desligado || self.ordem.is_empty() {
                    return None;
                }
                if self.aleatorio {
                    self.novo_ciclo();
                }
                i = 0;
            }
            self.indice = Some(i);
            self.contexto[self.ordem[i]]
        };
        self.atual = Some(id);
        Some(id)
    }

    /// Volta uma posição na ordem (músicas da fila manual não são revisitadas).
    /// Se a atual veio da fila manual, volta para a música do contexto onde parou.
    pub fn anterior(&mut self) -> Option<i64> {
        let indice = self.indice?;
        let no_contexto = self.contexto[self.ordem[indice]];
        if self.atual != Some(no_contexto) {
            self.atual = Some(no_contexto);
            return Some(no_contexto);
        }
        let i = indice.checked_sub(1)?;
        let id = self.contexto[self.ordem[i]];
        self.indice = Some(i);
        self.atual = Some(id);
        Some(id)
    }

    pub fn adicionar(&mut self, id: i64) {
        self.manual.push_back(id);
    }

    pub fn tocar_a_seguir(&mut self, id: i64) {
        self.manual.push_front(id);
    }

    pub fn remover_manual(&mut self, posicao: usize) -> Option<i64> {
        self.manual.remove(posicao)
    }

    pub fn manual(&self) -> impl Iterator<Item = i64> + '_ {
        self.manual.iter().copied()
    }

    /// As próximas `n` músicas do contexto depois da atual (na ordem em que vão tocar).
    pub fn proximas_do_contexto(&self, n: usize) -> Vec<i64> {
        let inicio = self.indice.map_or(0, |i| i + 1).min(self.ordem.len());
        self.ordem[inicio..].iter().take(n).map(|&p| self.contexto[p]).collect()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn contexto_avanca_e_volta() {
        let mut f = Fila::default();
        f.tocar_do_contexto(vec![10, 20, 30], 20);
        assert_eq!(f.proximas_do_contexto(5), vec![30]);
        assert_eq!(f.proxima(), Some(30));
        assert_eq!(f.proxima(), None);
        assert_eq!(f.anterior(), Some(20));
        assert_eq!(f.anterior(), Some(10));
        assert_eq!(f.anterior(), None);
        assert_eq!(f.atual(), Some(10));
    }

    #[test]
    fn manual_toca_antes_e_contexto_continua() {
        let mut f = Fila::default();
        f.tocar_do_contexto(vec![1, 2, 3], 1);
        f.adicionar(100);
        f.adicionar(200);
        f.tocar_a_seguir(50);
        assert_eq!(f.manual().collect::<Vec<_>>(), [50, 100, 200]);
        assert_eq!(f.remover_manual(1), Some(100));
        assert_eq!(f.proxima(), Some(50));
        assert_eq!(f.proxima(), Some(200));
        assert_eq!(f.proxima(), Some(2));
        assert_eq!(f.proxima(), Some(3));
        assert_eq!(f.proxima(), None);
    }

    #[test]
    fn anterior_depois_da_fila_manual() {
        let mut f = Fila::default();
        f.tocar_do_contexto(vec![1, 2, 3], 2);
        f.adicionar(100);
        assert_eq!(f.proxima(), Some(100));
        assert_eq!(f.anterior(), Some(2));
        assert_eq!(f.anterior(), Some(1));
    }

    #[test]
    fn fila_manual_sem_contexto() {
        let mut f = Fila::default();
        assert_eq!(f.proxima(), None);
        f.adicionar(7);
        assert_eq!(f.proxima(), Some(7));
        assert_eq!(f.anterior(), None);
    }

    fn ciclo(f: &mut Fila, n: usize) -> Vec<i64> {
        (0..n).map(|_| f.proxima().unwrap()).collect()
    }

    #[test]
    fn aleatorio_toca_cada_uma_uma_vez_e_protege_a_primeira() {
        let ids: Vec<i64> = (1..=20).collect();
        let mut f = Fila::default();
        f.definir_aleatorio(true);
        f.tocar_do_contexto(ids.clone(), 7);
        let mut tocadas = ciclo(&mut f, 19);
        assert!(!tocadas.contains(&7));
        assert_eq!(f.proxima(), None);
        tocadas.push(7);
        tocadas.sort();
        assert_eq!(tocadas, ids);
    }

    #[test]
    fn repetir_fila_inicia_novo_ciclo_sem_repetir_a_ultima() {
        for _ in 0..50 {
            let mut f = Fila::default();
            f.definir_aleatorio(true);
            f.definir_repetir(Repetir::Fila);
            f.tocar_do_contexto(vec![1, 2, 3], 1);
            let ultima = *ciclo(&mut f, 2).last().unwrap();
            let mut novo = ciclo(&mut f, 3);
            assert_ne!(novo[0], ultima);
            novo.sort();
            assert_eq!(novo, [1, 2, 3]);
        }
    }

    #[test]
    fn manual_toca_antes_do_aleatorio() {
        let mut f = Fila::default();
        f.definir_aleatorio(true);
        f.tocar_do_contexto(vec![1, 2, 3, 4], 1);
        f.adicionar(100);
        assert_eq!(f.proxima(), Some(100));
        assert!([2, 3, 4].contains(&f.proxima().unwrap()));
    }

    #[test]
    fn musica_nova_entra_nas_restantes() {
        let mut f = Fila::default();
        f.definir_aleatorio(true);
        f.tocar_do_contexto((1..=10).collect(), 1);
        let ja = ciclo(&mut f, 3);
        f.incluir(&[1, 99]);
        let resto = ciclo(&mut f, 7);
        assert!(resto.contains(&99));
        assert!(ja.iter().all(|x| !resto.contains(x)));
        assert_eq!(f.proxima(), None);
    }

    #[test]
    fn repetir_faixa_so_no_fim_natural() {
        let mut f = Fila::default();
        f.definir_repetir(Repetir::Faixa);
        f.tocar_do_contexto(vec![1, 2], 1);
        assert_eq!(f.proxima_automatica(), Some(1));
        assert_eq!(f.proxima(), Some(2));
    }

    #[test]
    fn desligar_aleatorio_volta_a_ordem_a_partir_da_atual() {
        let mut f = Fila::default();
        f.definir_aleatorio(true);
        f.tocar_do_contexto(vec![1, 2, 3, 4], 1);
        let atual = f.proxima().unwrap();
        f.definir_aleatorio(false);
        let esperado: Vec<i64> = (atual + 1..=4).collect();
        assert_eq!(f.proximas_do_contexto(10), esperado);
    }
}
