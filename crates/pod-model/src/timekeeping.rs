//! Máquinas de estado de Timer, Stopwatch e Pomodoro. Puras: o tempo é o relógio monotônico em
//! milissegundos que o runtime já entrega (`now_ms`); nada aqui lê relógio.

/// Contagem regressiva.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdState {
    Idle,
    Running { deadline: u64 },
    Paused { remaining: u64 },
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Countdown {
    pub duration: u64,
    pub state: CdState,
}

impl Countdown {
    pub fn new(duration_ms: u64) -> Self {
        Self { duration: duration_ms, state: CdState::Idle }
    }

    pub fn remaining(&self, now: u64) -> u64 {
        match self.state {
            CdState::Idle => self.duration,
            CdState::Running { deadline } => deadline.saturating_sub(now),
            CdState::Paused { remaining } => remaining,
            CdState::Done => 0,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, CdState::Running { .. })
    }

    /// Inicia (ou retoma). Depois de `Done` recomeça do início.
    pub fn start(&mut self, now: u64) {
        match self.state {
            CdState::Idle | CdState::Done if self.duration > 0 => {
                self.state = CdState::Running { deadline: now + self.duration }
            }
            CdState::Paused { remaining } => self.state = CdState::Running { deadline: now + remaining },
            _ => {}
        }
    }

    pub fn pause(&mut self, now: u64) {
        if self.is_running() {
            self.state = CdState::Paused { remaining: self.remaining(now) };
        }
    }

    pub fn toggle(&mut self, now: u64) {
        if self.is_running() {
            self.pause(now);
        } else {
            self.start(now);
        }
    }

    pub fn reset(&mut self) {
        self.state = CdState::Idle;
    }

    /// Mudar a duração zera a contagem.
    pub fn set_duration(&mut self, ms: u64) {
        self.duration = ms;
        self.state = CdState::Idle;
    }

    /// Vira `Done` se o prazo passou. Devolve `true` só na transição.
    pub fn poll(&mut self, now: u64) -> bool {
        if let CdState::Running { deadline } = self.state {
            if now >= deadline {
                self.state = CdState::Done;
                return true;
            }
        }
        false
    }

    /// Segundos mostrados: arredonda para cima, então só chega a 0 ao terminar.
    pub fn display_secs(&self, now: u64) -> u64 {
        self.remaining(now).div_ceil(1000)
    }

    /// Próximo instante em que o valor mostrado muda (ou o prazo vence), se estiver rodando.
    pub fn next_wakeup(&self, now: u64) -> Option<u64> {
        let CdState::Running { deadline } = self.state else { return None };
        let r = deadline.saturating_sub(now);
        if r == 0 {
            return Some(now);
        }
        let secs = r.div_ceil(1000);
        // O mostrador cai de `secs` para `secs - 1` quando restar (secs - 1) * 1000.
        Some(now + (r - (secs - 1) * 1000).max(1))
    }

    /// Fração decorrida (0..=1).
    pub fn progress(&self, now: u64) -> f32 {
        if self.duration == 0 {
            return 0.0;
        }
        (1.0 - self.remaining(now) as f64 / self.duration as f64).clamp(0.0, 1.0) as f32
    }
}

pub const MAX_LAPS: usize = 99;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwState {
    Idle,
    Running { since: u64, base: u64 },
    Paused { elapsed: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stopwatch {
    pub state: SwState,
    /// Tempo acumulado em cada volta (crescente).
    pub laps: Vec<u64>,
}

impl Default for Stopwatch {
    fn default() -> Self {
        Self { state: SwState::Idle, laps: Vec::new() }
    }
}

impl Stopwatch {
    pub fn elapsed(&self, now: u64) -> u64 {
        match self.state {
            SwState::Idle => 0,
            SwState::Running { since, base } => base + now.saturating_sub(since),
            SwState::Paused { elapsed } => elapsed,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, SwState::Running { .. })
    }

    pub fn start(&mut self, now: u64) {
        match self.state {
            SwState::Idle => self.state = SwState::Running { since: now, base: 0 },
            SwState::Paused { elapsed } => self.state = SwState::Running { since: now, base: elapsed },
            SwState::Running { .. } => {}
        }
    }

    pub fn pause(&mut self, now: u64) {
        if self.is_running() {
            self.state = SwState::Paused { elapsed: self.elapsed(now) };
        }
    }

    pub fn toggle(&mut self, now: u64) {
        if self.is_running() {
            self.pause(now);
        } else {
            self.start(now);
        }
    }

    /// Marca uma volta (só enquanto roda; no máximo `MAX_LAPS`).
    pub fn lap(&mut self, now: u64) {
        if self.is_running() && self.laps.len() < MAX_LAPS {
            self.laps.push(self.elapsed(now));
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn next_wakeup(&self, now: u64) -> Option<u64> {
        self.is_running().then(|| now + (1000 - self.elapsed(now) % 1000).max(1))
    }

    /// Duração de cada volta (diferença para a anterior).
    pub fn lap_splits(&self) -> Vec<u64> {
        self.laps.iter().scan(0u64, |prev, &t| {
            let d = t - *prev;
            *prev = t;
            Some(d)
        })
        .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Focus,
    ShortBreak,
    LongBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PomodoroConfig {
    pub focus_min: u32,
    pub short_min: u32,
    pub long_min: u32,
    /// Rodadas de foco até a pausa longa.
    pub rounds: u8,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self { focus_min: 25, short_min: 5, long_min: 15, rounds: 4 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pomodoro {
    pub cfg: PomodoroConfig,
    pub phase: Phase,
    /// Rodada de foco atual, de 1 a `cfg.rounds`.
    pub round: u8,
    pub cd: Countdown,
}

impl Pomodoro {
    pub fn new(cfg: PomodoroConfig) -> Self {
        let mut p = Self { cfg, phase: Phase::Focus, round: 1, cd: Countdown::new(0) };
        p.cd = Countdown::new(p.phase_ms());
        p
    }

    fn phase_ms(&self) -> u64 {
        let min = match self.phase {
            Phase::Focus => self.cfg.focus_min,
            Phase::ShortBreak => self.cfg.short_min,
            Phase::LongBreak => self.cfg.long_min,
        };
        min as u64 * 60_000
    }

    fn enter(&mut self, phase: Phase, round: u8) {
        self.phase = phase;
        self.round = round;
        self.cd = Countdown::new(self.phase_ms());
    }

    /// Vai para a fase seguinte, parada. Foco → pausa curta (ou longa na última rodada) → foco.
    pub fn advance(&mut self) {
        match self.phase {
            Phase::Focus if self.round >= self.cfg.rounds => self.enter(Phase::LongBreak, self.round),
            Phase::Focus => self.enter(Phase::ShortBreak, self.round),
            Phase::ShortBreak => self.enter(Phase::Focus, self.round + 1),
            Phase::LongBreak => self.enter(Phase::Focus, 1),
        }
    }

    /// Ação principal: pausa se roda; se a fase acabou, passa à seguinte e inicia; senão inicia/retoma.
    pub fn primary(&mut self, now: u64) {
        if self.cd.state == CdState::Done {
            self.advance();
        }
        self.cd.toggle(now);
    }

    /// Reinicia a fase atual (parada).
    pub fn reset_phase(&mut self) {
        self.cd = Countdown::new(self.phase_ms());
    }

    /// Volta ao começo do ciclo.
    pub fn reset_all(&mut self) {
        self.enter(Phase::Focus, 1);
    }

    pub fn set_config(&mut self, cfg: PomodoroConfig) {
        self.cfg = cfg;
        self.round = self.round.clamp(1, cfg.rounds.max(1));
        self.cd = Countdown::new(self.phase_ms());
    }

    pub fn poll(&mut self, now: u64) -> bool {
        self.cd.poll(now)
    }
}

/// "05:07" ou "1:05:07" (segundos inteiros).
pub fn fmt_hms(total_secs: u64) -> String {
    let (h, m, s) = (total_secs / 3600, total_secs / 60 % 60, total_secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hms_formatting() {
        assert_eq!(fmt_hms(0), "00:00");
        assert_eq!(fmt_hms(65), "01:05");
        assert_eq!(fmt_hms(25 * 60), "25:00");
        assert_eq!(fmt_hms(3600 + 5 * 60 + 7), "1:05:07");
        assert_eq!(fmt_hms(10 * 3600), "10:00:00");
    }

    #[test]
    fn countdown_runs_pauses_resumes_and_finishes() {
        let mut c = Countdown::new(10_000);
        assert_eq!((c.remaining(0), c.display_secs(0)), (10_000, 10));
        c.start(1_000);
        assert_eq!(c.remaining(4_000), 7_000);
        c.pause(4_000);
        assert_eq!(c.remaining(99_000), 7_000, "pausado não corre");
        c.start(100_000);
        assert_eq!(c.remaining(103_000), 4_000);
        assert!(!c.poll(106_999));
        assert!(c.poll(107_000), "vence exatamente no prazo");
        assert!(!c.poll(108_000), "a transição só é avisada uma vez");
        assert_eq!((c.state, c.remaining(108_000), c.display_secs(108_000)), (CdState::Done, 0, 0));
        c.start(200_000);
        assert_eq!(c.remaining(200_000), 10_000, "depois de Done recomeça do início");
    }

    #[test]
    fn countdown_display_rounds_up_and_wakeups_hit_the_boundaries() {
        let mut c = Countdown::new(5_000);
        c.start(0);
        assert_eq!(c.display_secs(0), 5);
        assert_eq!(c.display_secs(1), 5);
        assert_eq!(c.display_secs(999), 5);
        assert_eq!(c.display_secs(1_000), 4);
        assert_eq!(c.display_secs(4_999), 1);
        assert_eq!(c.next_wakeup(0), Some(1_000), "o mostrador troca de 5 para 4 quando restarem 4000 ms");
        assert_eq!(c.next_wakeup(1), Some(1_000));
        assert_eq!(c.next_wakeup(1_000), Some(2_000));
        assert_eq!(c.next_wakeup(4_500), Some(5_000), "o último acordar é o próprio prazo");
        assert_eq!(c.next_wakeup(5_000), Some(5_000));
        assert_eq!(Countdown::new(5_000).next_wakeup(0), None, "parado não acorda");
        // O acordar sempre avança ou cai no mesmo instante só no prazo.
        for now in 0..5_000 {
            assert!(c.next_wakeup(now).unwrap() > now, "{now}");
        }
    }

    #[test]
    fn countdown_edge_cases() {
        let mut zero = Countdown::new(0);
        zero.start(0);
        assert_eq!(zero.state, CdState::Idle, "duração zero não inicia");
        let mut c = Countdown::new(3_000);
        c.pause(0);
        assert_eq!(c.state, CdState::Idle);
        c.start(0);
        c.start(500);
        assert_eq!(c.remaining(500), 2_500, "iniciar de novo não reinicia");
        c.set_duration(60_000);
        assert_eq!((c.state, c.remaining(0)), (CdState::Idle, 60_000));
        assert_eq!(c.progress(0), 0.0);
        c.start(0);
        assert_eq!(c.progress(30_000), 0.5);
        assert_eq!(c.progress(99_000), 1.0);
        // Tempo que andou para trás não faz o prazo crescer.
        assert_eq!(c.remaining(0), 60_000);
    }

    #[test]
    fn stopwatch_accumulates_across_pauses() {
        let mut s = Stopwatch::default();
        assert_eq!(s.elapsed(500), 0);
        s.start(1_000);
        assert_eq!(s.elapsed(3_500), 2_500);
        s.pause(3_500);
        assert_eq!(s.elapsed(9_999), 2_500);
        s.start(10_000);
        assert_eq!(s.elapsed(11_000), 3_500);
        s.reset();
        assert_eq!((s.state, s.laps.len()), (SwState::Idle, 0));
    }

    #[test]
    fn stopwatch_laps_only_while_running_and_are_capped() {
        let mut s = Stopwatch::default();
        s.lap(100);
        assert!(s.laps.is_empty(), "parado não marca volta");
        s.start(0);
        s.lap(4_000);
        s.lap(10_500);
        assert_eq!(s.laps, vec![4_000, 10_500]);
        assert_eq!(s.lap_splits(), vec![4_000, 6_500]);
        s.pause(11_000);
        s.lap(12_000);
        assert_eq!(s.laps.len(), 2);
        s.start(12_000);
        for i in 0..200 {
            s.lap(13_000 + i);
        }
        assert_eq!(s.laps.len(), MAX_LAPS);
    }

    #[test]
    fn stopwatch_wakeups_follow_the_displayed_second() {
        let mut s = Stopwatch::default();
        assert_eq!(s.next_wakeup(0), None);
        s.start(0);
        assert_eq!(s.next_wakeup(0), Some(1_000));
        assert_eq!(s.next_wakeup(250), Some(1_000));
        assert_eq!(s.next_wakeup(1_000), Some(2_000));
        for now in 0..3_000 {
            assert!(s.next_wakeup(now).unwrap() > now);
        }
    }

    #[test]
    fn pomodoro_cycles_focus_break_and_a_long_break() {
        let mut p = Pomodoro::new(PomodoroConfig { focus_min: 25, short_min: 5, long_min: 15, rounds: 3 });
        let seq: Vec<(Phase, u8)> = (0..7)
            .map(|_| {
                let cur = (p.phase, p.round);
                p.advance();
                cur
            })
            .collect();
        assert_eq!(
            seq,
            vec![
                (Phase::Focus, 1),
                (Phase::ShortBreak, 1),
                (Phase::Focus, 2),
                (Phase::ShortBreak, 2),
                (Phase::Focus, 3),
                (Phase::LongBreak, 3),
                (Phase::Focus, 1),
            ],
            "a pausa longa vem depois da última rodada e o ciclo recomeça"
        );
    }

    #[test]
    fn pomodoro_primary_action_drives_the_phases() {
        let mut p = Pomodoro::new(PomodoroConfig { focus_min: 1, short_min: 1, long_min: 2, rounds: 2 });
        p.primary(0);
        assert!(p.cd.is_running());
        p.primary(10_000);
        assert!(matches!(p.cd.state, CdState::Paused { remaining: 50_000 }));
        p.primary(20_000);
        assert!(p.poll(70_000), "o foco termina");
        assert_eq!((p.phase, p.cd.state), (Phase::Focus, CdState::Done), "espera o usuário");
        p.primary(80_000);
        assert_eq!(p.phase, Phase::ShortBreak, "o próximo toque passa à pausa e já inicia");
        assert!(p.cd.is_running());
        assert_eq!(p.cd.remaining(80_000), 60_000);
    }

    #[test]
    fn pomodoro_reset_and_config_change() {
        let mut p = Pomodoro::new(PomodoroConfig::default());
        p.advance();
        p.advance();
        assert_eq!((p.phase, p.round), (Phase::Focus, 2));
        p.primary(0);
        p.reset_phase();
        assert_eq!((p.phase, p.round, p.cd.state), (Phase::Focus, 2, CdState::Idle));
        p.reset_all();
        assert_eq!((p.phase, p.round), (Phase::Focus, 1));
        p.set_config(PomodoroConfig { focus_min: 50, short_min: 10, long_min: 30, rounds: 2 });
        assert_eq!(p.cd.duration, 50 * 60_000);
        p.round = 4;
        p.set_config(PomodoroConfig { rounds: 3, ..p.cfg });
        assert_eq!(p.round, 3, "a rodada nunca passa do novo limite");
    }
}
