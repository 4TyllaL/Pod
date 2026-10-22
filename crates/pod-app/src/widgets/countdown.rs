//! Countdown: dias até um evento do calendário (1×1, 2×1, 2×2). Usa os eventos compartilhados;
//! sem escolha explícita acompanha o próximo evento. A data vira à meia-noite (assina a hora por minuto).

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::shared::{Date, SharedData};
use pod_model::time::Granularity;
use pod_model::view::{Align, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, MenuItem, Outcome, PanelKind, SizeDip, Snapshot,
    StopReason, ViewCtx, Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

use super::tkit::{item, label};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.countdown");
pub const STATE_VERSION: u32 = 1;
const CMD_EDIT: u32 = 1;
const CMD_NEXT: u32 = 2;
/// Eventos escolhíveis no menu: `CMD_PICK_BASE + id`.
const CMD_PICK_BASE: u32 = 1000;
const MAX_MENU_EVENTS: usize = 12;

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
    /// Evento escolhido; `None` = sempre o próximo.
    event_id: Option<u64>,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let saved = match restore {
        None => Saved::default(),
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) => serde_json::from_str::<Saved>(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    Ok(Box::new(Countdown { chosen: saved.event_id, shared: SharedData::default(), stopped: false }))
}

struct Countdown {
    chosen: Option<u64>,
    shared: SharedData,
    stopped: bool,
}

/// O que mostrar: número grande, legenda e o evento.
struct Reading {
    big: String,
    unit: String,
    title: String,
    date: String,
    role: Role,
}

fn reading(shared: &SharedData, chosen: Option<u64>, today: Date) -> Option<Reading> {
    let ev = chosen.and_then(|id| shared.event(id)).or_else(|| shared.next_event(today))?;
    let n = today.days_until(ev.date);
    let (big, unit, role) = match n {
        0 => ("Hoje".to_string(), String::new(), Role::Positive),
        1 => ("Amanh\u{e3}".to_string(), String::new(), Role::Accent),
        n if n > 1 => (n.to_string(), "dias".to_string(), Role::Primary),
        -1 => ("Ontem".to_string(), String::new(), Role::Secondary),
        n => ((-n).to_string(), "dias atr\u{e1}s".to_string(), Role::Secondary),
    };
    Some(Reading { big, unit, title: ev.title.clone(), date: ev.date.format(), role })
}

impl Widget for Countdown {
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
            Event::MenuCommand(CMD_EDIT) => Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::OpenPanel(PanelKind::Events)] },
            Event::MenuCommand(CMD_NEXT) => {
                self.chosen = None;
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged] }
            }
            Event::MenuCommand(c) if *c > CMD_PICK_BASE && self.shared.event((*c - CMD_PICK_BASE) as u64).is_some() => {
                self.chosen = Some((*c - CMD_PICK_BASE) as u64);
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged] }
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
        let today = Date::from_time(&ctx.time);
        let root = match (reading(&self.shared, self.chosen, today), ctx.shape) {
            (None, s) if s == Shape::S1X1 => Node::column(2.0, 6.0, vec![
                label("Sem".into(), TextStyle::Body, Role::Secondary, Align::Center),
                label("eventos".into(), TextStyle::Caption, Role::Tertiary, Align::Center),
            ])
            .centered(),
            (None, _) => Node::column(4.0, 12.0, vec![
                label("Sem eventos".into(), TextStyle::Title, Role::Primary, Align::Center),
                label("Menu \u{2192} Editar eventos\u{2026}".into(), TextStyle::Caption, Role::Secondary, Align::Center),
            ])
            .centered(),
            (Some(r), s) if s == Shape::S1X1 => {
                let big = if r.unit.is_empty() { TextStyle::Title } else { TextStyle::Headline };
                Node::column(1.0, 6.0, vec![
                    label(r.big, big, r.role, Align::Center),
                    label(if r.unit.is_empty() { r.date.clone() } else { r.unit.clone() }, TextStyle::Caption, Role::Secondary, Align::Center),
                    label(r.title, TextStyle::Caption, Role::Primary, Align::Center),
                ])
                .centered()
            }
            (Some(r), s) if s == Shape::S2X2 => Node::column(6.0, 14.0, vec![
                label(r.title, TextStyle::Title, Role::Primary, Align::Center),
                label(r.big, TextStyle::Display, r.role, Align::Center),
                label(if r.unit.is_empty() { String::new() } else { r.unit }, TextStyle::Body, Role::Secondary, Align::Center),
                label(r.date, TextStyle::Caption, Role::Secondary, Align::Center),
            ])
            .centered(),
            (Some(r), _) => Node::row(12.0, 12.0, vec![
                Node::column(0.0, 0.0, vec![
                    label(r.big, TextStyle::Headline, r.role, Align::Center),
                    label(r.unit, TextStyle::Caption, Role::Secondary, Align::Center),
                ])
                .size(Len::Fixed(78.0), Len::Auto)
                .centered(),
                Node::column(2.0, 0.0, vec![
                    label(r.title, TextStyle::Body, Role::Primary, Align::Start),
                    label(r.date, TextStyle::Caption, Role::Secondary, Align::Start),
                ])
                .size(Len::Fill(1), Len::Auto),
            ])
            .centered(),
        };
        Some(View::new(root.fill()))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        let mut items = vec![item(CMD_EDIT, "Editar eventos\u{2026}", false, None)];
        items.push(item(CMD_NEXT, "Pr\u{f3}ximo evento", self.chosen.is_none(), Some("Evento")));
        for e in self.shared.events().iter().take(MAX_MENU_EVENTS) {
            let text = format!("{}  {}", e.date.format(), e.title);
            items.push(item(CMD_PICK_BASE + e.id as u32, &text, self.chosen == Some(e.id), Some("Evento")));
        }
        items
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&Saved { event_id: self.chosen }).unwrap_or_default() }
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

    fn ctx(shape: Shape) -> ViewCtx {
        ViewCtx {
            shape,
            time: TimeInfo { year: 2026, month: 10, day: 18, weekday: weekday_of(2026, 10, 18), hour: 10, minute: 0, second: 0, millis: 0 },
        }
    }

    fn today() -> Date {
        Date::new(2026, 10, 18).unwrap()
    }

    fn data() -> SharedData {
        let mut s = SharedData::default();
        s.add_event(Date::new(2026, 10, 18).unwrap(), "Hoje mesmo").unwrap();
        s.add_event(Date::new(2026, 10, 19).unwrap(), "Amanh\u{e3} cedo").unwrap();
        s.add_event(Date::new(2026, 12, 25).unwrap(), "Natal").unwrap();
        s.add_event(Date::new(2026, 10, 1).unwrap(), "J\u{e1} passou").unwrap();
        s
    }

    #[test]
    fn reading_words_for_each_distance() {
        let s = data();
        let ids: Vec<(u64, String)> = s.events().iter().map(|e| (e.id, e.title.clone())).collect();
        let id_of = |t: &str| ids.iter().find(|(_, n)| n == t).unwrap().0;
        let r = |t: &str| reading(&s, Some(id_of(t)), today()).unwrap();
        assert_eq!((r("Hoje mesmo").big.as_str(), r("Hoje mesmo").role), ("Hoje", Role::Positive));
        assert_eq!(r("Amanh\u{e3} cedo").big, "Amanh\u{e3}");
        assert_eq!((r("Natal").big.as_str(), r("Natal").unit.as_str()), ("68", "dias"));
        assert_eq!((r("J\u{e1} passou").big.as_str(), r("J\u{e1} passou").unit.as_str()), ("17", "dias atr\u{e1}s"));
        // Sem escolha: o próximo a partir de hoje (inclui hoje).
        assert_eq!(reading(&s, None, today()).unwrap().title, "Hoje mesmo");
        assert!(reading(&SharedData::default(), None, today()).is_none());
    }

    #[test]
    fn a_removed_event_falls_back_to_the_next_one() {
        let mut w = create(None).unwrap();
        w.set_shared(&data());
        let natal = data().events().iter().find(|e| e.title == "Natal").unwrap().id;
        let out = w.handle(&Event::MenuCommand(CMD_PICK_BASE + natal as u32));
        assert!(out.effects.contains(&Effect::StateChanged));
        assert!(w.snapshot().data.contains(&natal.to_string()));
        w.set_shared(&SharedData::default());
        assert!(w.tile_view(&ctx(Shape::S2X1)).is_some(), "sem evento não quebra");
    }

    #[test]
    fn menu_edit_opens_the_panel_and_unknown_picks_are_ignored() {
        let mut w = create(None).unwrap();
        w.set_shared(&data());
        assert_eq!(w.handle(&Event::MenuCommand(CMD_EDIT)).effects, vec![Effect::OpenPanel(PanelKind::Events)]);
        assert_eq!(w.handle(&Event::MenuCommand(CMD_PICK_BASE + 999)), Outcome::none());
        let items = w.menu_items();
        assert!(items.iter().any(|i| i.group == Some("Evento") && i.checked && i.label.starts_with("Pr")), "{items:?}");
        assert_eq!(items.iter().filter(|i| i.group == Some("Evento")).count(), 1 + 4);
    }

    #[test]
    fn subscribes_to_time_and_unsubscribes_once() {
        let mut w = create(None).unwrap();
        assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::TimeTicks(Some(Granularity::Minute))]);
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::TimeTicks(None)]);
        assert!(w.stop(StopReason::Removed).is_empty());
    }

    #[test]
    fn every_shape_fits_with_and_without_events() {
        for with in [false, true] {
            let mut w = create(None).unwrap();
            if with {
                w.set_shared(&data());
            }
            for shape in descriptor().shapes {
                let v = w.tile_view(&ctx(shape)).unwrap();
                assert_eq!(v.validate(), Ok(()));
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{shape} com={with}: {:?}", l.rect);
                }
            }
        }
    }

    #[test]
    fn bad_or_future_state_is_refused() {
        assert!(matches!(create(Some(&Snapshot { version: 1, data: "x".into() })), Err(WidgetError::InvalidState(_))));
        assert!(matches!(create(Some(&Snapshot { version: 2, data: "{}".into() })), Err(WidgetError::UnsupportedVersion { .. })));
    }
}
