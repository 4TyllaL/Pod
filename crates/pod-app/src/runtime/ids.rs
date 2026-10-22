//! Geração de IDs de instância. Injetável para testes determinísticos.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use pod_model::ids::WidgetId;

pub trait IdSource {
    fn next_id(&mut self) -> WidgetId;
}

/// 128 bits aleatórios a partir da semente aleatória por processo da biblioteca padrão.
/// Suficiente para identidade (não é segredo); evita uma dependência só para isso.
pub struct RandomIds {
    state: RandomState,
    counter: u64,
}

impl RandomIds {
    pub fn new() -> Self {
        Self { state: RandomState::new(), counter: 0 }
    }

    fn word(&mut self) -> u64 {
        self.counter += 1;
        let mut h = self.state.build_hasher();
        h.write_u64(self.counter);
        h.finish()
    }
}

impl IdSource for RandomIds {
    fn next_id(&mut self) -> WidgetId {
        let (hi, lo) = (self.word(), self.word());
        WidgetId(((hi as u128) << 64) | lo as u128)
    }
}
