//! Stopwatch: cronômetro com voltas (1×1, 2×1, 2×2). Precisão de segundos na tela; o tempo vem do
//! relógio monotônico. Ao reabrir o Pod um cronômetro que rodava volta pausado.

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::timekeeping::{fmt_hms, Stopwatch as Sw, SwState, MAX_LAPS};
use pod_model::view::{Align, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, MenuItem, Outcome, SizeDip, Snapshot, StopReason,
    ViewCtx, Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

use super::tkit::{button, item, label, time_text};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.stopwatch");
pub const STATE_VERSION: u32 = 1;
const TICK: u64 = 1;
pub const PRIMARY: u32 = 1;
/// Segundo botão: "Volta" enquanto roda, "Zerar" quando parado.
pub const SECOND: u32 = 2;
const CMD_RESET: u32 = 3;

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

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    elapsed_ms: u64,
    laps: Vec<u64>,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let saved = match restore {
        None => Saved::default(),
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) => serde_json::from_str::<Saved>(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    if saved.laps.len() > MAX_LAPS || saved.laps.windows(2).any(|w| w[0] > w[1]) {
        return Err(WidgetError::InvalidState("voltas inválidas".into()));
    }
    let mut sw = Sw { state: SwState::Idle, laps: saved.laps };
    if saved.elapsed_ms > 0 {
        sw.state = SwState::Paused { elapsed: saved.elapsed_ms };
    }
    Ok(Box::new(Stopwatch { sw, now: 0, stopped: false }))
}

struct Stopwatch {
    sw: Sw,
    now: u64,
    stopped: bool,
}

impl Stopwatch {
    fn schedule(&self) -> Effect {
        match self.sw.next_wakeup(self.now) {
            Some(deadline_ms) => Effect::ScheduleAt { deadline_ms, token: TICK },
            None => Effect::CancelTimer { token: TICK },
        }
    }

    fn changed(&self) -> Outcome {
        Outcome { invalidate: Invalidation::CONTENT, effects: vec![self.schedule(), Effect::StateChanged] }
    }

    fn primary_label(&self) -> &'static str {
        match self.sw.state {
            SwState::Idle => "Iniciar",
            SwState::Running { .. } => "Pausar",
            SwState::Paused { .. } => "Seguir",
        }
    }

    fn second_label(&self) -> &'static str {
        if self.sw.is_running() {
            "Volta"
        } else {
            "Zerar"
        }
    }

    fn second(&mut self) -> Outcome {
        if self.sw.is_running() {
            self.sw.lap(self.now);
        } else {
            self.sw.reset();
        }
        self.changed()
    }
}

impl Widget for Stopwatch {
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
                self.sw.toggle(self.now);
                self.changed()
            }
            Event::Clicked { button } if button.0 == SECOND => self.second(),
            Event::MenuCommand(PRIMARY) => {
                self.sw.toggle(self.now);
                self.changed()
            }
            Event::MenuCommand(SECOND) => self.second(),
            Event::MenuCommand(CMD_RESET) => {
                self.sw.reset();
                self.changed()
            }
            Event::Timer { token: TICK, now_ms } => {
                self.now = *now_ms;
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![self.schedule()] }
            }
            Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        let secs = self.sw.elapsed(self.now) / 1000;
        let time = time_text(fmt_hms(secs), Role::Primary);
        let root = match ctx.shape {
            s if s == Shape::S1X1 => Node::column(4.0, 6.0, vec![time, button(PRIMARY, self.primary_label())]).centered(),
            s if s == Shape::S2X2 => {
                let splits = self.sw.lap_splits();
                let total = splits.len();
                // Últimas três voltas, a mais recente primeiro.
                let mut rows: Vec<Node> = splits
                    .iter()
                    .enumerate()
                    .rev()
                    .take(3)
                    .map(|(i, d)| {
                        label(
                            format!("Volta {}   {}", i + 1, fmt_hms(d / 1000)),
                            TextStyle::Caption,
                            Role::Secondary,
                            Align::Center,
                        )
                    })
                    .collect();
                if total == 0 {
                    rows.push(label("sem voltas".into(), TextStyle::Caption, Role::Tertiary, Align::Center));
                }
                let mut kids = vec![
                    time,
                    Node::row(8.0, 0.0, vec![button(PRIMARY, self.primary_label()), button(SECOND, self.second_label())])
                        .centered(),
                ];
                kids.extend(rows);
                Node::column(6.0, 12.0, kids).centered()
            }
            _ => Node::column(6.0, 8.0, vec![
                time,
                Node::row(8.0, 0.0, vec![button(PRIMARY, self.primary_label()), button(SECOND, self.second_label())])
                    .centered(),
            ])
            .centered(),
        };
        Some(View::new(root.fill()))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        vec![
            item(PRIMARY, self.primary_label(), false, None),
            item(SECOND, "Marcar volta", false, None),
            item(CMD_RESET, "Zerar", false, None),
        ]
        .into_iter()
        // Sem voltas possíveis (parado) o item "Marcar volta" some do menu.
        .filter(|i| i.id != SECOND || self.sw.is_running())
        .collect()
    }

    fn snapshot(&self) -> Snapshot {
        let elapsed = match self.sw.state {
            SwState::Idle => 0,
            _ => self.sw.elapsed(self.now),
        };
        let saved = Saved { elapsed_ms: elapsed, laps: self.sw.laps.clone() };
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
    fn ticks_follow_the_displayed_second_and_pause_stops_them() {
        let mut w = create(None).unwrap();
        let out = click(&mut w, 500, PRIMARY);
        assert!(out.effects.contains(&Effect::ScheduleAt { deadline_ms: 1_500, token: TICK }), "{:?}", out.effects);
        let out = w.handle(&Event::Timer { token: TICK, now_ms: 1_500 });
        assert!(out.effects.contains(&Effect::ScheduleAt { deadline_ms: 2_500, token: TICK }));
        let out = click(&mut w, 2_100, PRIMARY);
        assert!(out.effects.contains(&Effect::CancelTimer { token: TICK }));
    }

    #[test]
    fn second_button_is_lap_while_running_and_reset_otherwise() {
        let mut w = create(None).unwrap();
        click(&mut w, 0, PRIMARY);
        click(&mut w, 4_000, SECOND);
        click(&mut w, 9_000, SECOND);
        assert!(w.snapshot().data.contains("\"laps\":[4000,9000]"), "{}", w.snapshot().data);
        click(&mut w, 10_000, PRIMARY); // pausa
        assert!(w.menu_items().iter().all(|i| i.id != SECOND), "parado não marca volta");
        click(&mut w, 10_500, SECOND); // Zerar
        assert_eq!(w.snapshot().data, "{\"elapsed_ms\":0,\"laps\":[]}");
    }

    #[test]
    fn snapshot_while_running_keeps_the_elapsed_time_and_restores_paused() {
        let mut w = create(None).unwrap();
        click(&mut w, 1_000, PRIMARY);
        w.handle(&Event::Timer { token: TICK, now_ms: 6_000 });
        let snap = w.snapshot();
        assert!(snap.data.contains("\"elapsed_ms\":5000"), "{}", snap.data);
        let mut again = create(Some(&snap)).unwrap();
        assert!(again.menu_items().iter().any(|i| i.label == "Seguir"));
        click(&mut again, 50_000, PRIMARY);
        again.handle(&Event::Timer { token: TICK, now_ms: 52_000 });
        assert!(again.snapshot().data.contains("\"elapsed_ms\":7000"));
    }

    #[test]
    fn invalid_state_is_refused() {
        for bad in ["x", "{\"elapsed_ms\":1,\"laps\":[5,3]}"] {
            assert!(matches!(create(Some(&Snapshot { version: 1, data: bad.into() })), Err(WidgetError::InvalidState(_))), "{bad}");
        }
        let many = format!("{{\"elapsed_ms\":1,\"laps\":[{}]}}", vec!["1"; MAX_LAPS + 1].join(","));
        assert!(create(Some(&Snapshot { version: 1, data: many })).is_err());
        assert!(matches!(create(Some(&Snapshot { version: 3, data: "{}".into() })), Err(WidgetError::UnsupportedVersion { .. })));
    }

    #[test]
    fn every_shape_fits_with_laps_and_hours() {
        let mut w = create(None).unwrap();
        click(&mut w, 0, PRIMARY);
        for t in [3_000u64, 7_000, 20_000, 3_700_000] {
            click(&mut w, t, SECOND);
        }
        w.handle(&Event::Timer { token: TICK, now_ms: 3_700_000 });
        for shape in descriptor().shapes {
            let v = w.tile_view(&ctx(shape)).unwrap();
            assert_eq!(v.validate(), Ok(()));
            let s = shape.size_dip();
            let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
            for l in &laid {
                assert!(laid[0].rect.contains(&l.rect), "{shape}: {:?}", l.rect);
            }
            assert_eq!(button_rects(&v, &laid)[0].0, LocalId(PRIMARY));
        }
    }

    #[test]
    fn stop_is_idempotent() {
        let mut w = create(None).unwrap();
        assert_eq!(w.stop(StopReason::Removed).len(), 1);
        assert!(w.stop(StopReason::Removed).is_empty());
    }
}
