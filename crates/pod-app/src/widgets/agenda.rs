//! Agenda: os compromissos de hoje e dos próximos dias, a partir dos eventos do Calendário (mesmos
//! dados compartilhados). 1×1 (quantos hoje), 2×1 e 2×2/2×3/2×4 (lista agrupada por dia).

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::shared::{CalEvent, Date, SharedData};
use pod_model::time::{ptbr, Granularity};
use pod_model::view::{Align, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, MenuItem, Outcome, PanelKind, SizeDip, Snapshot, StopReason,
    ViewCtx, Widget, WidgetError,
};

use super::tkit::{item, label};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.agenda");
pub const STATE_VERSION: u32 = 1;
const CMD_EDIT: u32 = 1;

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X2.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S2X3, Shape::S2X4, Shape::S4X1, Shape::S4X2],
        default_shape: Some(Shape::S2X2),
    }
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(Agenda { shared: SharedData::default(), stopped: false }))
}

struct Agenda {
    shared: SharedData,
    stopped: bool,
}

/// Rótulo do dia: "Hoje", "Amanhã" ou "seg, 21/10".
fn day_label(d: Date, today: Date) -> String {
    let n = pod_model::habits::day_number(d.year, d.month, d.day) - pod_model::habits::day_number(today.year, today.month, today.day);
    match n {
        0 => "Hoje".to_string(),
        1 => "Amanh\u{e3}".to_string(),
        _ => {
            let wd = pod_model::time::weekday_of(d.year, d.month, d.day);
            format!("{}, {:02}/{:02}", ptbr::weekday_short(wd).to_lowercase(), d.day, d.month)
        }
    }
}

impl Agenda {
    fn upcoming(&self, today: Date, max: usize) -> Vec<&CalEvent> {
        self.shared.events().iter().filter(|e| e.date >= today).take(max).collect()
    }

    fn today_count(&self, today: Date) -> usize {
        self.shared.events_on(today).count()
    }

    /// Linhas agrupadas por dia: um cabeçalho por data seguido dos títulos (até `max` linhas no total).
    fn grouped(&self, today: Date, max: usize) -> Vec<Node> {
        let mut out: Vec<Node> = Vec::new();
        let mut last: Option<Date> = None;
        let mut used = 0;
        for e in self.shared.events().iter().filter(|e| e.date >= today) {
            let header = last != Some(e.date);
            if used + 1 + usize::from(header) > max {
                break;
            }
            if header {
                let role = if e.date == today { Role::Accent } else { Role::Secondary };
                out.push(label(day_label(e.date, today), TextStyle::Caption, role, Align::Start));
                used += 1;
                last = Some(e.date);
            }
            out.push(label(e.title.clone(), TextStyle::Body, Role::Primary, Align::Start));
            used += 1;
        }
        out
    }

    fn view_for(&self, shape: Shape, t: &pod_model::time::TimeInfo) -> View {
        let today = Date::from_time(t);
        let n_today = self.today_count(today);
        let free = || label("Dia livre".into(), TextStyle::Body, Role::Secondary, Align::Center);
        let root = if shape == Shape::S1X1 {
            Node::column(0.0, 6.0, vec![
                label(n_today.to_string(), TextStyle::Display, if n_today == 0 { Role::Positive } else { Role::Primary }, Align::Center),
                label(if n_today == 1 { "evento hoje" } else { "eventos hoje" }.to_string(), TextStyle::Caption, Role::Secondary, Align::Center),
            ])
            .centered()
        } else if shape.rows == 1 {
            // Faixas: os dois próximos compromissos, com a data à esquerda.
            let ev = self.upcoming(today, if shape.cols >= 3 { 3 } else { 2 });
            let rows: Vec<Node> = if ev.is_empty() {
                vec![free()]
            } else {
                ev.into_iter()
                    .map(|e| {
                        Node::row(8.0, 0.0, vec![
                            label(day_label(e.date, today), TextStyle::Caption, Role::Accent, Align::Start).size(Len::Fixed(68.0), Len::Auto),
                            label(e.title.clone(), TextStyle::Body, Role::Primary, Align::Start).size(Len::Fill(1), Len::Auto),
                        ])
                        .size(Len::Fill(1), Len::Auto)
                    })
                    .collect()
            };
            Node::column(3.0, 12.0, rows).centered().size(Len::Fill(1), Len::Fill(1))
        } else {
            let max = 6 + (shape.rows as usize).saturating_sub(2) * 4;
            let mut kids = vec![label("Agenda".into(), TextStyle::Title, Role::Primary, Align::Start)];
            let rows = self.grouped(today, max);
            if rows.is_empty() {
                kids.push(label("Nenhum evento pr\u{f3}ximo".into(), TextStyle::Body, Role::Secondary, Align::Start));
            }
            kids.extend(rows);
            Node::column(5.0, 14.0, kids)
        };
        View::new(root.fill())
    }
}

impl Widget for Agenda {
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
            Event::MenuCommand(CMD_EDIT) => {
                Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::OpenPanel(PanelKind::Events)] }
            }
            _ => Outcome::none(),
        }
    }

    fn set_shared(&mut self, shared: &SharedData) {
        self.shared = shared.clone();
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        Some(self.view_for(ctx.shape, &ctx.time))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        vec![item(CMD_EDIT, "Editar eventos\u{2026}", false, None)]
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
    use pod_model::time::{weekday_of, TimeInfo};

    fn at(day: u8) -> TimeInfo {
        TimeInfo { year: 2026, month: 10, day, weekday: weekday_of(2026, 10, day), hour: 9, minute: 0, second: 0, millis: 0 }
    }

    fn with(events: &[(u8, &str)]) -> Agenda {
        let mut a = Agenda { shared: SharedData::default(), stopped: false };
        for (i, (d, t)) in events.iter().enumerate() {
            a.shared.add_event(Date { year: 2026, month: 10, day: *d }, t).unwrap_or_else(|_| panic!("evento {i}"));
        }
        a
    }

    fn texts(v: &View) -> Vec<String> {
        fn walk(n: &Node, out: &mut Vec<String>) {
            if let pod_model::view::Kind::Text { text, .. } = &n.kind {
                out.push(text.clone());
            }
            n.children().iter().for_each(|c| walk(c, out));
        }
        let mut t = Vec::new();
        walk(&v.root, &mut t);
        t
    }

    #[test]
    fn groups_by_day_with_today_and_tomorrow() {
        let a = with(&[(19, "Dentista"), (19, "Reuni\u{e3}o"), (20, "Pagar conta"), (25, "Festa")]);
        let t = texts(&a.view_for(Shape::S2X2, &at(19)));
        assert_eq!(t[0], "Agenda");
        assert_eq!(&t[1..4], ["Hoje", "Dentista", "Reuni\u{e3}o"]);
        assert_eq!(&t[4..6], ["Amanh\u{e3}", "Pagar conta"]);
        assert!(!t.iter().any(|s| s.contains("25/10")), "o 2x2 cabe 6 linhas: {t:?}");
        let tall = texts(&a.view_for(Shape::S2X3, &at(19)));
        assert!(tall.iter().any(|s| s.contains("25/10")) && tall.iter().any(|s| s == "Festa"), "{tall:?}");
    }

    #[test]
    fn past_events_are_hidden_and_empty_days_say_so() {
        let a = with(&[(10, "Passou")]);
        let t = texts(&a.view_for(Shape::S2X2, &at(19)));
        assert!(!t.iter().any(|s| s == "Passou"));
        assert!(t.iter().any(|s| s.contains("Nenhum evento")));
        let one = texts(&a.view_for(Shape::S1X1, &at(19)));
        assert_eq!(one[0], "0");
        assert!(texts(&a.view_for(Shape::S2X1, &at(19))).iter().any(|s| s == "Dia livre"));
    }

    #[test]
    fn every_shape_is_valid_and_the_list_is_capped() {
        let many: Vec<(u8, String)> = (19..=28u8).flat_map(|d| [(d, format!("a{d}")), (d, format!("b{d}"))]).collect();
        let refs: Vec<(u8, &str)> = many.iter().map(|(d, s)| (*d, s.as_str())).collect();
        let a = with(&refs);
        for shape in descriptor().shapes {
            let v = a.view_for(shape, &at(19));
            assert!(v.validate().is_ok(), "{shape}");
        }
        assert!(texts(&a.view_for(Shape::S2X2, &at(19))).len() <= 1 + 6);
    }

    #[test]
    fn subscribes_to_the_minute_and_opens_the_events_panel() {
        let mut w = create(None).unwrap();
        assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::TimeTicks(Some(Granularity::Minute))]);
        assert_eq!(w.handle(&Event::MenuCommand(CMD_EDIT)).effects, vec![Effect::OpenPanel(PanelKind::Events)]);
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::TimeTicks(None)]);
    }
}
