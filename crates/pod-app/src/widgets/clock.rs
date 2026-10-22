//! Relógio: hora local em três formatos (1×1, 2×1 e 2×2 analógico). A view é função pura do
//! estado e da hora do contexto; o widget só assina a hora e invalida quando ela vira.

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::time::{ptbr, Granularity, TimeInfo};
use pod_model::view::{Align, Kind, Node, Role, TextStyle, View, VisualKind};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, MenuItem, Outcome, SizeDip, Snapshot, StopReason,
    ViewCtx, Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.clock");
pub const STATE_VERSION: u32 = 1;
const CMD_H24: u32 = 1;
const CMD_SECONDS: u32 = 2;

pub fn descriptor() -> Descriptor {
    let shapes = vec![Shape::S1X1, Shape::S2X1, Shape::S2X2];
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X1.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes,
        default_shape: Some(Shape::S2X1),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Settings {
    h24: bool,
    seconds: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { h24: true, seconds: false }
    }
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let settings = match restore {
        None => Settings::default(),
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) => serde_json::from_str(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    Ok(Box::new(Clock { settings, stopped: false }))
}

struct Clock {
    settings: Settings,
    stopped: bool,
}

impl Clock {
    fn granularity(&self) -> Granularity {
        if self.settings.seconds {
            Granularity::Second
        } else {
            Granularity::Minute
        }
    }
}

/// "10:42" ou "10:42:07"; em 12 horas, "10:42 PM" (o sufixo só nas formas largas).
pub fn digital_text(t: &TimeInfo, s_h24: bool, seconds: bool, suffix: bool) -> String {
    let (h, am_pm) = if s_h24 {
        (t.hour, "")
    } else {
        (if t.hour % 12 == 0 { 12 } else { t.hour % 12 }, if t.hour < 12 { " AM" } else { " PM" })
    };
    let mut out = format!("{h:02}:{:02}", t.minute);
    if seconds {
        out += &format!(":{:02}", t.second);
    }
    if suffix {
        out += am_pm;
    }
    out
}

pub fn date_long(t: &TimeInfo) -> String {
    format!("{}, {} de {}", ptbr::weekday_long(t.weekday), t.day, ptbr::month_long(t.month))
}

fn text(s: String, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s, style, role, align, max_lines: 1 })
}

impl Widget for Clock {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => Outcome {
                invalidate: Invalidation::CONTENT,
                effects: vec![Effect::TimeTicks(Some(self.granularity()))],
            },
            Event::Time(_) | Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            Event::MenuCommand(id) => {
                match *id {
                    CMD_H24 => self.settings.h24 = !self.settings.h24,
                    CMD_SECONDS => self.settings.seconds = !self.settings.seconds,
                    _ => return Outcome::none(),
                }
                Outcome {
                    invalidate: Invalidation::CONTENT,
                    effects: vec![Effect::StateChanged, Effect::TimeTicks(Some(self.granularity()))],
                }
            }
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        let t = &ctx.time;
        let st = self.settings;
        let root = match ctx.shape {
            s if s == Shape::S1X1 => Node::column(0.0, 6.0, vec![
                text(ptbr::weekday_short(t.weekday).to_uppercase(), TextStyle::Caption, Role::Accent, Align::Center)
                    .size(pod_model::view::Len::Fill(1), pod_model::view::Len::Auto),
                text(digital_text(t, st.h24, false, false), TextStyle::Headline, Role::Primary, Align::Center)
                    .size(pod_model::view::Len::Fill(1), pod_model::view::Len::Auto),
            ])
            .centered(),
            s if s == Shape::S2X2 => {
                // Sem segundos o ponteiro dos minutos fica parado dentro do minuto: a view não muda
                // até virar e o diff evita quadros.
                let secs = (t.hour as u32 * 3600 + t.minute as u32 * 60 + if st.seconds { t.second as u32 } else { 0 }) as f32;
                Node::column(8.0, 12.0, vec![
                    Node::visual(VisualKind::AnalogClock { secs, seconds_hand: st.seconds }, 120.0),
                    text(date_long(t), TextStyle::Caption, Role::Secondary, Align::Center)
                        .size(pod_model::view::Len::Fill(1), pod_model::view::Len::Auto),
                ])
                .centered()
            }
            _ => {
                let time_str = digital_text(t, st.h24, st.seconds, true);
                Node::column(2.0, 12.0, vec![
                text(time_str.clone(), if time_str.chars().count() > 8 { TextStyle::Headline } else { TextStyle::Display }, Role::Primary, Align::Center)
                    .size(pod_model::view::Len::Fill(1), pod_model::view::Len::Auto),
                text(date_long(t), TextStyle::Body, Role::Secondary, Align::Center)
                    .size(pod_model::view::Len::Fill(1), pod_model::view::Len::Auto),
                ])
                .centered()
            }
        };
        Some(View::new(root.fill()))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        vec![
            MenuItem { id: CMD_H24, label: "Formato de 24 horas".into(), checked: self.settings.h24, group: None },
            MenuItem { id: CMD_SECONDS, label: "Mostrar segundos".into(), checked: self.settings.seconds, group: None },
        ]
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&self.settings).unwrap_or_default() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        vec![Effect::TimeTicks(None)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::layout::{layout, Rect, TextMeasure};
    use pod_model::time::weekday_of;

    struct Fake;
    impl TextMeasure for Fake {
        fn measure(&self, text: &str, style: TextStyle, max_w: Option<f32>, _lines: u8) -> (f32, f32) {
            let w = text.chars().count() as f32 * style.size_dip() * 0.55;
            (w.min(max_w.unwrap_or(w)), style.line_dip())
        }
    }

    fn at(h: u8, m: u8, s: u8) -> TimeInfo {
        TimeInfo { year: 2026, month: 10, day: 18, weekday: weekday_of(2026, 10, 18), hour: h, minute: m, second: s, millis: 0 }
    }

    fn make(settings: Settings) -> Box<dyn Widget> {
        let snap = Snapshot { version: 1, data: serde_json::to_string(&settings).unwrap() };
        create(Some(&snap)).unwrap()
    }

    #[test]
    fn digital_text_in_24_and_12_hours() {
        assert_eq!(digital_text(&at(9, 5, 7), true, false, true), "09:05");
        assert_eq!(digital_text(&at(9, 5, 7), true, true, true), "09:05:07");
        assert_eq!(digital_text(&at(0, 5, 0), false, false, true), "12:05 AM");
        assert_eq!(digital_text(&at(12, 0, 0), false, false, true), "12:00 PM");
        assert_eq!(digital_text(&at(23, 59, 0), false, false, false), "11:59");
    }

    #[test]
    fn date_is_in_portuguese() {
        assert_eq!(date_long(&at(1, 0, 0)), "domingo, 18 de outubro");
    }

    #[test]
    fn every_shape_gives_a_valid_view_that_fits_the_tile() {
        let w = make(Settings { h24: true, seconds: true });
        for shape in descriptor().shapes {
            let v = w.tile_view(&ViewCtx { shape, time: at(23, 59, 59) }).expect("cartão");
            assert_eq!(v.validate(), Ok(()), "{shape}");
            let s = shape.size_dip();
            let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
            let whole = laid[0].rect;
            for l in &laid {
                assert!(whole.contains(&l.rect), "{shape}: {:?} fora do cartão", l.rect);
            }
        }
    }

    #[test]
    fn view_is_stable_within_the_minute_unless_seconds_are_shown() {
        let ctx = |s| ViewCtx { shape: Shape::S2X2, time: at(10, 42, s) };
        let quiet = make(Settings { h24: true, seconds: false });
        assert_eq!(quiet.tile_view(&ctx(1)), quiet.tile_view(&ctx(59)), "sem segundos: sem quadro novo");
        assert_ne!(quiet.tile_view(&ctx(59)), quiet.tile_view(&ViewCtx { shape: Shape::S2X2, time: at(10, 43, 0) }));
        let ticking = make(Settings { h24: true, seconds: true });
        assert_ne!(ticking.tile_view(&ctx(1)), ticking.tile_view(&ctx(2)));
    }

    #[test]
    fn subscribes_on_start_and_follows_the_seconds_setting() {
        let mut w = make(Settings::default());
        let out = w.handle(&Event::Started { now_ms: 0 });
        assert_eq!(out.effects, vec![Effect::TimeTicks(Some(Granularity::Minute))]);
        let out = w.handle(&Event::MenuCommand(CMD_SECONDS));
        assert!(out.effects.contains(&Effect::StateChanged));
        assert!(out.effects.contains(&Effect::TimeTicks(Some(Granularity::Second))));
        assert!(w.menu_items().iter().any(|m| m.id == CMD_SECONDS && m.checked));
        assert_eq!(w.handle(&Event::MenuCommand(99)), Outcome::none());
    }

    #[test]
    fn state_roundtrips_and_bad_or_future_state_is_refused() {
        let w = make(Settings { h24: false, seconds: true });
        let again = create(Some(&w.snapshot())).unwrap();
        assert_eq!(again.snapshot(), w.snapshot());
        assert!(matches!(create(Some(&Snapshot { version: 1, data: "x".into() })), Err(WidgetError::InvalidState(_))));
        assert!(matches!(
            create(Some(&Snapshot { version: 9, data: "{}".into() })),
            Err(WidgetError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn stop_unsubscribes_once() {
        let mut w = make(Settings::default());
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::TimeTicks(None)]);
        assert!(w.stop(StopReason::Removed).is_empty());
    }
}
