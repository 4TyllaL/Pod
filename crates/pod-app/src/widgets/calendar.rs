//! Calendar: data de hoje e eventos (1×1, 2×1) e grade do mês (2×2), com os eventos compartilhados.
//! Semana começando no domingo.

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::shared::{Date, SharedData};
use pod_model::time::{month_grid, ptbr, Granularity};
use pod_model::view::{Align, Fill, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, MenuItem, Outcome, PanelKind, SizeDip, Snapshot,
    StopReason, ViewCtx, Widget, WidgetError,
};

use super::tkit::{item, label};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.calendar");
pub const STATE_VERSION: u32 = 1;
const CMD_EDIT: u32 = 1;
const WEEKDAY_INITIALS: [&str; 7] = ["D", "S", "T", "Q", "Q", "S", "S"];

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X2.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S2X3, Shape::S2X4, Shape::S4X1, Shape::S4X2, Shape::S4X4],
        default_shape: Some(Shape::S2X2),
    }
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(Calendar { shared: SharedData::default(), stopped: false }))
}

struct Calendar {
    shared: SharedData,
    stopped: bool,
}

fn cell(text: String, style: TextStyle, role: Role, h: f32) -> Node {
    Node::new(Kind::Text { text, style, role, align: Align::Center, max_lines: 1 }).size(Len::Fill(1), Len::Fixed(h))
}

impl Calendar {
    /// Linhas "data · título" dos próximos `max` eventos a partir de hoje.
    fn agenda(&self, today: Date, max: usize) -> Vec<Node> {
        let upcoming: Vec<_> = self.shared.events().iter().filter(|e| e.date >= today).take(max).collect();
        if upcoming.is_empty() {
            return vec![label("Sem eventos pr\u{f3}ximos".into(), TextStyle::Caption, Role::Secondary, Align::Start)];
        }
        upcoming
            .into_iter()
            .map(|e| {
                let when = if e.date == today { "Hoje".to_string() } else { format!("{:02}/{:02}", e.date.day, e.date.month) };
                Node::row(8.0, 0.0, vec![
                    label(when, TextStyle::Caption, Role::Accent, Align::Start).size(Len::Fixed(44.0), Len::Auto),
                    label(e.title.clone(), TextStyle::Caption, Role::Primary, Align::Start).size(Len::Fill(1), Len::Auto),
                ])
                .size(Len::Fill(1), Len::Auto)
            })
            .collect()
    }

    fn month_view(&self, today: Date) -> Node {
        let mut rows: Vec<Node> = vec![
            label(
                format!("{} {}", ptbr::month_long(today.month), today.year),
                TextStyle::Title,
                Role::Accent,
                Align::Start,
            ),
            Node::row(0.0, 0.0, WEEKDAY_INITIALS.iter().map(|d| cell((*d).to_string(), TextStyle::Caption, Role::Tertiary, 14.0)).collect())
                .size(Len::Fill(1), Len::Auto),
        ];
        for week in month_grid(today.year, today.month) {
            let cells = week
                .iter()
                .map(|&d| {
                    if d == 0 {
                        return cell(String::new(), TextStyle::Caption, Role::Primary, 18.0);
                    }
                    let date = Date { year: today.year, month: today.month, day: d };
                    let has_event = self.shared.events_on(date).next().is_some();
                    let role = if has_event { Role::Accent } else { Role::Primary };
                    if date == today {
                        // Hoje: placa com o tom do acento por baixo do número.
                        Node::plate(Fill::Accent, 9.0, 2.0, cell(d.to_string(), TextStyle::Caption, role, 14.0))
                            .size(Len::Fill(1), Len::Fixed(18.0))
                    } else {
                        cell(d.to_string(), TextStyle::Caption, role, 18.0)
                    }
                })
                .collect();
            rows.push(Node::row(0.0, 0.0, cells).size(Len::Fill(1), Len::Auto));
        }
        Node::column(3.0, 12.0, rows)
    }
}

impl Widget for Calendar {
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
        let t = &ctx.time;
        let today = Date::from_time(t);
        let root = match ctx.shape {
            s if s.cols >= 3 && s.rows == 1 => {
                // Data grande, mês e os próximos dois eventos em linha.
                let mut kids = vec![
                    Node::column(0.0, 0.0, vec![
                        label(ptbr::weekday_short(t.weekday).to_uppercase(), TextStyle::Caption, Role::Accent, Align::Center),
                        label(t.day.to_string(), TextStyle::Display, Role::Primary, Align::Center),
                    ])
                    .size(Len::Fixed(60.0), Len::Auto),
                ];
                let mut agenda = vec![label(format!("{} {}", ptbr::month_long(t.month), t.year), TextStyle::Body, Role::Primary, Align::Start)];
                agenda.extend(self.agenda(today, 2));
                kids.push(Node::column(3.0, 0.0, agenda).size(Len::Fill(1), Len::Auto));
                Node::row(12.0, 12.0, kids).centered()
            }
            s if s.cols >= 3 => {
                let mut agenda = vec![label("Pr\u{f3}ximos".into(), TextStyle::Title, Role::Primary, Align::Start)];
                agenda.extend(self.agenda(today, s.rows as usize * 3));
                Node::row(0.0, 0.0, vec![
                    self.month_view(today).size(Len::Fill(1), Len::Fill(1)),
                    Node::column(6.0, 14.0, agenda).size(Len::Fill(1), Len::Fill(1)),
                ])
            }
            s if s == Shape::S2X2 => self.month_view(today),
            s if s.cols == 2 && s.rows > 2 => {
                // Mês em cima e a agenda dos próximos eventos embaixo (3 no 2×3, 6 no 2×4).
                let max = (s.rows as usize - 2) * 3;
                let mut kids = vec![Node::divider()];
                let upcoming: Vec<_> = self.shared.events().iter().filter(|e| e.date >= today).take(max).collect();
                if upcoming.is_empty() {
                    kids.push(label("Sem eventos pr\u{f3}ximos".into(), TextStyle::Caption, Role::Secondary, Align::Start).size(Len::Fill(1), Len::Auto));
                }
                for e in upcoming {
                    let when = if e.date == today { "Hoje".to_string() } else { format!("{:02}/{:02}", e.date.day, e.date.month) };
                    kids.push(
                        Node::row(8.0, 0.0, vec![
                            label(when, TextStyle::Caption, Role::Accent, Align::Start).size(Len::Fixed(44.0), Len::Auto),
                            label(e.title.clone(), TextStyle::Caption, Role::Primary, Align::Start).size(Len::Fill(1), Len::Auto),
                        ])
                        .size(Len::Fill(1), Len::Auto),
                    );
                }
                Node::column(0.0, 0.0, vec![self.month_view(today).size(Len::Fill(1), Len::Auto), Node::column(5.0, 12.0, kids).size(Len::Fill(1), Len::Auto)])
                    .size(Len::Fill(1), Len::Fill(1))
            }
            s if s == Shape::S1X1 => {
                // Dia da semana em vermelho, número grande e o próximo evento com uma barra de cor.
                let next = match self.shared.next_event(today) {
                    Some(e) => {
                        Node::row(6.0, 0.0, vec![
                            Node::plate(Fill::Gradient { top: [0xFF, 0x3B, 0x30], bottom: [0xFF, 0x3B, 0x30] }, 1.5, 0.0, Node::spacer())
                                .size(Len::Fixed(3.0), Len::Fixed(14.0)),
                            label(e.title.clone(), TextStyle::Caption, Role::Primary, Align::Start),
                        ])
                        .size(Len::Fill(1), Len::Auto)
                    }
                    None => label("Sem eventos".into(), TextStyle::Caption, Role::Secondary, Align::Start),
                };
                Node::column(0.0, 8.0, vec![
                    label(ptbr::weekday_short(t.weekday).to_uppercase(), TextStyle::Caption, Role::Red, Align::Start),
                    label(t.day.to_string(), TextStyle::Display, Role::Primary, Align::Start),
                    Node::spacer().size(Len::Fill(1), Len::Fill(1)),
                    next,
                ])
            }
            _ => {
                let next = self.shared.next_event(today);
                let next_line = match next {
                    Some(e) if e.date == today => format!("Hoje \u{b7} {}", e.title),
                    Some(e) => format!("{:02}/{:02} \u{b7} {}", e.date.day, e.date.month, e.title),
                    None => "Sem eventos pr\u{f3}ximos".to_string(),
                };
                Node::row(10.0, 12.0, vec![
                    Node::column(0.0, 0.0, vec![
                        label(ptbr::weekday_short(t.weekday).to_uppercase(), TextStyle::Caption, Role::Accent, Align::Center),
                        label(t.day.to_string(), TextStyle::Display, Role::Primary, Align::Center),
                    ])
                    .size(Len::Fixed(60.0), Len::Auto)
                    .centered(),
                    Node::column(3.0, 0.0, vec![
                        label(format!("{} {}", ptbr::month_long(t.month), t.year), TextStyle::Body, Role::Primary, Align::Start),
                        label(next_line, TextStyle::Caption, if next.is_some() { Role::Accent } else { Role::Secondary }, Align::Start),
                    ])
                    .size(Len::Fill(1), Len::Auto),
                ])
                .centered()
            }
        };
        Some(View::new(root.fill()))
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
    use pod_model::layout::{layout, Rect, TextMeasure};
    use pod_model::time::{weekday_of, TimeInfo};

    struct Fake;
    impl TextMeasure for Fake {
        fn measure(&self, text: &str, style: TextStyle, max_w: Option<f32>, _l: u8) -> (f32, f32) {
            let w = text.chars().count() as f32 * style.size_dip() * 0.55;
            (w.min(max_w.unwrap_or(w)), style.line_dip())
        }
    }

    fn ctx(shape: Shape, month: u8, day: u8) -> ViewCtx {
        ViewCtx {
            shape,
            time: TimeInfo { year: 2026, month, day, weekday: weekday_of(2026, month, day), hour: 10, minute: 0, second: 0, millis: 0 },
        }
    }

    fn with_events() -> Box<dyn Widget> {
        let mut s = SharedData::default();
        s.add_event(Date::new(2026, 10, 20).unwrap(), "Reuni\u{e3}o").unwrap();
        s.add_event(Date::new(2026, 10, 18).unwrap(), "Anivers\u{e1}rio").unwrap();
        let mut w = create(None).unwrap();
        w.set_shared(&s);
        w
    }

    fn names(w: &Box<dyn Widget>, shape: Shape, month: u8, day: u8) -> Vec<String> {
        let v = w.tile_view(&ctx(shape, month, day)).unwrap();
        let s = shape.size_dip();
        let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
        pod_model::a11y::items(&v, &laid).into_iter().map(|i| i.name).collect()
    }

    #[test]
    fn month_view_lists_every_day_and_the_month_title() {
        let w = with_events();
        let n = names(&w, Shape::S2X2, 10, 18);
        assert_eq!(n[0], "outubro 2026");
        let days: Vec<&String> = n.iter().filter(|s| s.chars().all(|c| c.is_ascii_digit())).collect();
        assert_eq!(days.len(), 31);
        assert!(n.iter().any(|s| s == "D"), "iniciais dos dias da semana");
    }

    #[test]
    fn today_and_event_days_are_marked() {
        let w = with_events();
        let v = w.tile_view(&ctx(Shape::S2X2, 10, 18)).unwrap();
        // Hoje tem uma placa de acento; o dia 20 tem texto em destaque.
        fn count(n: &Node, f: &dyn Fn(&Node) -> bool) -> usize {
            usize::from(f(n)) + n.children().iter().map(|c| count(c, f)).sum::<usize>()
        }
        assert_eq!(count(&v.root, &|n| matches!(n.kind, Kind::Plate { .. })), 1);
        assert_eq!(
            count(&v.root, &|n| matches!(&n.kind, Kind::Text { role: Role::Accent, text, .. } if text.len() <= 2)),
            2,
            "dias 18 (aniversário) e 20 (reunião)"
        );
    }

    #[test]
    fn small_shapes_show_the_date_and_the_next_event() {
        let w = with_events();
        let one = names(&w, Shape::S1X1, 10, 18);
        assert_eq!(one, vec!["DOM", "18", "Anivers\u{e1}rio"]);
        let wide = names(&w, Shape::S2X1, 10, 19);
        assert!(wide.contains(&"20/10 \u{b7} Reuni\u{e3}o".to_string()), "{wide:?}");
        let today = names(&w, Shape::S2X1, 10, 18);
        assert!(today.contains(&"Hoje \u{b7} Anivers\u{e1}rio".to_string()), "{today:?}");
        let none = names(&create(None).unwrap(), Shape::S2X1, 10, 18);
        assert!(none.contains(&"Sem eventos pr\u{f3}ximos".to_string()));
    }

    #[test]
    fn every_month_of_the_year_fits_the_tile() {
        let w = with_events();
        for month in 1..=12u8 {
            for shape in descriptor().shapes {
                let v = w.tile_view(&ctx(shape, month, 1)).unwrap();
                assert_eq!(v.validate(), Ok(()), "{month} {shape}");
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "mês {month} {shape}: {:?}", l.rect);
                }
            }
        }
    }

    #[test]
    fn menu_opens_the_panel_and_stop_unsubscribes() {
        let mut w = create(None).unwrap();
        assert_eq!(w.handle(&Event::MenuCommand(CMD_EDIT)).effects, vec![Effect::OpenPanel(PanelKind::Events)]);
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::TimeTicks(None)]);
        assert!(w.stop(StopReason::Removed).is_empty());
        assert!(matches!(create(Some(&Snapshot { version: 2, data: String::new() })), Err(WidgetError::UnsupportedVersion { .. })));
    }
}
