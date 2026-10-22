//! Timer: contagem regressiva com duração por instância (1×1, 2×1, 2×2). Funciona pelo relógio
//! monotônico do runtime; ao reabrir o Pod um timer que rodava volta pausado com o tempo restante.

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::timekeeping::{fmt_hms, CdState, Countdown};
use pod_model::view::{Node, Role, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, MenuItem, Outcome, SizeDip, Snapshot, StopReason,
    ViewCtx, Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

use super::tkit::{button, item, label, time_text};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.timer");
pub const STATE_VERSION: u32 = 1;
const TICK: u64 = 1;
pub const PRIMARY: u32 = 1;
pub const RESET: u32 = 2;
const CMD_SOUND: u32 = 3;
const CMD_DURATION_BASE: u32 = 1000;
pub const DURATIONS_MIN: [u32; 10] = [1, 3, 5, 10, 15, 20, 25, 30, 45, 60];
const DEFAULT_MIN: u32 = 5;

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

#[derive(Serialize, Deserialize)]
struct Saved {
    duration_min: u32,
    /// Tempo restante de um timer em andamento ou pausado; `None` = parado no início.
    remaining_ms: Option<u64>,
    sound: bool,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let saved = match restore {
        None => Saved { duration_min: DEFAULT_MIN, remaining_ms: None, sound: true },
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) => serde_json::from_str(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    if !(1..=24 * 60).contains(&saved.duration_min) {
        return Err(WidgetError::InvalidState(format!("duração fora da faixa: {}", saved.duration_min)));
    }
    let mut cd = Countdown::new(saved.duration_min as u64 * 60_000);
    if let Some(r) = saved.remaining_ms.filter(|r| *r > 0) {
        cd.state = CdState::Paused { remaining: r.min(cd.duration) };
    }
    Ok(Box::new(Timer { cd, sound: saved.sound, now: 0, stopped: false }))
}

struct Timer {
    cd: Countdown,
    sound: bool,
    now: u64,
    stopped: bool,
}

impl Timer {
    /// Efeitos para manter o prazo do próximo segundo (ou cancelá-lo se não está rodando).
    fn schedule(&self) -> Effect {
        match self.cd.next_wakeup(self.now) {
            Some(deadline_ms) => Effect::ScheduleAt { deadline_ms, token: TICK },
            None => Effect::CancelTimer { token: TICK },
        }
    }

    fn changed(&self) -> Outcome {
        Outcome { invalidate: Invalidation::CONTENT, effects: vec![self.schedule(), Effect::StateChanged] }
    }

    fn primary_label(&self) -> &'static str {
        match self.cd.state {
            CdState::Idle => "Iniciar",
            CdState::Running { .. } => "Pausar",
            CdState::Paused { .. } => "Seguir",
            CdState::Done => "De novo",
        }
    }
}

impl Widget for Timer {
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
                self.cd.toggle(self.now);
                self.changed()
            }
            Event::Clicked { button } if button.0 == RESET => {
                self.cd.reset();
                self.changed()
            }
            Event::MenuCommand(PRIMARY) => {
                self.cd.toggle(self.now);
                self.changed()
            }
            Event::MenuCommand(RESET) => {
                self.cd.reset();
                self.changed()
            }
            Event::MenuCommand(CMD_SOUND) => {
                self.sound = !self.sound;
                Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::StateChanged] }
            }
            Event::MenuCommand(c) if *c > CMD_DURATION_BASE && DURATIONS_MIN.contains(&(c - CMD_DURATION_BASE)) => {
                self.cd.set_duration((c - CMD_DURATION_BASE) as u64 * 60_000);
                self.changed()
            }
            Event::Timer { token: TICK, now_ms } => {
                self.now = *now_ms;
                let mut effects = Vec::new();
                if self.cd.poll(self.now) {
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
        let secs = self.cd.display_secs(self.now);
        let done = self.cd.state == CdState::Done;
        let role = if done { Role::Positive } else { Role::Primary };
        let time = time_text(fmt_hms(secs), role);
        let primary = button(PRIMARY, self.primary_label());
        let root = match ctx.shape {
            s if s == Shape::S1X1 => Node::column(4.0, 6.0, vec![time, primary]).centered(),
            s if s == Shape::S2X2 => {
                let remaining_frac = 1.0 - self.cd.progress(self.now);
                let ring_role = if done { Role::Positive } else { Role::Accent };
                Node::column(8.0, 12.0, vec![
                    Node::layers(vec![Node::ring(remaining_frac, 112.0, 9.0, ring_role), time]),
                    label(
                        if done { "Conclu\u{ed}do".to_string() } else { format!("{} min", self.cd.duration / 60_000) },
                        pod_model::view::TextStyle::Caption,
                        Role::Secondary,
                        pod_model::view::Align::Center,
                    ),
                    Node::row(8.0, 0.0, vec![primary, button(RESET, "Zerar")]).centered(),
                ])
                .centered()
            }
            _ => Node::column(6.0, 8.0, vec![
                time_text(fmt_hms(secs), role),
                Node::row(8.0, 0.0, vec![button(PRIMARY, self.primary_label()), button(RESET, "Zerar")]).centered(),
            ])
            .centered(),
        };
        Some(View::new(root.fill()))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        let mut items = vec![
            item(PRIMARY, self.primary_label(), false, None),
            item(RESET, "Zerar", false, None),
        ];
        let current = (self.cd.duration / 60_000) as u32;
        for m in DURATIONS_MIN {
            items.push(item(CMD_DURATION_BASE + m, &format!("{m} min"), m == current, Some("Dura\u{e7}\u{e3}o")));
        }
        items.push(item(CMD_SOUND, "Som ao terminar", self.sound, None));
        items
    }

    fn snapshot(&self) -> Snapshot {
        let remaining = match self.cd.state {
            CdState::Running { .. } | CdState::Paused { .. } => Some(self.cd.remaining(self.now)),
            _ => None,
        };
        let saved = Saved { duration_min: (self.cd.duration / 60_000) as u32, remaining_ms: remaining, sound: self.sound };
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
    use pod_model::view::TextStyle;
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

    fn make(min: u32) -> Box<dyn Widget> {
        let snap = Snapshot { version: 1, data: format!("{{\"duration_min\":{min},\"remaining_ms\":null,\"sound\":true}}") };
        create(Some(&snap)).unwrap()
    }

    fn click(w: &mut Box<dyn Widget>, now: u64, id: u32) -> Outcome {
        w.set_now(now);
        w.handle(&Event::Clicked { button: LocalId(id) })
    }

    #[test]
    fn start_schedules_the_next_second_and_pause_cancels() {
        let mut w = make(1);
        w.handle(&Event::Started { now_ms: 0 });
        let out = click(&mut w, 10_000, PRIMARY);
        assert!(out.effects.contains(&Effect::ScheduleAt { deadline_ms: 11_000, token: TICK }), "{:?}", out.effects);
        assert!(out.effects.contains(&Effect::StateChanged));
        let out = click(&mut w, 12_500, PRIMARY);
        assert!(out.effects.contains(&Effect::CancelTimer { token: TICK }));
        // Pausado com 57,5 s restantes: o mostrador arredonda para cima.
        assert!(w.snapshot().data.contains("\"remaining_ms\":57500"), "{}", w.snapshot().data);
    }

    #[test]
    fn finishing_chimes_once_and_stops_ticking() {
        let mut w = make(1);
        click(&mut w, 0, PRIMARY);
        let out = w.handle(&Event::Timer { token: TICK, now_ms: 60_000 });
        assert!(out.effects.contains(&Effect::Chime));
        assert!(out.effects.contains(&Effect::CancelTimer { token: TICK }));
        // Um tick atrasado depois do fim não toca de novo.
        let again = w.handle(&Event::Timer { token: TICK, now_ms: 61_000 });
        assert!(!again.effects.contains(&Effect::Chime));
        let v = w.tile_view(&ctx(Shape::S1X1)).unwrap();
        assert_eq!(v.validate(), Ok(()));
    }

    #[test]
    fn sound_can_be_turned_off() {
        let mut w = make(1);
        w.handle(&Event::MenuCommand(CMD_SOUND));
        click(&mut w, 0, PRIMARY);
        let out = w.handle(&Event::Timer { token: TICK, now_ms: 60_000 });
        assert!(!out.effects.contains(&Effect::Chime));
        assert!(w.snapshot().data.contains("\"sound\":false"));
    }

    #[test]
    fn duration_menu_sets_and_marks_the_choice() {
        let mut w = make(5);
        click(&mut w, 0, PRIMARY);
        w.handle(&Event::MenuCommand(CMD_DURATION_BASE + 25));
        let items = w.menu_items();
        assert!(items.iter().any(|i| i.id == CMD_DURATION_BASE + 25 && i.checked));
        assert!(items.iter().all(|i| i.id != CMD_DURATION_BASE + 5 || !i.checked));
        assert!(w.snapshot().data.contains("\"duration_min\":25") && w.snapshot().data.contains("null"), "mudar a duração zera");
        assert_eq!(w.handle(&Event::MenuCommand(CMD_DURATION_BASE + 7)), Outcome::none(), "duração fora da lista");
    }

    #[test]
    fn restored_running_timer_comes_back_paused_with_its_remaining_time() {
        let snap = Snapshot { version: 1, data: "{\"duration_min\":10,\"remaining_ms\":123000,\"sound\":true}".into() };
        let mut w = create(Some(&snap)).unwrap();
        w.set_now(777);
        let v = w.tile_view(&ctx(Shape::S2X1)).unwrap();
        let laid = layout(&v, Rect::new(0.0, 0.0, 188.0, 88.0), &Fake);
        assert!(laid.len() > 3);
        assert!(w.menu_items().iter().any(|i| i.label == "Seguir"));
        click(&mut w, 1_000, PRIMARY);
        assert!(w.snapshot().data.contains("123000"), "retomar não perde tempo");
    }

    #[test]
    fn bad_state_is_refused() {
        for bad in ["x", "{\"duration_min\":0,\"remaining_ms\":null,\"sound\":true}", "{\"duration_min\":99999,\"remaining_ms\":null,\"sound\":true}"] {
            assert!(matches!(create(Some(&Snapshot { version: 1, data: bad.into() })), Err(WidgetError::InvalidState(_))), "{bad}");
        }
        assert!(matches!(create(Some(&Snapshot { version: 2, data: "{}".into() })), Err(WidgetError::UnsupportedVersion { .. })));
    }

    #[test]
    fn every_shape_fits_and_exposes_the_primary_button_first() {
        for state in [None, Some(60_000u64)] {
            let mut w = make(25);
            if let Some(r) = state {
                click(&mut w, 0, PRIMARY);
                w.handle(&Event::Timer { token: TICK, now_ms: r });
            }
            for shape in descriptor().shapes {
                let v = w.tile_view(&ctx(shape)).unwrap();
                assert_eq!(v.validate(), Ok(()));
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{shape}: {:?}", l.rect);
                }
                assert_eq!(button_rects(&v, &laid)[0].0, LocalId(PRIMARY), "Espaço/Enter aciona o primeiro botão");
            }
        }
    }

    #[test]
    fn stop_is_idempotent() {
        let mut w = make(1);
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::CancelTimer { token: TICK }]);
        assert!(w.stop(StopReason::Removed).is_empty());
        assert_eq!(click(&mut w, 0, PRIMARY), Outcome::none());
    }
}
