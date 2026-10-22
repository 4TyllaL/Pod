//! Transições curtas (120–180 ms). Puro: o relógio é argumento. Quadros só são pedidos enquanto
//! `is_running` for verdadeiro; parado, nenhum timer fica ativo (ARCHITECTURE §6).

/// Duração padrão de hover/foco.
pub const DURATION_MS: u32 = 150;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tween {
    from: f32,
    to: f32,
    start_ms: u64,
    duration_ms: u32,
}

impl Tween {
    pub fn at(value: f32) -> Self {
        Self { from: value, to: value, start_ms: 0, duration_ms: 0 }
    }

    /// Redireciona a transição a partir do valor atual (sem salto). Com `reduce_motion`, vai direto.
    pub fn retarget(&mut self, now_ms: u64, to: f32, reduce_motion: bool) {
        if (to - self.to).abs() < f32::EPSILON {
            return;
        }
        self.from = if reduce_motion { to } else { self.value(now_ms) };
        self.to = to;
        self.start_ms = now_ms;
        self.duration_ms = if reduce_motion { 0 } else { DURATION_MS };
    }

    pub fn value(&self, now_ms: u64) -> f32 {
        if self.duration_ms == 0 {
            return self.to;
        }
        let t = (now_ms.saturating_sub(self.start_ms) as f32 / self.duration_ms as f32).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3); // ease-out cúbico
        self.from + (self.to - self.from) * eased
    }

    pub fn is_running(&self, now_ms: u64) -> bool {
        self.duration_ms > 0 && now_ms.saturating_sub(self.start_ms) < self.duration_ms as u64
    }

    pub fn target(&self) -> f32 {
        self.to
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_from_start_to_target_monotonically() {
        let mut t = Tween::at(0.0);
        t.retarget(1000, 1.0, false);
        let samples: Vec<f32> = (0..=15).map(|i| t.value(1000 + i * 10)).collect();
        assert!((samples[0] - 0.0).abs() < 1e-6);
        assert!((samples[15] - 1.0).abs() < 1e-6, "termina exatamente no alvo");
        assert!(samples.windows(2).all(|w| w[1] >= w[0]), "monótona");
        assert!(t.value(1000 + 75) > 0.5, "ease-out: passa da metade antes da metade do tempo");
    }

    #[test]
    fn running_state_ends_with_the_duration() {
        let mut t = Tween::at(0.0);
        assert!(!t.is_running(0), "parado: nenhum quadro pedido");
        t.retarget(500, 1.0, false);
        assert!(t.is_running(500) && t.is_running(649));
        assert!(!t.is_running(650), "150 ms depois, sem mais quadros");
        assert!(!t.is_running(10_000));
    }

    #[test]
    fn retargeting_midway_has_no_jump() {
        let mut t = Tween::at(0.0);
        t.retarget(0, 1.0, false);
        let mid = t.value(60);
        t.retarget(60, 0.0, false);
        assert!((t.value(60) - mid).abs() < 1e-6, "continua de onde estava");
        assert!(t.value(60 + 150) < 1e-6, "e chega ao novo alvo");
    }

    #[test]
    fn reduce_motion_is_instant_and_never_runs() {
        let mut t = Tween::at(0.0);
        t.retarget(100, 1.0, true);
        assert_eq!(t.value(100), 1.0);
        assert!(!t.is_running(100));
        t.retarget(200, 0.0, true);
        assert_eq!(t.value(200), 0.0);
    }

    #[test]
    fn same_target_is_a_noop() {
        let mut t = Tween::at(1.0);
        t.retarget(10, 1.0, false);
        assert!(!t.is_running(10));
        assert_eq!(t.value(10), 1.0);
    }
}
