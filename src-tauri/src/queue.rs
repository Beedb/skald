use std::collections::VecDeque;

/// Fila de reprodução.
///
/// - `manual`: músicas adicionadas pelo usuário; sempre tocam antes do contexto.
/// - `contexto`: a ordem de onde a música foi escolhida (hoje, a biblioteca);
///   `indice` aponta a posição atual nela e continua de onde parou depois da fila manual.
#[derive(Default, Debug)]
pub struct Fila {
    manual: VecDeque<i64>,
    contexto: Vec<i64>,
    indice: Option<usize>,
    atual: Option<i64>,
}

impl Fila {
    pub fn atual(&self) -> Option<i64> {
        self.atual
    }

    /// Começa a tocar `id` dentro de `contexto`. A fila manual é mantida.
    pub fn tocar_do_contexto(&mut self, contexto: Vec<i64>, id: i64) -> i64 {
        self.indice = contexto.iter().position(|&x| x == id);
        self.contexto = contexto;
        self.atual = Some(id);
        id
    }

    /// Como `tocar_do_contexto`, mas pela posição (útil quando a mesma música aparece duas vezes).
    pub fn tocar_do_contexto_em(&mut self, contexto: Vec<i64>, indice: usize) -> Option<i64> {
        let id = *contexto.get(indice)?;
        self.contexto = contexto;
        self.indice = Some(indice);
        self.atual = Some(id);
        Some(id)
    }

    /// Avança: primeiro a fila manual, depois o contexto. `None` no fim.
    pub fn proxima(&mut self) -> Option<i64> {
        let id = if let Some(id) = self.manual.pop_front() {
            id
        } else {
            let i = self.indice.map_or(0, |i| i + 1);
            let &id = self.contexto.get(i)?;
            self.indice = Some(i);
            id
        };
        self.atual = Some(id);
        Some(id)
    }

    /// Volta uma posição no contexto (músicas da fila manual não são revisitadas).
    /// Se a atual veio da fila manual, volta para a música do contexto onde parou.
    pub fn anterior(&mut self) -> Option<i64> {
        let indice = self.indice?;
        let no_contexto = self.contexto[indice];
        if self.atual != Some(no_contexto) {
            self.atual = Some(no_contexto);
            return Some(no_contexto);
        }
        let i = indice.checked_sub(1)?;
        let id = self.contexto[i];
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

    /// As próximas `n` músicas do contexto depois da atual.
    pub fn proximas_do_contexto(&self, n: usize) -> &[i64] {
        let inicio = self.indice.map_or(0, |i| i + 1).min(self.contexto.len());
        let fim = (inicio + n).min(self.contexto.len());
        &self.contexto[inicio..fim]
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn contexto_avanca_e_volta() {
        let mut f = Fila::default();
        f.tocar_do_contexto(vec![10, 20, 30], 20);
        assert_eq!(f.proximas_do_contexto(5), [30]);
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
}
