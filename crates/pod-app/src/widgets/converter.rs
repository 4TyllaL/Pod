//! Converter: conversor de unidades (comprimento, massa, temperatura, volume, velocidade). O valor
//! é digitado numa janelinha de entrada (o widget valida e pode recusar); categoria e unidades vêm do menu.

use pod_model::convert::{convert, format_number, parse_number, Category};
use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::view::{Align, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, InputSpec, Invalidation, MenuItem, Outcome, SizeDip, Snapshot,
    StopReason, ViewCtx, Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

use super::tkit::{button, item, label};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.converter");
pub const STATE_VERSION: u32 = 1;
pub const VALUE: u32 = 1;
pub const SWAP: u32 = 2;
const CAT_BASE: u32 = 100;
const FROM_BASE: u32 = 200;
const TO_BASE: u32 = 300;

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
    category: String,
    from: usize,
    to: usize,
    value: f64,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let saved = match restore {
        None => Saved { category: "length".into(), from: 3, to: 7, value: 1.0 },
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) => serde_json::from_str::<Saved>(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    let cat = Category::from_key(&saved.category).ok_or_else(|| WidgetError::InvalidState("categoria desconhecida".into()))?;
    let n = cat.units().len();
    if saved.from >= n || saved.to >= n || !saved.value.is_finite() || saved.value.abs() > 1e15 {
        return Err(WidgetError::InvalidState("unidade ou valor inv\u{e1}lido".into()));
    }
    Ok(Box::new(Converter { cat, from: saved.from, to: saved.to, value: saved.value, stopped: false }))
}

struct Converter {
    cat: Category,
    from: usize,
    to: usize,
    value: f64,
    stopped: bool,
}

impl Converter {
    fn symbols(&self) -> (&'static str, &'static str) {
        let u = self.cat.units();
        (u[self.from].symbol, u[self.to].symbol)
    }

    fn result(&self) -> f64 {
        convert(self.cat, self.from, self.to, self.value).unwrap_or(f64::NAN)
    }

    fn changed(&self) -> Outcome {
        Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged] }
    }

    fn ask_value(&self) -> Outcome {
        Outcome {
            invalidate: Invalidation::NONE,
            effects: vec![Effect::OpenInput(InputSpec {
                title: "Converter".into(),
                label: format!("Valor em {} ({})", self.symbols().0, self.cat.units()[self.from].name),
                initial: format_number(self.value),
                secret: false,
            })],
        }
    }

    fn swap(&mut self) -> Outcome {
        std::mem::swap(&mut self.from, &mut self.to);
        self.changed()
    }
}

impl Widget for Converter {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Clicked { button } if button.0 == VALUE => self.ask_value(),
            Event::Clicked { button } if button.0 == SWAP => self.swap(),
            Event::MenuCommand(VALUE) => self.ask_value(),
            Event::MenuCommand(SWAP) => self.swap(),
            Event::TextChanged { text, .. } => match parse_number(text) {
                Some(v) => {
                    self.value = v;
                    self.changed()
                }
                None => Outcome {
                    invalidate: Invalidation::NONE,
                    effects: vec![Effect::InputError("N\u{fa}mero inv\u{e1}lido. Exemplos: 12 ou 12,5".into())],
                },
            },
            Event::MenuCommand(c) if (CAT_BASE..FROM_BASE).contains(c) => match pod_model::convert::ALL.get((c - CAT_BASE) as usize) {
                Some(cat) if *cat != self.cat => {
                    self.cat = *cat;
                    self.from = 0;
                    self.to = 1;
                    self.changed()
                }
                _ => Outcome::none(),
            },
            Event::MenuCommand(c) if (FROM_BASE..TO_BASE).contains(c) && ((c - FROM_BASE) as usize) < self.cat.units().len() => {
                self.from = (c - FROM_BASE) as usize;
                self.changed()
            }
            Event::MenuCommand(c) if *c >= TO_BASE && ((c - TO_BASE) as usize) < self.cat.units().len() => {
                self.to = (c - TO_BASE) as usize;
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
        let (from, to) = self.symbols();
        let input = format!("{} {}", format_number(self.value), from);
        let result = format_number(self.result());
        let big = if result.chars().count() > 7 { TextStyle::Headline } else { TextStyle::Display };
        let root = match ctx.shape {
            s if s == Shape::S1X1 => Node::column(1.0, 6.0, vec![
                label(input, TextStyle::Caption, Role::Secondary, Align::Center),
                label(result, TextStyle::Body, Role::Primary, Align::Center),
                label(to.to_string(), TextStyle::Caption, Role::Accent, Align::Center),
            ])
            .centered(),
            s if s == Shape::S2X2 => Node::column(6.0, 12.0, vec![
                label(self.cat.name().into(), TextStyle::Title, Role::Primary, Align::Center),
                label(format!("{input} ="), TextStyle::Body, Role::Secondary, Align::Center),
                label(result, big, Role::Primary, Align::Center),
                label(self.cat.units()[self.to].name.to_string(), TextStyle::Body, Role::Accent, Align::Center),
                Node::row(8.0, 0.0, vec![button(VALUE, "Valor\u{2026}"), button(SWAP, "Inverter")]).centered(),
            ])
            .centered(),
            _ => Node::row(8.0, 10.0, vec![
                Node::column(2.0, 0.0, vec![
                    label(format!("{input} ="), TextStyle::Caption, Role::Secondary, Align::Start),
                    label(format!("{result} {to}"), TextStyle::Title, Role::Primary, Align::Start),
                ])
                .size(Len::Fill(1), Len::Auto),
                button(VALUE, "Valor\u{2026}"),
            ])
            .centered(),
        };
        Some(View::new(root.fill()))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        let mut items = vec![
            item(VALUE, "Alterar valor\u{2026}", false, None),
            item(SWAP, "Inverter unidades", false, None),
        ];
        for (i, c) in pod_model::convert::ALL.iter().enumerate() {
            items.push(item(CAT_BASE + i as u32, c.name(), *c == self.cat, Some("Categoria")));
        }
        for (i, u) in self.cat.units().iter().enumerate() {
            items.push(item(FROM_BASE + i as u32, &format!("{} ({})", u.symbol, u.name), i == self.from, Some("De")));
        }
        for (i, u) in self.cat.units().iter().enumerate() {
            items.push(item(TO_BASE + i as u32, &format!("{} ({})", u.symbol, u.name), i == self.to, Some("Para")));
        }
        items
    }

    fn snapshot(&self) -> Snapshot {
        let saved = Saved { category: self.cat.key().into(), from: self.from, to: self.to, value: self.value };
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&saved).unwrap_or_default() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        self.stopped = true;
        Vec::new()
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

    fn names(w: &Box<dyn Widget>, shape: Shape) -> Vec<String> {
        let v = w.tile_view(&ctx(shape)).unwrap();
        let s = shape.size_dip();
        pod_model::a11y::items(&v, &layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake)).into_iter().map(|i| i.name).collect()
    }

    #[test]
    fn default_shows_one_kilometer_in_miles() {
        let w = create(None).unwrap();
        let n = names(&w, Shape::S2X1);
        assert!(n.contains(&"1 km =".to_string()) && n.contains(&"0,621371 mi".to_string()), "{n:?}");
    }

    #[test]
    fn typed_values_are_parsed_and_invalid_ones_are_refused_with_a_message() {
        let mut w = create(None).unwrap();
        let ok = w.handle(&Event::TextChanged { field: LocalId(0), text: "12,5".into() });
        assert!(ok.effects.contains(&Effect::StateChanged));
        assert!(w.snapshot().data.contains("12.5"));
        let bad = w.handle(&Event::TextChanged { field: LocalId(0), text: "doze".into() });
        assert!(matches!(bad.effects.as_slice(), [Effect::InputError(m)] if m.contains("inv\u{e1}lido")));
        assert!(w.snapshot().data.contains("12.5"), "valor recusado não altera nada");
    }

    #[test]
    fn asking_for_a_value_offers_the_current_one() {
        let mut w = create(None).unwrap();
        w.handle(&Event::TextChanged { field: LocalId(0), text: "2500".into() });
        let out = w.handle(&Event::Clicked { button: LocalId(VALUE) });
        match out.effects.as_slice() {
            [Effect::OpenInput(spec)] => {
                assert_eq!(spec.initial, "2.500");
                assert!(spec.label.contains("km") && spec.label.contains("quil\u{f4}metro"), "{}", spec.label);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn category_units_and_swap_work_through_the_menu() {
        let mut w = create(None).unwrap();
        w.handle(&Event::MenuCommand(CAT_BASE + 2)); // Temperatura
        let n = names(&w, Shape::S2X2);
        assert!(n.contains(&"Temperatura".to_string()), "{n:?}");
        w.handle(&Event::TextChanged { field: LocalId(0), text: "100".into() });
        assert!(names(&w, Shape::S2X1).contains(&"212 \u{b0}F".to_string()), "{:?}", names(&w, Shape::S2X1));
        w.handle(&Event::Clicked { button: LocalId(SWAP) });
        assert!(names(&w, Shape::S2X1).contains(&"100 \u{b0}F =".to_string()));
        w.handle(&Event::MenuCommand(TO_BASE + 2)); // Kelvin
        assert!(names(&w, Shape::S2X1).iter().any(|s| s.ends_with(" K")), "{:?}", names(&w, Shape::S2X1));
        assert_eq!(w.handle(&Event::MenuCommand(FROM_BASE + 9)), Outcome::none(), "unidade fora da categoria");
        assert_eq!(w.handle(&Event::MenuCommand(CAT_BASE + 2)), Outcome::none(), "mesma categoria");
        let groups: std::collections::BTreeSet<_> = w.menu_items().iter().filter_map(|i| i.group).collect();
        assert_eq!(groups.len(), 3);
    }

    #[test]
    fn state_roundtrips_and_bad_state_is_refused() {
        let mut w = create(None).unwrap();
        w.handle(&Event::TextChanged { field: LocalId(0), text: "-40".into() });
        let again = create(Some(&w.snapshot())).unwrap();
        assert_eq!(again.snapshot(), w.snapshot());
        for bad in [
            "x",
            "{\"category\":\"moeda\",\"from\":0,\"to\":1,\"value\":1}",
            "{\"category\":\"mass\",\"from\":0,\"to\":99,\"value\":1}",
            "{\"category\":\"mass\",\"from\":0,\"to\":1,\"value\":1e300}",
        ] {
            assert!(matches!(create(Some(&Snapshot { version: 1, data: bad.into() })), Err(WidgetError::InvalidState(_))), "{bad}");
        }
        assert!(matches!(create(Some(&Snapshot { version: 2, data: "{}".into() })), Err(WidgetError::UnsupportedVersion { .. })));
    }

    #[test]
    fn every_category_and_shape_fits_the_tile() {
        for ci in 0..pod_model::convert::ALL.len() {
            let mut w = create(None).unwrap();
            w.handle(&Event::MenuCommand(CAT_BASE + ci as u32));
            for value in ["1", "123456789,123", "0,000001", "-99999"] {
                w.handle(&Event::TextChanged { field: LocalId(0), text: value.into() });
                for shape in descriptor().shapes {
                    let v = w.tile_view(&ctx(shape)).unwrap();
                    assert_eq!(v.validate(), Ok(()));
                    let s = shape.size_dip();
                    let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                    for l in &laid {
                        assert!(laid[0].rect.contains(&l.rect), "cat {ci} {value} {shape}: {:?}", l.rect);
                    }
                    if shape != Shape::S1X1 {
                        assert_eq!(button_rects(&v, &laid)[0].0, LocalId(VALUE));
                    }
                }
            }
        }
    }
}
