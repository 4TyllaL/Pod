//! Agendador central de prazos: um só, para todas as instâncias (sem timer por widget).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use pod_model::ids::WidgetId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timer {
    pub deadline_ms: u64,
    pub id: WidgetId,
    pub generation: u64,
    pub token: u64,
}

#[derive(Default)]
pub struct Scheduler {
    heap: BinaryHeap<Reverse<Timer>>,
}

impl Scheduler {
    pub fn schedule(&mut self, id: WidgetId, generation: u64, token: u64, deadline_ms: u64) {
        // Reagendar o mesmo token substitui o prazo anterior.
        self.cancel(id, generation, token);
        self.heap.push(Reverse(Timer { deadline_ms, id, generation, token }));
    }

    pub fn cancel(&mut self, id: WidgetId, generation: u64, token: u64) {
        self.heap.retain(|Reverse(t)| !(t.id == id && t.generation == generation && t.token == token));
    }

    pub fn cancel_all(&mut self, id: WidgetId) {
        self.heap.retain(|Reverse(t)| t.id != id);
    }

    pub fn take_due(&mut self, now_ms: u64) -> Vec<Timer> {
        let mut due = Vec::new();
        while let Some(Reverse(t)) = self.heap.peek() {
            if t.deadline_ms > now_ms {
                break;
            }
            due.push(self.heap.pop().expect("peek garantiu um elemento").0);
        }
        due
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.heap.peek().map(|Reverse(t)| t.deadline_ms)
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }
}
