//! Reminders: lista de tarefas com caixas de seleção clicáveis (1×1 contagem, 2×1 duas tarefas,
//! 2×2 cinco). As tarefas são dados compartilhados; marcar/desmarcar não abre o painel.

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::shared::{SharedData, SharedOp, Task};
use pod_model::view::{Align, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, LocalId, MenuItem, Outcome, PanelKind, SizeDip, Snapshot,
    StopReason, ViewCtx, Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

use super::tkit::{item, label};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.reminders");
pub const STATE_VERSION: u32 = 1;
const CMD_EDIT: u32 = 1;
const CMD_HIDE_DONE: u32 = 2;

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

#[derive(Serialize, Deserialize)]
struct Saved {
    hide_done: bool,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let saved = match restore {
        None => Saved { hide_done: false },
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) => serde_json::from_str::<Saved>(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    Ok(Box::new(Reminders { hide_done: saved.hide_done, shared: SharedData::default(), stopped: false }))
}

struct Reminders {
    hide_done: bool,
    shared: SharedData,
    stopped: bool,
}

impl Reminders {
    fn visible(&self) -> Vec<&Task> {
        self.shared.tasks().iter().filter(|t| !(self.hide_done && t.done)).collect()
    }

    fn rows(&self, max: usize) -> Vec<Node> {
        self.visible().into_iter().take(max).map(|t| Node::check(LocalId(t.id as u32), t.title.clone(), t.done)).collect()
    }

    fn overflow(&self, max: usize) -> usize {
        self.visible().len().saturating_sub(max)
    }
}

impl Widget for Reminders {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Clicked { button } => {
                let id = button.0 as u64;
                if self.shared.tasks().iter().any(|t| t.id == id) {
                    Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::SharedOp(SharedOp::ToggleTask(id))] }
                } else {
                    Outcome::none()
                }
            }
            Event::MenuCommand(CMD_EDIT) => {
                Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::OpenPanel(PanelKind::Tasks)] }
            }
            Event::MenuCommand(CMD_HIDE_DONE) => {
                self.hide_done = !self.hide_done;
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
        let pending = self.shared.pending_tasks();
        let none_text = if self.shared.tasks().is_empty() { "Sem tarefas" } else { "Tudo feito" };
        let more = |n: usize| label(format!("+{n} mais"), TextStyle::Caption, Role::Secondary, Align::Start);
        let root = match ctx.shape {
            s if s == Shape::S1X1 => Node::column(0.0, 6.0, vec![
                label(pending.to_string(), TextStyle::Display, if pending == 0 { Role::Positive } else { Role::Primary }, Align::Center),
                label(
                    if pending == 1 { "pendente" } else { "pendentes" }.to_string(),
                    TextStyle::Caption,
                    Role::Secondary,
                    Align::Center,
                ),
            ])
            .centered(),
            s if s.cols == 2 && s.rows >= 2 => {
                // 5 linhas no 2×2 e mais 3 por linha de grade extra (2×3 = 8, 2×4 = 11).
                let max = 5 + (s.rows as usize - 2) * 3;
                let mut kids = vec![super::tkit::chip_header('\u{2713}', "Tarefas", false)];
                let rows = self.rows(max);
                if rows.is_empty() {
                    kids.push(label(none_text.into(), TextStyle::Body, Role::Secondary, Align::Start));
                }
                kids.extend(rows);
                if self.overflow(max) > 0 {
                    kids.push(more(self.overflow(max)));
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
            item(CMD_EDIT, "Editar tarefas\u{2026}", false, None),
            item(CMD_HIDE_DONE, "Ocultar conclu\u{ed}das", self.hide_done, None),
        ]
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&Saved { hide_done: self.hide_done }).unwrap_or_default() }
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

    fn data(n: usize, done_first: bool) -> SharedData {
        let mut s = SharedData::default();
        for i in 0..n {
            let id = s.add_task(&format!("Tarefa {}", i + 1)).unwrap();
            if done_first && i == 0 {
                s.toggle_task(id);
            }
        }
        s
    }

    fn make(shared: &SharedData) -> Box<dyn Widget> {
        let mut w = create(None).unwrap();
        w.set_shared(shared);
        w
    }

    #[test]
    fn clicking_a_checkbox_asks_for_the_toggle_and_ignores_unknown_ids() {
        let s = data(3, false);
        let mut w = make(&s);
        let id = s.tasks()[1].id;
        let out = w.handle(&Event::Clicked { button: LocalId(id as u32) });
        assert_eq!(out.effects, vec![Effect::SharedOp(SharedOp::ToggleTask(id))]);
        assert_eq!(w.handle(&Event::Clicked { button: LocalId(999) }), Outcome::none());
    }

    #[test]
    fn hide_done_filters_and_is_persisted() {
        let s = data(3, true);
        let mut w = make(&s);
        let count = |w: &Box<dyn Widget>| {
            let v = w.tile_view(&ctx(Shape::S2X2)).unwrap();
            let s = Shape::S2X2.size_dip();
            button_rects(&v, &layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake)).len()
        };
        assert_eq!(count(&w), 3);
        let out = w.handle(&Event::MenuCommand(CMD_HIDE_DONE));
        assert!(out.effects.contains(&Effect::StateChanged));
        assert_eq!(count(&w), 2, "a concluída some");
        assert!(w.snapshot().data.contains("true"));
        let again = create(Some(&w.snapshot())).unwrap();
        assert!(again.menu_items().iter().any(|i| i.id == CMD_HIDE_DONE && i.checked));
    }

    #[test]
    fn counts_and_placeholders() {
        let pending = |w: &Box<dyn Widget>| -> Vec<String> {
            let v = w.tile_view(&ctx(Shape::S1X1)).unwrap();
            let s = Shape::S1X1.size_dip();
            pod_model::a11y::items(&v, &layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake)).into_iter().map(|i| i.name).collect()
        };
        assert_eq!(pending(&make(&data(3, true))), vec!["2", "pendentes"]);
        assert_eq!(pending(&make(&data(2, true))), vec!["1", "pendente"]);
        assert_eq!(pending(&make(&SharedData::default())), vec!["0", "pendentes"]);
    }

    #[test]
    fn long_lists_show_the_overflow_and_everything_fits() {
        for n in [0usize, 1, 2, 3, 6, 40] {
            let w = make(&data(n, true));
            for shape in descriptor().shapes {
                let v = w.tile_view(&ctx(shape)).unwrap();
                assert_eq!(v.validate(), Ok(()));
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{n} tarefas {shape}: {:?}", l.rect);
                }
            }
        }
        let w = make(&data(8, false));
        let v = w.tile_view(&ctx(Shape::S2X2)).unwrap();
        let s = Shape::S2X2.size_dip();
        let names: Vec<String> =
            pod_model::a11y::items(&v, &layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake)).into_iter().map(|i| i.name).collect();
        assert!(names.contains(&"+3 mais".to_string()), "{names:?}");
        assert!(names.iter().any(|n| n == "Tarefa 1, pendente"));
    }

    #[test]
    fn menu_opens_the_panel() {
        let mut w = create(None).unwrap();
        assert_eq!(w.handle(&Event::MenuCommand(CMD_EDIT)).effects, vec![Effect::OpenPanel(PanelKind::Tasks)]);
        assert!(matches!(create(Some(&Snapshot { version: 1, data: "x".into() })), Err(WidgetError::InvalidState(_))));
        assert!(matches!(create(Some(&Snapshot { version: 2, data: "{}".into() })), Err(WidgetError::UnsupportedVersion { .. })));
    }
}
