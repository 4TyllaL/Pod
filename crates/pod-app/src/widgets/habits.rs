//! Hábitos: marque o que cumpriu hoje e veja a sequência de dias seguidos. 1×1 (quantos hoje), 2×1 e
//! 2×2 (lista; mais linhas nos formatos altos). Adicionar pelo botão ou pelo menu; remover pelo menu.
//! O estado (nomes e dias cumpridos) é durável e fica no próprio widget.

use pod_model::habits::{self, day_number, done_today, Habit, MAX_HABITS};
use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::time::{Granularity, TimeInfo};
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, InputSpec, Invalidation, LocalId, MenuItem, Outcome, SizeDip, Snapshot,
    StopReason, ViewCtx, Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.habits");
pub const STATE_VERSION: u32 = 1;

const ADD: u32 = 900;
/// Ids dos botões/itens: o hábito `i` é `i + 1`.
const ADD_CMD: u32 = 50;
const REMOVE_BASE: u32 = 100;

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X2.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S2X3, Shape::S2X4],
        default_shape: Some(Shape::S2X2),
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    habits: Vec<Habit>,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let saved = match restore {
        None => Saved::default(),
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) if s.data.is_empty() => Saved::default(),
        Some(s) => serde_json::from_str::<Saved>(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    // Estado de arquivo editado à mão: respeita os limites e a ordem dos dias.
    let mut list = saved.habits;
    list.truncate(MAX_HABITS);
    for h in &mut list {
        h.name = habits::clean_name(&h.name).unwrap_or_else(|_| "H\u{e1}bito".into());
        h.days.sort_unstable();
        h.days.dedup();
    }
    Ok(Box::new(Habits { list, today: None, stopped: false }))
}

struct Habits {
    list: Vec<Habit>,
    /// Dia de hoje (atualizado pela hora que o host entrega).
    today: Option<i64>,
    stopped: bool,
}

fn label(s: &str, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

fn today_of(t: &TimeInfo) -> i64 {
    day_number(t.year, t.month, t.day)
}

impl Habits {
    fn row_text(h: &Habit, today: i64) -> String {
        match h.streak(today) {
            0 => h.name.clone(),
            n => format!("{} \u{b7} {n} d", h.name),
        }
    }

    fn rows(&self, today: i64, max: usize) -> Vec<Node> {
        self.list
            .iter()
            .enumerate()
            .take(max)
            .map(|(i, h)| Node::check(LocalId(i as u32 + 1), Self::row_text(h, today), h.done_on(today)))
            .collect()
    }

    fn view_for(&self, shape: Shape, time: &TimeInfo) -> View {
        let today = today_of(time);
        let (done, total) = (done_today(&self.list, today), self.list.len());
        let empty = || {
            Node::column(6.0, 12.0, vec![
                label("Sem h\u{e1}bitos", TextStyle::Body, Role::Secondary, Align::Center),
                Node::new(Kind::Button { id: LocalId(ADD), label: "Adicionar".into() }),
            ])
            .centered()
        };
        let root = match shape {
            s if s == Shape::S1X1 => {
                if total == 0 {
                    empty()
                } else {
                    Node::column(0.0, 6.0, vec![
                        label(&format!("{done}/{total}"), TextStyle::Display, if done == total { Role::Positive } else { Role::Primary }, Align::Center),
                        label("h\u{e1}bitos hoje", TextStyle::Caption, Role::Secondary, Align::Center),
                    ])
                    .centered()
                }
            }
            _ if total == 0 => empty(),
            s if s == Shape::S2X1 => {
                let mut kids = self.rows(today, 2);
                if total > 2 {
                    kids.push(label(&format!("{done}/{total} hoje"), TextStyle::Caption, Role::Secondary, Align::Start));
                }
                Node::column(2.0, 10.0, kids).centered().size(Len::Fill(1), Len::Fill(1))
            }
            s => {
                let max = 4 + (s.rows as usize).saturating_sub(2) * 3;
                let mut kids = vec![Node::row(6.0, 0.0, vec![
                    label("H\u{e1}bitos", TextStyle::Title, Role::Primary, Align::Start),
                    label(&format!("{done}/{total}"), TextStyle::Body, if done == total { Role::Positive } else { Role::Secondary }, Align::End),
                ])
                .size(Len::Fill(1), Len::Auto)];
                kids.extend(self.rows(today, max));
                if total > max {
                    kids.push(label(&format!("+{} mais", total - max), TextStyle::Caption, Role::Secondary, Align::Start));
                }
                Node::column(4.0, 14.0, kids)
            }
        };
        View::new(root.fill())
    }

    fn ask_name(&self) -> Outcome {
        if self.list.len() >= MAX_HABITS {
            return Outcome::none();
        }
        Outcome {
            invalidate: Invalidation::NONE,
            effects: vec![Effect::OpenInput(InputSpec {
                title: "Novo h\u{e1}bito".into(),
                label: "Nome do h\u{e1}bito (ex.: Ler 15 min)".into(),
                initial: String::new(),
                secret: false,
            })],
        }
    }

    fn changed(&self) -> Outcome {
        Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged] }
    }
}

impl Widget for Habits {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => Outcome {
                invalidate: Invalidation::CONTENT,
                effects: vec![Effect::TimeTicks(Some(Granularity::Minute))],
            },
            Event::Time(t) => {
                let day = today_of(t);
                if self.today == Some(day) {
                    return Outcome::none();
                }
                self.today = Some(day);
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![] }
            }
            Event::Clicked { button } if button.0 == ADD => self.ask_name(),
            Event::MenuCommand(ADD_CMD) => self.ask_name(),
            Event::Clicked { button } => {
                let (Some(today), Some(h)) = (self.today, (button.0 as usize).checked_sub(1).and_then(|i| self.list.get_mut(i))) else {
                    return Outcome::none();
                };
                h.toggle(today);
                self.changed()
            }
            Event::MenuCommand(c) if *c >= REMOVE_BASE && ((*c - REMOVE_BASE) as usize) < self.list.len() => {
                self.list.remove((*c - REMOVE_BASE) as usize);
                self.changed()
            }
            Event::TextChanged { text, .. } => match habits::add(&mut self.list, text) {
                Ok(()) => self.changed(),
                Err(e) => Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::InputError(e.message())] },
            },
            Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        Some(self.view_for(ctx.shape, &ctx.time))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        let mut items = vec![MenuItem { id: ADD_CMD, label: "Adicionar h\u{e1}bito\u{2026}".into(), checked: false, group: None }];
        for (i, h) in self.list.iter().enumerate() {
            items.push(MenuItem { id: REMOVE_BASE + i as u32, label: h.name.clone(), checked: false, group: Some("Remover h\u{e1}bito") });
        }
        items
    }

    fn snapshot(&self) -> Snapshot {
        let data = serde_json::to_string(&Saved { habits: self.list.clone() }).unwrap_or_default();
        Snapshot { version: STATE_VERSION, data }
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

    fn at(day: u8) -> TimeInfo {
        TimeInfo { year: 2026, month: 10, day, weekday: weekday_of(2026, 10, day), hour: 9, minute: 0, second: 0, millis: 0 }
    }

    fn with(names: &[&str]) -> Box<dyn Widget> {
        let mut w = create(None).unwrap();
        w.handle(&Event::Started { now_ms: 0 });
        w.handle(&Event::Time(at(19)));
        for n in names {
            w.handle(&Event::TextChanged { field: LocalId(0), text: (*n).into() });
        }
        w
    }

    #[test]
    fn adding_validates_persists_and_restores() {
        let mut w = with(&[]);
        let bad = w.handle(&Event::TextChanged { field: LocalId(0), text: "  ".into() });
        assert!(matches!(bad.effects.as_slice(), [Effect::InputError(_)]));
        let ok = w.handle(&Event::TextChanged { field: LocalId(0), text: "Ler  15 min".into() });
        assert_eq!(ok.effects, vec![Effect::StateChanged]);
        let again = create(Some(&w.snapshot())).unwrap();
        assert_eq!(again.snapshot().data, w.snapshot().data);
        assert!(w.snapshot().data.contains("Ler 15 min"));
    }

    #[test]
    fn checking_toggles_today_and_builds_a_streak() {
        let mut w = with(&["Caminhar"]);
        // sem a hora do host ainda não há "hoje": nada marca
        let mut fresh = create(Some(&w.snapshot())).unwrap();
        assert_eq!(fresh.handle(&Event::Clicked { button: LocalId(1) }), Outcome::none());
        for d in [17u8, 18, 19] {
            w.handle(&Event::Time(at(d)));
            assert_eq!(w.handle(&Event::Clicked { button: LocalId(1) }).effects, vec![Effect::StateChanged]);
        }
        let v = w.tile_view(&ViewCtx { shape: Shape::S2X2, time: at(19) }).unwrap();
        fn texts(n: &Node, out: &mut Vec<String>) {
            if let Kind::Check { label, .. } = &n.kind {
                out.push(label.clone());
            }
            n.children().iter().for_each(|c| texts(c, out));
        }
        let mut t = Vec::new();
        texts(&v.root, &mut t);
        assert_eq!(t, vec!["Caminhar \u{b7} 3 d".to_string()]);
        // desmarcar hoje mantém a sequência de ontem
        w.handle(&Event::Clicked { button: LocalId(1) });
        let v = w.tile_view(&ViewCtx { shape: Shape::S2X2, time: at(19) }).unwrap();
        let mut t = Vec::new();
        texts(&v.root, &mut t);
        assert_eq!(t, vec!["Caminhar \u{b7} 2 d".to_string()]);
    }

    #[test]
    fn removing_from_the_menu_and_menu_lists_every_habit() {
        let mut w = with(&["A", "B", "C"]);
        assert_eq!(w.menu_items().len(), 4);
        w.handle(&Event::MenuCommand(REMOVE_BASE + 1));
        let names: Vec<String> = w.menu_items().into_iter().skip(1).map(|m| m.label).collect();
        assert_eq!(names, vec!["A", "C"]);
        assert_eq!(w.handle(&Event::MenuCommand(REMOVE_BASE + 9)), Outcome::none());
    }

    #[test]
    fn add_asks_for_a_name_until_the_list_is_full() {
        let mut w = with(&[]);
        assert!(matches!(w.handle(&Event::Clicked { button: LocalId(ADD) }).effects.as_slice(), [Effect::OpenInput(s)] if !s.secret));
        let names: Vec<String> = (0..MAX_HABITS).map(|i| format!("h{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut full = with(&refs);
        assert_eq!(full.handle(&Event::MenuCommand(ADD_CMD)), Outcome::none());
        let over = full.handle(&Event::TextChanged { field: LocalId(0), text: "extra".into() });
        assert!(matches!(over.effects.as_slice(), [Effect::InputError(_)]));
    }

    #[test]
    fn every_shape_with_few_and_many_habits_is_valid() {
        let many: Vec<String> = (0..MAX_HABITS).map(|i| format!("H\u{e1}bito {i}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        for list in [&[][..], &["Um"][..], &refs[..]] {
            let w = with(list);
            for shape in descriptor().shapes {
                let v = w.tile_view(&ViewCtx { shape, time: at(19) }).unwrap();
                assert!(v.validate().is_ok(), "{} h\u{e1}bitos {shape}", list.len());
            }
        }
    }

    #[test]
    fn bad_saved_state_is_refused_and_future_version_too() {
        assert!(create(Some(&Snapshot { version: 1, data: "{".into() })).is_err());
        assert!(create(Some(&Snapshot { version: 2, data: String::new() })).is_err());
        assert!(create(Some(&Snapshot { version: 1, data: String::new() })).is_ok());
    }
}
