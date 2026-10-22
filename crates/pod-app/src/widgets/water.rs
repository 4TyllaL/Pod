//! Drink water: meta diária de água com um botão por copo (1×1, 2×1, 2×2). O contador zera a cada
//! dia (a data vem da assinatura de hora) e a meta e o tamanho do copo vêm de listas no menu.

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::shared::Date;
use pod_model::time::Granularity;
use pod_model::view::{Align, Fill, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, MenuItem, Outcome, SizeDip, Snapshot, StopReason, ViewCtx,
    Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

use super::tkit::{button, item, label};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.water");
pub const STATE_VERSION: u32 = 1;
pub const ADD: u32 = 1;
pub const UNDO: u32 = 2;
const CMD_ZERO: u32 = 3;
const GOAL_BASE: u32 = 10_000;
const GLASS_BASE: u32 = 20_000;
pub const GOALS_ML: [u32; 5] = [1500, 2000, 2500, 3000, 3500];
pub const GLASSES_ML: [u32; 5] = [150, 200, 250, 300, 500];
/// Teto do contador do dia (evita números absurdos por cliques acidentais).
const MAX_DRUNK_ML: u32 = 10_000;

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

#[derive(Serialize, Deserialize, Clone, PartialEq)]
struct Saved {
    goal_ml: u32,
    glass_ml: u32,
    day: Option<Date>,
    drunk_ml: u32,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let saved = match restore {
        None => Saved { goal_ml: 2000, glass_ml: 250, day: None, drunk_ml: 0 },
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) => serde_json::from_str::<Saved>(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    if !GOALS_ML.contains(&saved.goal_ml)
        || !GLASSES_ML.contains(&saved.glass_ml)
        || saved.drunk_ml > MAX_DRUNK_ML
        || saved.day.is_some_and(|d| Date::new(d.year, d.month, d.day).is_none())
    {
        return Err(WidgetError::InvalidState("valores fora da lista".into()));
    }
    Ok(Box::new(Water { s: saved, stopped: false }))
}

struct Water {
    s: Saved,
    stopped: bool,
}

/// "750 ml", "1,25 L", "2 L".
pub fn fmt_ml(ml: u32) -> String {
    if ml < 1000 {
        return format!("{ml} ml");
    }
    let l = ml as f64 / 1000.0;
    let mut t = format!("{l:.2}");
    while t.ends_with('0') {
        t.pop();
    }
    if t.ends_with('.') {
        t.pop();
    }
    format!("{} L", t.replace('.', ","))
}

impl Water {
    fn frac(&self) -> f32 {
        (self.s.drunk_ml as f32 / self.s.goal_ml as f32).clamp(0.0, 1.0)
    }

    fn pct(&self) -> u32 {
        (self.s.drunk_ml as u64 * 100 / self.s.goal_ml as u64) as u32
    }

    fn changed(&self) -> Outcome {
        Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged] }
    }

    fn ring_role(&self) -> Role {
        if self.s.drunk_ml >= self.s.goal_ml {
            Role::Positive
        } else {
            Role::Accent
        }
    }

    /// Marcas de copos: proporcional à meta, de 4 a 10, preenchidas pelo que já foi bebido.
    fn dots(&self) -> Node {
        let n = self.s.goal_ml.div_ceil(self.s.glass_ml).clamp(4, 10) as usize;
        let filled = ((self.frac() * n as f32) + 0.0001).floor() as usize;
        let cells = (0..n)
            .map(|i| {
                let fill = if i < filled { Fill::Identity } else { Fill::Plain };
                Node::plate(fill, 6.0, 0.0, Node::spacer().size(Len::Fixed(12.0), Len::Fixed(12.0)))
                    .size(Len::Fixed(12.0), Len::Fixed(12.0))
            })
            .collect();
        Node::row(4.0, 0.0, cells).centered()
    }
}

impl Widget for Water {
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
                let today = Date::from_time(t);
                if self.s.day == Some(today) {
                    return Outcome::none();
                }
                // Dia novo: o contador volta a zero (e o dia fica gravado).
                self.s.day = Some(today);
                self.s.drunk_ml = 0;
                self.changed()
            }
            Event::Clicked { button } if button.0 == ADD => {
                self.s.drunk_ml = (self.s.drunk_ml + self.s.glass_ml).min(MAX_DRUNK_ML);
                self.changed()
            }
            Event::Clicked { button } if button.0 == UNDO => {
                self.s.drunk_ml = self.s.drunk_ml.saturating_sub(self.s.glass_ml);
                self.changed()
            }
            Event::MenuCommand(ADD) => {
                self.s.drunk_ml = (self.s.drunk_ml + self.s.glass_ml).min(MAX_DRUNK_ML);
                self.changed()
            }
            Event::MenuCommand(UNDO) => {
                self.s.drunk_ml = self.s.drunk_ml.saturating_sub(self.s.glass_ml);
                self.changed()
            }
            Event::MenuCommand(CMD_ZERO) => {
                self.s.drunk_ml = 0;
                self.changed()
            }
            Event::MenuCommand(c) if (GOAL_BASE..GLASS_BASE).contains(c) && GOALS_ML.contains(&(c - GOAL_BASE)) => {
                self.s.goal_ml = c - GOAL_BASE;
                self.changed()
            }
            Event::MenuCommand(c) if *c >= GLASS_BASE && GLASSES_ML.contains(&(c - GLASS_BASE)) => {
                self.s.glass_ml = c - GLASS_BASE;
                self.changed()
            }
            Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        let add_label = format!("+{}", self.s.glass_ml);
        let pct = format!("{}%", self.pct());
        let progress = format!("{} de {}", fmt_ml(self.s.drunk_ml), fmt_ml(self.s.goal_ml));
        let ring = |size: f32, thick: f32, style: TextStyle| {
            Node::layers(vec![Node::ring(self.frac(), size, thick, self.ring_role()), label(pct.clone(), style, Role::Primary, Align::Center)])
        };
        let root = match ctx.shape {
            s if s == Shape::S1X1 => {
                Node::column(2.0, 6.0, vec![ring(44.0, 5.0, TextStyle::Caption), button(ADD, &add_label)]).centered()
            }
            s if s == Shape::S2X2 => Node::column(4.0, 10.0, vec![
                ring(90.0, 9.0, TextStyle::Headline),
                label(progress, TextStyle::Caption, Role::Secondary, Align::Center),
                self.dots(),
                Node::row(8.0, 0.0, vec![button(ADD, &add_label), button(UNDO, "\u{2212}")]).centered(),
            ])
            .centered(),
            _ => Node::row(14.0, 8.0, vec![
                ring(64.0, 7.0, TextStyle::Body),
                Node::column(3.0, 0.0, vec![
                    super::tkit::chip_header('\u{1f4a7}', "\u{c1}gua", false),
                    label(progress, TextStyle::Caption, Role::Secondary, Align::Start),
                    Node::row(6.0, 0.0, vec![button(ADD, &add_label), button(UNDO, "\u{2212}")]),
                ])
                .size(Len::Fill(1), Len::Auto),
            ])
            .centered(),
        };
        Some(View::new(root.fill()))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        let mut items = vec![
            item(ADD, &format!("Beber um copo ({})", fmt_ml(self.s.glass_ml)), false, None),
            item(UNDO, "Desfazer \u{fa}ltimo copo", false, None),
            item(CMD_ZERO, "Zerar hoje", false, None),
        ];
        for g in GOALS_ML {
            items.push(item(GOAL_BASE + g, &fmt_ml(g), g == self.s.goal_ml, Some("Meta di\u{e1}ria")));
        }
        for g in GLASSES_ML {
            items.push(item(GLASS_BASE + g, &fmt_ml(g), g == self.s.glass_ml, Some("Tamanho do copo")));
        }
        items
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&self.s).unwrap_or_default() }
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

    fn time(day: u8) -> TimeInfo {
        TimeInfo { year: 2026, month: 10, day, weekday: weekday_of(2026, 10, day), hour: 10, minute: 0, second: 0, millis: 0 }
    }

    fn ctx(shape: Shape) -> ViewCtx {
        ViewCtx { shape, time: time(18) }
    }

    fn click(w: &mut Box<dyn Widget>, id: u32) -> Outcome {
        w.handle(&Event::Clicked { button: LocalId(id) })
    }

    #[test]
    fn liters_and_milliliters_in_portuguese() {
        assert_eq!(fmt_ml(0), "0 ml");
        assert_eq!(fmt_ml(750), "750 ml");
        assert_eq!(fmt_ml(1000), "1 L");
        assert_eq!(fmt_ml(1250), "1,25 L");
        assert_eq!(fmt_ml(2500), "2,5 L");
    }

    #[test]
    fn glasses_add_and_undo_and_are_capped() {
        let mut w = create(None).unwrap();
        w.handle(&Event::Time(time(18)));
        click(&mut w, ADD);
        click(&mut w, ADD);
        assert!(w.snapshot().data.contains("\"drunk_ml\":500"), "{}", w.snapshot().data);
        click(&mut w, UNDO);
        assert!(w.snapshot().data.contains("\"drunk_ml\":250"));
        click(&mut w, UNDO);
        let out = click(&mut w, UNDO);
        assert!(w.snapshot().data.contains("\"drunk_ml\":0"), "nunca fica negativo");
        assert!(out.effects.contains(&Effect::StateChanged));
        for _ in 0..100 {
            click(&mut w, ADD);
        }
        assert!(w.snapshot().data.contains("\"drunk_ml\":10000"), "teto do dia");
    }

    #[test]
    fn a_new_day_resets_the_counter_once() {
        let mut w = create(None).unwrap();
        w.handle(&Event::Time(time(18)));
        click(&mut w, ADD);
        assert_eq!(w.handle(&Event::Time(time(18))), Outcome::none(), "mesmo dia: nada muda");
        let out = w.handle(&Event::Time(time(19)));
        assert!(out.effects.contains(&Effect::StateChanged));
        let d = w.snapshot().data;
        assert!(d.contains("\"drunk_ml\":0") && d.contains("\"day\":{\"year\":2026,\"month\":10,\"day\":19}"), "{d}");
        assert_eq!(w.handle(&Event::Time(time(19))), Outcome::none());
    }

    #[test]
    fn goal_and_glass_come_from_the_lists() {
        let mut w = create(None).unwrap();
        w.handle(&Event::MenuCommand(GOAL_BASE + 3000));
        w.handle(&Event::MenuCommand(GLASS_BASE + 500));
        let d = w.snapshot().data;
        assert!(d.contains("\"goal_ml\":3000") && d.contains("\"glass_ml\":500"), "{d}");
        assert_eq!(w.handle(&Event::MenuCommand(GOAL_BASE + 1234)), Outcome::none());
        assert_eq!(w.handle(&Event::MenuCommand(GLASS_BASE + 7)), Outcome::none());
        let checked: Vec<_> = w.menu_items().into_iter().filter(|i| i.checked).map(|i| i.label).collect();
        assert_eq!(checked, vec!["3 L", "500 ml"]);
    }

    #[test]
    fn the_ring_turns_positive_when_the_goal_is_reached() {
        let mut w = create(None).unwrap();
        w.handle(&Event::MenuCommand(GOAL_BASE + 1500));
        w.handle(&Event::MenuCommand(GLASS_BASE + 500));
        for _ in 0..3 {
            click(&mut w, ADD);
        }
        let v = w.tile_view(&ctx(Shape::S2X1)).unwrap();
        fn find_ring(n: &Node) -> Option<Role> {
            if let pod_model::view::Kind::Ring { role, .. } = &n.kind {
                return Some(*role);
            }
            n.children().iter().find_map(find_ring)
        }
        assert_eq!(find_ring(&v.root), Some(Role::Positive));
    }

    #[test]
    fn every_shape_fits_at_every_level_and_the_first_button_is_add() {
        for glasses in [0usize, 1, 5, 8, 40] {
            let mut w = create(None).unwrap();
            for _ in 0..glasses {
                click(&mut w, ADD);
            }
            for shape in descriptor().shapes {
                let v = w.tile_view(&ctx(shape)).unwrap();
                assert_eq!(v.validate(), Ok(()));
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{glasses} copos {shape}: {:?}", l.rect);
                }
                assert_eq!(button_rects(&v, &laid)[0].0, LocalId(ADD));
            }
        }
    }

    #[test]
    fn bad_state_is_refused() {
        for bad in [
            "x",
            "{\"goal_ml\":1234,\"glass_ml\":250,\"day\":null,\"drunk_ml\":0}",
            "{\"goal_ml\":2000,\"glass_ml\":251,\"day\":null,\"drunk_ml\":0}",
            "{\"goal_ml\":2000,\"glass_ml\":250,\"day\":null,\"drunk_ml\":99999}",
            "{\"goal_ml\":2000,\"glass_ml\":250,\"day\":{\"year\":2026,\"month\":2,\"day\":31},\"drunk_ml\":0}",
        ] {
            assert!(matches!(create(Some(&Snapshot { version: 1, data: bad.into() })), Err(WidgetError::InvalidState(_))), "{bad}");
        }
        assert!(matches!(create(Some(&Snapshot { version: 2, data: "{}".into() })), Err(WidgetError::UnsupportedVersion { .. })));
    }

    #[test]
    fn stop_unsubscribes_once() {
        let mut w = create(None).unwrap();
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::TimeTicks(None)]);
        assert!(w.stop(StopReason::Removed).is_empty());
    }
}
