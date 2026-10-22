//! Parcels: lista de encomendas a acompanhar (1×1 contador, 2×1 duas, 2×2 cinco). Só guarda o
//! código de rastreio que o usuário digita e se já chegou; não consulta nenhum serviço.

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::shared::{Parcel, SharedData, SharedOp};
use pod_model::view::{Align, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, LocalId, MenuItem, Outcome, PanelKind, SizeDip, Snapshot,
    StopReason, ViewCtx, Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

use super::tkit::{item, label};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.parcels");
pub const STATE_VERSION: u32 = 1;
const CMD_EDIT: u32 = 1;
const CMD_HIDE_DELIVERED: u32 = 2;

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X2.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S4X1, Shape::S4X2],
        default_shape: Some(Shape::S2X2),
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    hide_delivered: bool,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let saved = match restore {
        None => Saved { hide_delivered: false },
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) => serde_json::from_str::<Saved>(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    Ok(Box::new(Parcels { hide_delivered: saved.hide_delivered, shared: SharedData::default(), stopped: false }))
}

struct Parcels {
    hide_delivered: bool,
    shared: SharedData,
    stopped: bool,
}

impl Parcels {
    fn visible(&self) -> Vec<&Parcel> {
        self.shared.parcels().iter().filter(|p| !(self.hide_delivered && p.delivered)).collect()
    }

    fn rows(&self, max: usize) -> Vec<Node> {
        self.visible().into_iter().take(max).map(|p| Node::check(LocalId(p.id as u32), p.label.clone(), p.delivered)).collect()
    }

    fn overflow(&self, max: usize) -> usize {
        self.visible().len().saturating_sub(max)
    }
}

impl Widget for Parcels {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Clicked { button } => {
                let id = button.0 as u64;
                if self.shared.parcels().iter().any(|p| p.id == id) {
                    Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::SharedOp(SharedOp::ToggleParcel(id))] }
                } else {
                    Outcome::none()
                }
            }
            Event::MenuCommand(CMD_EDIT) => {
                Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::OpenPanel(PanelKind::Parcels)] }
            }
            Event::MenuCommand(CMD_HIDE_DELIVERED) => {
                self.hide_delivered = !self.hide_delivered;
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged] }
            }
            Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
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
        let pending = self.shared.pending_parcels();
        let none_text = if self.shared.parcels().is_empty() { "Sem encomendas" } else { "Tudo entregue" };
        let more = |n: usize| label(format!("+{n} mais"), TextStyle::Caption, Role::Secondary, Align::Start);
        let root = match ctx.shape {
            s if s == Shape::S1X1 => Node::column(0.0, 6.0, vec![
                label(pending.to_string(), TextStyle::Display, if pending == 0 { Role::Positive } else { Role::Primary }, Align::Center),
                label(if pending == 1 { "a caminho" } else { "a caminho" }.to_string(), TextStyle::Caption, Role::Secondary, Align::Center),
            ])
            .centered(),
            s if s == Shape::S2X2 => {
                let mut kids = vec![label("Encomendas".into(), TextStyle::Title, Role::Primary, Align::Start)];
                let rows = self.rows(5);
                if rows.is_empty() {
                    kids.push(label(none_text.into(), TextStyle::Body, Role::Secondary, Align::Start));
                }
                kids.extend(rows);
                if self.overflow(5) > 0 {
                    kids.push(more(self.overflow(5)));
                }
                Node::column(4.0, 14.0, kids)
            }
            _ => {
                let rows = self.rows(2);
                let mut kids = Vec::new();
                if rows.is_empty() {
                    kids.push(label(none_text.into(), TextStyle::Title, Role::Secondary, Align::Center));
                }
                kids.extend(rows);
                if self.overflow(2) > 0 {
                    kids.push(more(self.overflow(2)));
                }
                Node::column(2.0, 10.0, kids).centered().size(Len::Fill(1), Len::Fill(1))
            }
        };
        Some(View::new(root.fill()))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        vec![
            item(CMD_EDIT, "Editar encomendas\u{2026}", false, None),
            item(CMD_HIDE_DELIVERED, "Ocultar entregues", self.hide_delivered, None),
        ]
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&Saved { hide_delivered: self.hide_delivered }).unwrap_or_default() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        self.stopped = true;
        Vec::new()
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

    fn data(n: usize) -> SharedData {
        let mut s = SharedData::default();
        for i in 0..n {
            let id = s.add_parcel(&format!("Compra {}", i + 1), &format!("AA12345678{i}BR")).unwrap();
            if i == 0 {
                s.toggle_parcel(id);
            }
        }
        s
    }

    fn make(s: &SharedData) -> Box<dyn Widget> {
        let mut w = create(None).unwrap();
        w.set_shared(s);
        w
    }

    fn names(w: &Box<dyn Widget>, shape: Shape) -> Vec<String> {
        let v = w.tile_view(&ctx(shape)).unwrap();
        let s = shape.size_dip();
        pod_model::a11y::items(&v, &layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake)).into_iter().map(|i| i.name).collect()
    }

    #[test]
    fn clicking_marks_delivery_through_the_shared_service() {
        let s = data(3);
        let mut w = make(&s);
        let id = s.parcels()[1].id;
        assert_eq!(w.handle(&Event::Clicked { button: LocalId(id as u32) }).effects, vec![Effect::SharedOp(SharedOp::ToggleParcel(id))]);
        assert_eq!(w.handle(&Event::Clicked { button: LocalId(999) }), Outcome::none());
    }

    #[test]
    fn counts_rows_and_overflow() {
        assert_eq!(names(&make(&data(3)), Shape::S1X1), vec!["2", "a caminho"]);
        let n = names(&make(&data(9)), Shape::S2X2);
        assert!(n.contains(&"Compra 1, conclu\u{ed}da".to_string()) && n.contains(&"+4 mais".to_string()), "{n:?}");
        assert!(names(&make(&SharedData::default()), Shape::S2X1).contains(&"Sem encomendas".to_string()));
    }

    #[test]
    fn hide_delivered_filters_persists_and_empty_message_changes() {
        let s = data(1); // a única já foi entregue
        let mut w = make(&s);
        assert!(names(&w, Shape::S2X1).contains(&"Compra 1, conclu\u{ed}da".to_string()));
        w.handle(&Event::MenuCommand(CMD_HIDE_DELIVERED));
        assert!(names(&w, Shape::S2X1).contains(&"Tudo entregue".to_string()));
        assert!(w.snapshot().data.contains("true"));
        assert!(create(Some(&w.snapshot())).unwrap().menu_items().iter().any(|i| i.id == CMD_HIDE_DELIVERED && i.checked));
        assert_eq!(w.handle(&Event::MenuCommand(CMD_EDIT)).effects, vec![Effect::OpenPanel(PanelKind::Parcels)]);
    }

    #[test]
    fn every_shape_fits_with_any_number_of_parcels() {
        for n in [0usize, 1, 2, 3, 6, 50] {
            let w = make(&data(n));
            for shape in descriptor().shapes {
                let v = w.tile_view(&ctx(shape)).unwrap();
                assert_eq!(v.validate(), Ok(()));
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{n} encomendas {shape}: {:?}", l.rect);
                }
            }
        }
    }

    #[test]
    fn bad_state_is_refused() {
        assert!(matches!(create(Some(&Snapshot { version: 1, data: "x".into() })), Err(WidgetError::InvalidState(_))));
        assert!(matches!(create(Some(&Snapshot { version: 2, data: "{}".into() })), Err(WidgetError::UnsupportedVersion { .. })));
    }
}
