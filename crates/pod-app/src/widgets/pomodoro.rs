//! Pomodoro: ciclos de foco e pausas (curta e longa) com rodadas (1×1, 2×1, 2×2). Os ajustes
//! (durações e rodadas) ficam no menu do cartão, agrupados; o fim de cada fase toca o aviso do
//! sistema e espera o usuário iniciar a próxima.

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::timekeeping::{fmt_hms, CdState, Countdown, Phase, Pomodoro as Pomo, PomodoroConfig};
use pod_model::view::{Align, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, MenuItem, Outcome, SizeDip, Snapshot, StopReason,
    ViewCtx, Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

use super::tkit::{button, item, label, time_text};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.pomodoro");
pub const STATE_VERSION: u32 = 1;
const TICK: u64 = 1;
pub const PRIMARY: u32 = 1;
pub const SKIP: u32 = 2;
const CMD_RESET_PHASE: u32 = 3;
const CMD_RESET_ALL: u32 = 4;
const CMD_SOUND: u32 = 5;
const FOCUS_BASE: u32 = 1000;
const SHORT_BASE: u32 = 2000;
const LONG_BASE: u32 = 3000;
const ROUNDS_BASE: u32 = 4000;
pub const FOCUS_MIN: [u32; 7] = [15, 20, 25, 30, 45, 50, 60];
pub const SHORT_MIN: [u32; 4] = [3, 5, 10, 15];
pub const LONG_MIN: [u32; 5] = [10, 15, 20, 30, 45];
pub const ROUNDS: [u8; 5] = [2, 3, 4, 5, 6];

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X1.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S4X1, Shape::S4X2],
        default_shape: Some(Shape::S2X1),
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
enum SavedPhase {
    Focus,
    ShortBreak,
    LongBreak,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    focus_min: u32,
    short_min: u32,
    long_min: u32,
    rounds: u8,
    phase: SavedPhase,
    round: u8,
    remaining_ms: Option<u64>,
    sound: bool,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let d = PomodoroConfig::default();
    let saved = match restore {
        None => Saved {
            focus_min: d.focus_min,
            short_min: d.short_min,
            long_min: d.long_min,
            rounds: d.rounds,
            phase: SavedPhase::Focus,
            round: 1,
            remaining_ms: None,
            sound: true,
        },
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) => serde_json::from_str(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    let ok = FOCUS_MIN.contains(&saved.focus_min)
        && SHORT_MIN.contains(&saved.short_min)
        && LONG_MIN.contains(&saved.long_min)
        && ROUNDS.contains(&saved.rounds)
        && (1..=saved.rounds).contains(&saved.round);
    if !ok {
        return Err(WidgetError::InvalidState("ajustes fora da lista".into()));
    }
    let cfg = PomodoroConfig {
        focus_min: saved.focus_min,
        short_min: saved.short_min,
        long_min: saved.long_min,
        rounds: saved.rounds,
    };
    let mut p = Pomo::new(cfg);
    p.phase = match saved.phase {
        SavedPhase::Focus => Phase::Focus,
        SavedPhase::ShortBreak => Phase::ShortBreak,
        SavedPhase::LongBreak => Phase::LongBreak,
    };
    p.round = saved.round;
    p.reset_phase();
    if let Some(r) = saved.remaining_ms.filter(|r| *r > 0) {
        p.cd.state = CdState::Paused { remaining: r.min(p.cd.duration) };
    }
    Ok(Box::new(Pomodoro { p, sound: saved.sound, now: 0, stopped: false }))
}

struct Pomodoro {
    p: Pomo,
    sound: bool,
    now: u64,
    stopped: bool,
}

impl Pomodoro {
    fn schedule(&self) -> Effect {
        match self.p.cd.next_wakeup(self.now) {
            Some(deadline_ms) => Effect::ScheduleAt { deadline_ms, token: TICK },
            None => Effect::CancelTimer { token: TICK },
        }
    }

    fn changed(&self) -> Outcome {
        Outcome { invalidate: Invalidation::CONTENT, effects: vec![self.schedule(), Effect::StateChanged] }
    }

    fn primary_label(&self) -> &'static str {
        match self.p.cd.state {
            CdState::Idle => "Iniciar",
            CdState::Running { .. } => "Pausar",
            CdState::Paused { .. } => "Seguir",
            CdState::Done => "Pr\u{f3}xima",
        }
    }

    fn phase_title(&self) -> String {
        match self.p.phase {
            Phase::Focus => format!("Foco {}/{}", self.p.round, self.p.cfg.rounds),
            Phase::ShortBreak => "Pausa curta".to_string(),
            Phase::LongBreak => "Pausa longa".to_string(),
        }
    }

    fn phase_role(&self) -> Role {
        if self.p.phase == Phase::Focus {
            Role::Accent
        } else {
            Role::Positive
        }
    }

    /// Rodadas como pontos: preenchidos até a atual.
    fn dots(&self) -> String {
        (1..=self.p.cfg.rounds)
            .map(|r| {
                let filled = r < self.p.round || (r == self.p.round && self.p.phase != Phase::Focus) || (r == self.p.round && self.p.cd.state == CdState::Done);
                if filled { '\u{25CF}' } else { '\u{25CB}' }
            })
            .collect::<Vec<_>>()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn apply_config(&mut self, f: impl FnOnce(&mut PomodoroConfig)) -> Outcome {
        let mut cfg = self.p.cfg;
        f(&mut cfg);
        self.p.set_config(cfg);
        self.changed()
    }
}

impl Widget for Pomodoro {
    fn set_now(&mut self, now_ms: u64) {
        self.now = now_ms;
    }

    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { now_ms } => {
                self.now = *now_ms;
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![] }
            }
            Event::Clicked { button } if button.0 == PRIMARY => {
                self.p.primary(self.now);
                self.changed()
            }
            Event::Clicked { button } if button.0 == SKIP => {
                self.p.advance();
                self.changed()
            }
            Event::MenuCommand(PRIMARY) => {
                self.p.primary(self.now);
                self.changed()
            }
            Event::MenuCommand(SKIP) => {
                self.p.advance();
                self.changed()
            }
            Event::MenuCommand(CMD_RESET_PHASE) => {
                self.p.reset_phase();
                self.changed()
            }
            Event::MenuCommand(CMD_RESET_ALL) => {
                self.p.reset_all();
                self.changed()
            }
            Event::MenuCommand(CMD_SOUND) => {
                self.sound = !self.sound;
                Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::StateChanged] }
            }
            Event::MenuCommand(c) if (FOCUS_BASE..SHORT_BASE).contains(c) && FOCUS_MIN.contains(&(c - FOCUS_BASE)) => {
                self.apply_config(|cfg| cfg.focus_min = c - FOCUS_BASE)
            }
            Event::MenuCommand(c) if (SHORT_BASE..LONG_BASE).contains(c) && SHORT_MIN.contains(&(c - SHORT_BASE)) => {
                self.apply_config(|cfg| cfg.short_min = c - SHORT_BASE)
            }
            Event::MenuCommand(c) if (LONG_BASE..ROUNDS_BASE).contains(c) && LONG_MIN.contains(&(c - LONG_BASE)) => {
                self.apply_config(|cfg| cfg.long_min = c - LONG_BASE)
            }
            Event::MenuCommand(c) if *c >= ROUNDS_BASE && ROUNDS.contains(&((c - ROUNDS_BASE) as u8)) => {
                self.apply_config(|cfg| cfg.rounds = (c - ROUNDS_BASE) as u8)
            }
            Event::Timer { token: TICK, now_ms } => {
                self.now = *now_ms;
                let mut effects = Vec::new();
                if self.p.poll(self.now) {
                    if self.sound {
                        effects.push(Effect::Chime);
                    }
                    effects.push(Effect::StateChanged);
                }
                effects.push(self.schedule());
                Outcome { invalidate: Invalidation::CONTENT, effects }
            }
            Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        let cd: &Countdown = &self.p.cd;
        let secs = cd.display_secs(self.now);
        let role = self.phase_role();
        let primary = button(PRIMARY, self.primary_label());
        let frac = 1.0 - cd.progress(self.now);
        let root = match ctx.shape {
            s if s == Shape::S1X1 => Node::column(2.0, 4.0, vec![
                label(self.phase_title().to_uppercase(), TextStyle::Caption, role, Align::Center),
                label(fmt_hms(secs), TextStyle::Headline, Role::Primary, Align::Center),
                primary,
            ])
            .centered(),
            s if s == Shape::S2X2 => Node::column(6.0, 12.0, vec![
                label(self.phase_title(), TextStyle::Title, Role::Primary, Align::Center),
                time_text(fmt_hms(secs), Role::Primary),
                Node::bar(cd.progress(self.now), 6.0, role),
                label(self.dots(), TextStyle::Caption, role, Align::Center),
                Node::row(8.0, 0.0, vec![primary, button(SKIP, "Pular")]).centered(),
            ])
            .centered(),
            _ => Node::row(10.0, 10.0, vec![
                Node::layers(vec![
                    Node::ring(frac, 64.0, 7.0, role),
                    label(fmt_hms(secs), TextStyle::Caption, Role::Primary, Align::Center),
                ]),
                Node::column(3.0, 0.0, vec![
                    label(self.phase_title(), TextStyle::Body, Role::Primary, Align::Start),
                    label(self.dots(), TextStyle::Caption, role, Align::Start),
                    primary,
                ])
                .size(pod_model::view::Len::Fill(1), pod_model::view::Len::Auto),
            ])
            .centered(),
        };
        Some(View::new(root.fill()))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        let cfg = self.p.cfg;
        let mut items = vec![
            item(PRIMARY, self.primary_label(), false, None),
            item(SKIP, "Pular fase", false, None),
            item(CMD_RESET_PHASE, "Reiniciar fase", false, None),
            item(CMD_RESET_ALL, "Recome\u{e7}ar ciclo", false, None),
        ];
        for m in FOCUS_MIN {
            items.push(item(FOCUS_BASE + m, &format!("{m} min"), m == cfg.focus_min, Some("Foco")));
        }
        for m in SHORT_MIN {
            items.push(item(SHORT_BASE + m, &format!("{m} min"), m == cfg.short_min, Some("Pausa curta")));
        }
        for m in LONG_MIN {
            items.push(item(LONG_BASE + m, &format!("{m} min"), m == cfg.long_min, Some("Pausa longa")));
        }
        for r in ROUNDS {
            items.push(item(ROUNDS_BASE + r as u32, &format!("{r} rodadas"), r == cfg.rounds, Some("Rodadas")));
        }
        items.push(item(CMD_SOUND, "Som ao terminar", self.sound, None));
        items
    }

    fn snapshot(&self) -> Snapshot {
        let cfg = self.p.cfg;
        let remaining = match self.p.cd.state {
            CdState::Running { .. } | CdState::Paused { .. } => Some(self.p.cd.remaining(self.now)),
            _ => None,
        };
        let saved = Saved {
            focus_min: cfg.focus_min,
            short_min: cfg.short_min,
            long_min: cfg.long_min,
            rounds: cfg.rounds,
            phase: match self.p.phase {
                Phase::Focus => SavedPhase::Focus,
                Phase::ShortBreak => SavedPhase::ShortBreak,
                Phase::LongBreak => SavedPhase::LongBreak,
            },
            round: self.p.round,
            remaining_ms: remaining,
            sound: self.sound,
        };
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&saved).unwrap_or_default() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        vec![Effect::CancelTimer { token: TICK }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::layout::{button_rects, layout, Rect, TextMeasure};
    use pod_model::time::{weekday_of, TimeInfo};
    use pod_model::widget::LocalId;

    struct Fake;
    impl TextMeasure for Fake {
        fn measure(&self, text: &str, style: TextStyle, max_w: Option<f32>, _l: u8) -> (f32, f32) {
            let w = text.chars().count() as f32 * style.size_dip() * 0.55;
            (w.min(max_w.unwrap_or(w)), style.line_dip())
        }
    }

    fn ctx(shape: Shape) -> ViewCtx {
        ViewCtx {
            shape,
            time: TimeInfo { year: 2026, month: 10, day: 18, weekday: weekday_of(2026, 10, 18), hour: 10, minute: 0, second: 0, millis: 0 },
        }
    }

    fn click(w: &mut Box<dyn Widget>, now: u64, id: u32) -> Outcome {
        w.set_now(now);
        w.handle(&Event::Clicked { button: LocalId(id) })
    }

    #[test]
    fn a_focus_phase_ends_with_a_chime_and_waits_for_the_user() {
        let mut w = create(None).unwrap();
        click(&mut w, 0, PRIMARY);
        let out = w.handle(&Event::Timer { token: TICK, now_ms: 25 * 60_000 });
        assert!(out.effects.contains(&Effect::Chime));
        assert!(out.effects.contains(&Effect::CancelTimer { token: TICK }), "sem tick depois do fim");
        assert!(w.menu_items().iter().any(|i| i.label == "Pr\u{f3}xima"));
        // O próximo toque entra na pausa curta e já inicia.
        let out = click(&mut w, 25 * 60_000 + 5_000, PRIMARY);
        assert!(out.effects.iter().any(|e| matches!(e, Effect::ScheduleAt { .. })));
        assert!(w.snapshot().data.contains("\"phase\":\"short_break\""), "{}", w.snapshot().data);
    }

    #[test]
    fn settings_come_from_fixed_lists_and_reset_the_phase() {
        let mut w = create(None).unwrap();
        click(&mut w, 0, PRIMARY);
        w.handle(&Event::MenuCommand(FOCUS_BASE + 50));
        let snap = w.snapshot().data;
        assert!(snap.contains("\"focus_min\":50") && snap.contains("\"remaining_ms\":null"), "{snap}");
        assert_eq!(w.handle(&Event::MenuCommand(FOCUS_BASE + 7)), Outcome::none(), "fora da lista");
        w.handle(&Event::MenuCommand(ROUNDS_BASE + 2));
        assert!(w.snapshot().data.contains("\"rounds\":2"));
        let marked: Vec<_> = w.menu_items().into_iter().filter(|i| i.checked).map(|i| (i.group, i.label)).collect();
        assert!(marked.contains(&(Some("Foco"), "50 min".to_string())));
        assert!(marked.contains(&(Some("Rodadas"), "2 rodadas".to_string())));
    }

    #[test]
    fn skip_and_restart_cycle() {
        let mut w = create(None).unwrap();
        click(&mut w, 0, SKIP);
        assert!(w.snapshot().data.contains("short_break"));
        w.handle(&Event::MenuCommand(CMD_RESET_ALL));
        let d = w.snapshot().data;
        assert!(d.contains("\"phase\":\"focus\"") && d.contains("\"round\":1"), "{d}");
    }

    #[test]
    fn restore_validates_and_keeps_the_remaining_time() {
        let good = "{\"focus_min\":25,\"short_min\":5,\"long_min\":15,\"rounds\":4,\"phase\":\"long_break\",\"round\":4,\"remaining_ms\":90000,\"sound\":false}";
        let mut w = create(Some(&Snapshot { version: 1, data: good.into() })).unwrap();
        assert!(w.menu_items().iter().any(|i| i.label == "Seguir"));
        click(&mut w, 0, PRIMARY);
        assert!(w.snapshot().data.contains("90000"));
        for bad in [
            "x",
            "{\"focus_min\":7,\"short_min\":5,\"long_min\":15,\"rounds\":4,\"phase\":\"focus\",\"round\":1,\"remaining_ms\":null,\"sound\":true}",
            "{\"focus_min\":25,\"short_min\":5,\"long_min\":15,\"rounds\":4,\"phase\":\"focus\",\"round\":9,\"remaining_ms\":null,\"sound\":true}",
        ] {
            assert!(matches!(create(Some(&Snapshot { version: 1, data: bad.into() })), Err(WidgetError::InvalidState(_))), "{bad}");
        }
    }

    #[test]
    fn dots_mark_finished_rounds() {
        let w = create(None).unwrap();
        let v = w.tile_view(&ctx(Shape::S2X2)).unwrap();
        assert_eq!(v.validate(), Ok(()));
        let mut w = create(None).unwrap();
        click(&mut w, 0, SKIP); // pausa curta da rodada 1: a rodada 1 já conta
        let items = pod_model::a11y::items(
            &w.tile_view(&ctx(Shape::S2X2)).unwrap(),
            &layout(&w.tile_view(&ctx(Shape::S2X2)).unwrap(), Rect::new(0.0, 0.0, 188.0, 188.0), &Fake),
        );
        assert!(items.iter().any(|i| i.name == "\u{25CF} \u{25CB} \u{25CB} \u{25CB}"), "{items:?}");
    }

    #[test]
    fn every_shape_and_phase_fits_the_tile() {
        let mut w = create(None).unwrap();
        for step in 0..8 {
            for shape in descriptor().shapes {
                let v = w.tile_view(&ctx(shape)).unwrap();
                assert_eq!(v.validate(), Ok(()), "passo {step}");
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{shape} passo {step}: {:?}", l.rect);
                }
                assert_eq!(button_rects(&v, &laid)[0].0, LocalId(PRIMARY));
            }
            click(&mut w, 0, SKIP);
        }
    }

    #[test]
    fn stop_is_idempotent() {
        let mut w = create(None).unwrap();
        assert_eq!(w.stop(StopReason::Shutdown).len(), 1);
        assert!(w.stop(StopReason::Shutdown).is_empty());
    }
}
