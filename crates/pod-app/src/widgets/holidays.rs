//! Feriados: o próximo feriado nacional do Brasil, quantos dias faltam e se vira fim de semana
//! prolongado. Calculado offline (inclui os que dependem da Páscoa); não conhece feriados estaduais.

use pod_model::holidays::{days_until, long_weekend, upcoming, Holiday};
use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::time::{ptbr, Granularity, TimeInfo};
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, Outcome, SizeDip, Snapshot, StopReason, ViewCtx, Widget,
    WidgetError,
};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.holidays");
pub const STATE_VERSION: u32 = 1;

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X2.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S2X3],
        default_shape: Some(Shape::S2X2),
    }
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(Holidays { stopped: false }))
}

struct Holidays {
    stopped: bool,
}

fn text(s: &str, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

fn when(n: i64) -> String {
    match n {
        0 => "hoje".to_string(),
        1 => "amanh\u{e3}".to_string(),
        n => format!("em {n} dias"),
    }
}

fn date_text(h: &Holiday) -> String {
    format!("{}, {:02}/{:02}", ptbr::weekday_short(h.weekday()).to_lowercase(), h.day, h.month)
}

fn view_for(shape: Shape, t: &TimeInfo) -> View {
    let from = (t.year, t.month, t.day);
    let list = upcoming(from, 6);
    let Some(next) = list.first().copied() else { return View::new(Node::spacer()) };
    let n = days_until(from, &next);
    let extra = |h: &Holiday| if long_weekend(h) { " \u{b7} fim de semana prolongado" } else if h.optional { " \u{b7} ponto facultativo" } else { "" };
    let root = if shape == Shape::S1X1 {
        Node::column(2.0, 6.0, vec![
            text(&n.to_string(), TextStyle::Display, if n == 0 { Role::Positive } else { Role::Primary }, Align::Center),
            text(if n == 1 { "dia p/ feriado" } else { "dias p/ feriado" }, TextStyle::Caption, Role::Secondary, Align::Center),
        ])
        .centered()
    } else if shape.rows == 1 {
        Node::row(12.0, 14.0, vec![
            Node::column(0.0, 0.0, vec![
                text(&n.to_string(), TextStyle::Display, if n == 0 { Role::Positive } else { Role::Accent }, Align::Center),
                text("dias", TextStyle::Caption, Role::Secondary, Align::Center),
            ])
            .size(Len::Fixed(64.0), Len::Auto),
            Node::column(2.0, 0.0, vec![
                text(next.name, TextStyle::Body, Role::Primary, Align::Start),
                text(&format!("{}{}", date_text(&next), extra(&next)), TextStyle::Caption, Role::Secondary, Align::Start),
            ])
            .size(Len::Fill(1), Len::Auto),
        ])
        .centered()
    } else {
        let rows = 1 + (shape.rows as usize - 1) * 2; // 3 no 2×2, 5 no 2×3
        let mut kids = vec![
            text("Pr\u{f3}ximo feriado", TextStyle::Caption, Role::Accent, Align::Start),
            text(next.name, TextStyle::Title, Role::Primary, Align::Start),
            text(&format!("{} \u{b7} {}", date_text(&next), when(n)), TextStyle::Body, Role::Secondary, Align::Start),
        ];
        if long_weekend(&next) {
            kids.push(text("Fim de semana prolongado", TextStyle::Caption, Role::Positive, Align::Start));
        }
        kids.push(Node::divider());
        for h in list.iter().skip(1).take(rows) {
            kids.push(
                Node::row(8.0, 0.0, vec![
                    text(&format!("{:02}/{:02}", h.day, h.month), TextStyle::Caption, Role::Accent, Align::Start).size(Len::Fixed(40.0), Len::Auto),
                    text(h.name, TextStyle::Caption, Role::Primary, Align::Start).size(Len::Fill(1), Len::Auto),
                ])
                .size(Len::Fill(1), Len::Auto),
            );
        }
        Node::column(4.0, 14.0, kids)
    };
    View::new(root.fill())
}

impl Widget for Holidays {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => Outcome {
                invalidate: Invalidation::CONTENT,
                effects: vec![Effect::TimeTicks(Some(Granularity::Minute))],
            },
            Event::Time(_) | Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        Some(view_for(ctx.shape, &ctx.time))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: String::new() }
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
    use pod_model::time::weekday_of;

    fn at(y: i32, m: u8, d: u8) -> TimeInfo {
        TimeInfo { year: y, month: m, day: d, weekday: weekday_of(y, m, d), hour: 9, minute: 0, second: 0, millis: 0 }
    }

    fn texts(v: &View) -> Vec<String> {
        fn walk(n: &Node, out: &mut Vec<String>) {
            if let Kind::Text { text, .. } = &n.kind {
                out.push(text.clone());
            }
            n.children().iter().for_each(|c| walk(c, out));
        }
        let mut t = Vec::new();
        walk(&v.root, &mut t);
        t
    }

    #[test]
    fn shows_the_next_holiday_and_the_countdown() {
        // 19/10/2026: o próximo é Finados (2/11, segunda), em 14 dias.
        let t = texts(&view_for(Shape::S2X2, &at(2026, 10, 19)));
        assert!(t.contains(&"Finados".to_string()), "{t:?}");
        assert!(t.iter().any(|s| s.contains("em 14 dias")), "{t:?}");
        assert!(t.contains(&"Fim de semana prolongado".to_string()), "2/11/2026 \u{e9} segunda");
        assert_eq!(texts(&view_for(Shape::S1X1, &at(2026, 10, 19)))[0], "14");
        assert_eq!(texts(&view_for(Shape::S1X1, &at(2026, 11, 2)))[0], "0", "hoje conta");
    }

    #[test]
    fn rolls_over_the_year_and_every_shape_is_valid() {
        let t = texts(&view_for(Shape::S2X2, &at(2026, 12, 26)));
        assert!(t.iter().any(|s| s == "Confraterniza\u{e7}\u{e3}o Universal"), "{t:?}");
        for shape in descriptor().shapes {
            for (y, m, d) in [(2026, 1, 1), (2026, 6, 4), (2026, 12, 31), (2028, 2, 29)] {
                assert!(view_for(shape, &at(y, m, d)).validate().is_ok(), "{shape} {y}-{m}-{d}");
            }
        }
    }

    #[test]
    fn subscribes_to_the_minute_and_keeps_no_state() {
        let mut w = create(None).unwrap();
        assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::TimeTicks(Some(Granularity::Minute))]);
        assert!(w.snapshot().data.is_empty());
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::TimeTicks(None)]);
        assert!(create(Some(&Snapshot { version: 2, data: String::new() })).is_err());
    }
}
