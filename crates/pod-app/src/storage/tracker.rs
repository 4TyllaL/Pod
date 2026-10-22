//! Rastreia o que precisa ser gravado e o que foi confirmado. Puro: o relógio é argumento.
//!
//! "Salvo" só existe depois do ack da geração que continha a última revisão. Se houver mudança
//! depois do snapshot enviado, continua sujo.

use std::collections::VecDeque;

pub const DEBOUNCE_MS: u64 = 500;
pub const MAX_DELAY_MS: u64 = 2_000;
pub const RETRY_MS: u64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveState {
    Saved,
    Dirty,
    /// A última gravação falhou; haverá nova tentativa.
    Error(String),
}

#[derive(Default)]
pub struct SaveTracker {
    revision: u64,
    sent_revision: u64,
    acked_revision: u64,
    first_dirty_ms: Option<u64>,
    last_change_ms: u64,
    in_flight: VecDeque<(u64, u64)>, // (geração, revisão contida)
    acked_generation: u64,
    last_error: Option<String>,
    retry_at_ms: Option<u64>,
}

impl SaveTracker {
    pub fn mark_changed(&mut self, now_ms: u64) {
        self.revision += 1;
        self.first_dirty_ms.get_or_insert(now_ms);
        self.last_change_ms = now_ms;
    }

    /// Instante em que a próxima gravação deve ser solicitada: 500 ms sem mudanças, no máximo 2 s
    /// desde a primeira mudança não enviada. `None` se não há nada novo a enviar.
    pub fn due_at(&self) -> Option<u64> {
        if self.revision == self.sent_revision {
            return None;
        }
        let first = self.first_dirty_ms.unwrap_or(self.last_change_ms);
        let due = (self.last_change_ms + DEBOUNCE_MS).min(first + MAX_DELAY_MS);
        Some(self.retry_at_ms.map_or(due, |r| due.max(r)))
    }

    pub fn should_save(&self, now_ms: u64) -> bool {
        self.due_at().is_some_and(|d| d <= now_ms)
    }

    pub fn has_unsent_changes(&self) -> bool {
        self.revision != self.sent_revision
    }

    /// Registra que o snapshot de geração `generation` foi enviado ao worker.
    pub fn begin_save(&mut self, generation: u64) {
        self.in_flight.push_back((generation, self.revision));
        self.sent_revision = self.revision;
        self.first_dirty_ms = None;
        self.retry_at_ms = None;
    }

    /// Confirmação do worker para a geração efetivamente gravada (ou falha).
    pub fn on_ack(&mut self, generation: u64, result: Result<(), String>, now_ms: u64) {
        let rev = self.in_flight.iter().find(|(g, _)| *g == generation).map(|(_, r)| *r);
        self.in_flight.retain(|(g, _)| *g > generation);
        match result {
            Ok(()) => {
                if let Some(rev) = rev {
                    self.acked_revision = self.acked_revision.max(rev);
                }
                self.acked_generation = self.acked_generation.max(generation);
                self.last_error = None;
            }
            Err(msg) => {
                self.last_error = Some(msg);
                // O que não foi confirmado volta a ser pendente, com espera antes de tentar de novo.
                self.sent_revision = self.acked_revision;
                self.first_dirty_ms = Some(self.first_dirty_ms.unwrap_or(now_ms));
                self.last_change_ms = now_ms;
                self.retry_at_ms = Some(now_ms + RETRY_MS);
            }
        }
    }

    pub fn acked_generation(&self) -> u64 {
        self.acked_generation
    }

    pub fn is_saved(&self) -> bool {
        self.revision == self.acked_revision
    }

    pub fn state(&self) -> SaveState {
        match (&self.last_error, self.is_saved()) {
            (Some(e), _) => SaveState::Error(e.clone()),
            (None, true) => SaveState::Saved,
            (None, false) => SaveState::Dirty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_to_save_without_changes() {
        let t = SaveTracker::default();
        assert_eq!(t.due_at(), None);
        assert!(t.is_saved());
    }

    #[test]
    fn debounce_waits_500ms_after_the_last_change() {
        let mut t = SaveTracker::default();
        t.mark_changed(1_000);
        assert_eq!(t.due_at(), Some(1_500));
        t.mark_changed(1_300);
        assert_eq!(t.due_at(), Some(1_800), "nova mudança adia");
        assert!(!t.should_save(1_799));
        assert!(t.should_save(1_800));
    }

    #[test]
    fn continuous_typing_is_capped_at_two_seconds() {
        let mut t = SaveTracker::default();
        for i in 0..40 {
            t.mark_changed(1_000 + i * 100); // digita a cada 100 ms por 4 s
        }
        assert_eq!(t.due_at(), Some(3_000), "teto: 1000 + 2000, apesar de mudanças contínuas");
    }

    #[test]
    fn saved_requires_the_ack_of_the_generation_with_the_latest_revision() {
        let mut t = SaveTracker::default();
        t.mark_changed(0);
        t.begin_save(1);
        assert!(!t.is_saved(), "enviado não é salvo");
        t.on_ack(1, Ok(()), 10);
        assert!(t.is_saved());
        assert_eq!(t.state(), SaveState::Saved);
    }

    #[test]
    fn change_after_the_sent_snapshot_stays_dirty_when_the_old_ack_arrives() {
        let mut t = SaveTracker::default();
        t.mark_changed(0);
        t.begin_save(1);
        t.mark_changed(50); // editou depois de enviar a geração 1
        t.on_ack(1, Ok(()), 60);
        assert!(!t.is_saved(), "o ack antigo não pode limpar a mudança nova");
        assert_eq!(t.state(), SaveState::Dirty);
        assert!(t.has_unsent_changes());
        t.begin_save(2);
        t.on_ack(2, Ok(()), 70);
        assert!(t.is_saved());
    }

    #[test]
    fn a_later_ack_covers_a_superseded_generation() {
        let mut t = SaveTracker::default();
        t.mark_changed(0);
        t.begin_save(1);
        t.mark_changed(10);
        t.begin_save(2); // o worker pode descartar a 1 e gravar só a 2
        t.on_ack(2, Ok(()), 20);
        assert!(t.is_saved());
        assert_eq!(t.acked_generation(), 2);
    }

    #[test]
    fn failure_keeps_it_dirty_and_retries_after_a_delay() {
        let mut t = SaveTracker::default();
        t.mark_changed(0);
        t.begin_save(1);
        t.on_ack(1, Err("disco cheio".into()), 100);
        assert_eq!(t.state(), SaveState::Error("disco cheio".into()));
        assert!(!t.is_saved());
        assert!(t.due_at().unwrap() >= 100 + RETRY_MS);
        t.begin_save(2);
        t.on_ack(2, Ok(()), 6_000);
        assert_eq!(t.state(), SaveState::Saved);
    }
}
