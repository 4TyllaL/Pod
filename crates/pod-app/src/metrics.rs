//! Contadores de desempenho do desenho, só leitura e baratos: quadros apresentados e o tempo do
//! trabalho de UI (BeginDraw..EndDraw, sem o `Present`). Servem para provar "sem apresentações em
//! repouso" e medir o p95 durante resize (ARCHITECTURE §13). Thread de UI apenas.

use std::cell::RefCell;
use std::time::Duration;

const KEEP: usize = 1024;

#[derive(Default)]
struct Metrics {
    presents: u64,
    work_ms: Vec<f32>,
    total_ms: Vec<f32>,
}

thread_local! {
    static METRICS: RefCell<Metrics> = RefCell::new(Metrics::default());
}

pub fn record_frame(work: Duration, total: Duration) {
    METRICS.with(|m| {
        let mut guard = m.borrow_mut();
        let Metrics { presents, work_ms, total_ms } = &mut *guard;
        *presents += 1;
        for (v, d) in [(work_ms, work), (total_ms, total)] {
            if v.len() == KEEP {
                v.remove(0);
            }
            v.push(d.as_secs_f32() * 1000.0);
        }
    });
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub presents: u64,
    pub samples: usize,
    pub work_p50_ms: f32,
    pub work_p95_ms: f32,
    pub work_max_ms: f32,
    pub total_p95_ms: f32,
}

pub fn snapshot() -> Snapshot {
    METRICS.with(|m| {
        let m = m.borrow();
        Snapshot {
            presents: m.presents,
            samples: m.work_ms.len(),
            work_p50_ms: percentile(&m.work_ms, 0.50),
            work_p95_ms: percentile(&m.work_ms, 0.95),
            work_max_ms: m.work_ms.iter().copied().fold(0.0, f32::max),
            total_p95_ms: percentile(&m.total_ms, 0.95),
        }
    })
}

/// Zera as amostras (mantém o contador de quadros), para medir uma janela de tempo específica.
pub fn reset_samples() {
    METRICS.with(|m| {
        let mut m = m.borrow_mut();
        m.work_ms.clear();
        m.total_ms.clear();
    });
}

/// Percentil por posto mais próximo; 0 se não há amostras.
pub fn percentile(samples: &[f32], p: f32) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut v = samples.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let rank = ((p * v.len() as f32).ceil() as usize).clamp(1, v.len());
    v[rank - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_by_nearest_rank() {
        let v: Vec<f32> = (1..=100).map(|i| i as f32).collect();
        assert_eq!(percentile(&v, 0.95), 95.0);
        assert_eq!(percentile(&v, 0.50), 50.0);
        assert_eq!(percentile(&v, 1.0), 100.0);
        assert_eq!(percentile(&[], 0.95), 0.0);
        assert_eq!(percentile(&[7.0], 0.95), 7.0);
        // Ordem de entrada não importa.
        assert_eq!(percentile(&[5.0, 1.0, 3.0], 0.5), 3.0);
    }

    #[test]
    fn recording_counts_presents_and_keeps_a_bounded_window() {
        reset_samples();
        let before = snapshot().presents;
        for i in 0..(KEEP + 50) {
            record_frame(Duration::from_micros(i as u64), Duration::from_micros(2 * i as u64));
        }
        let s = snapshot();
        assert_eq!(s.presents - before, (KEEP + 50) as u64);
        assert_eq!(s.samples, KEEP, "janela limitada: sem crescimento de memória");
        assert!(s.work_max_ms >= s.work_p95_ms && s.work_p95_ms >= s.work_p50_ms);
        reset_samples();
        assert_eq!(snapshot().samples, 0);
    }
}
