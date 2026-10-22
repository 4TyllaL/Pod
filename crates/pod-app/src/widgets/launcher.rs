//! Atalhos: lançador de programas e pastas. Cada botão abre o programa (ou a pasta no Explorador).
//! Adicionar pelo menu: o host mostra o seletor de arquivos do Windows e devolve só o caminho; o host
//! valida de novo na hora de abrir (caminho absoluto, `.exe`/`.lnk` ou pasta, sem argumentos).

use pod_model::ids::WidgetTypeId;
use pod_model::launcher::{self, Shortcut, MAX_ITEMS};
use pod_model::shape::Shape;
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, LocalId, MenuItem, Outcome, SizeDip, Snapshot, StopReason,
    ViewCtx, Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.launcher");
pub const STATE_VERSION: u32 = 1;

const ADD_APP: u32 = 50;
const ADD_FOLDER: u32 = 51;
const REMOVE_BASE: u32 = 100;
/// Botão "Adicionar" do estado vazio.
const EMPTY_ADD: u32 = 900;

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X2.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S2X3, Shape::S4X1, Shape::S4X2],
        default_shape: Some(Shape::S2X2),
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    items: Vec<Shortcut>,
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
    // Arquivo editado à mão: o que não passa na validação é descartado.
    let mut items: Vec<Shortcut> = Vec::new();
    for s in saved.items {
        let _ = launcher::add(&mut items, &s.path, s.folder);
    }
    Ok(Box::new(Launcher { items, stopped: false }))
}

struct Launcher {
    items: Vec<Shortcut>,
    stopped: bool,
}

fn label(s: &str, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

fn button(id: u32, text: &str) -> Node {
    Node::new(Kind::Button { id: LocalId(id), label: text.to_string() }).size(Len::Fill(1), Len::Auto)
}

impl Launcher {
    /// Botões em linhas de `per_row`, até `max` atalhos.
    fn grid(&self, per_row: usize, max: usize) -> Node {
        let shown: Vec<&Shortcut> = self.items.iter().take(max).collect();
        let rows: Vec<Node> = shown
            .chunks(per_row)
            .enumerate()
            .map(|(r, chunk)| {
                let mut cells: Vec<Node> = chunk
                    .iter()
                    .enumerate()
                    .map(|(c, s)| button((r * per_row + c) as u32 + 1, &s.name))
                    .collect();
                while cells.len() < per_row {
                    cells.push(Node::spacer());
                }
                Node::row(8.0, 0.0, cells).size(Len::Fill(1), Len::Auto)
            })
            .collect();
        Node::column(8.0, 0.0, rows).size(Len::Fill(1), Len::Auto)
    }

    fn view_for(&self, shape: Shape) -> View {
        let root = if self.items.is_empty() {
            Node::column(6.0, 12.0, vec![
                label("Sem atalhos", TextStyle::Body, Role::Secondary, Align::Center),
                button(EMPTY_ADD, "Adicionar"),
            ])
            .centered()
        } else if shape == Shape::S1X1 {
            // Um só botão grande com o primeiro atalho e a contagem.
            Node::column(4.0, 10.0, vec![
                button(1, &self.items[0].name),
                label(&format!("{} atalhos", self.items.len()), TextStyle::Caption, Role::Secondary, Align::Center),
            ])
            .centered()
        } else {
            let (per_row, max) = match (shape.cols >= 3, shape.rows) {
                (true, 1) => (4, 4),
                (true, r) => (4, 4 * r as usize),
                (false, 1) => (2, 2),
                (false, r) => (2, 2 * (r as usize + 1)),
            };
            let mut kids = Vec::new();
            if shape.rows >= 2 {
                kids.push(label("Atalhos", TextStyle::Title, Role::Primary, Align::Start));
            }
            kids.push(self.grid(per_row, max));
            if self.items.len() > max {
                kids.push(label(&format!("+{} mais", self.items.len() - max), TextStyle::Caption, Role::Secondary, Align::Start));
            }
            Node::column(10.0, 14.0, kids)
        };
        View::new(root.fill())
    }

    fn pick(&self, folder: bool) -> Outcome {
        if self.items.len() >= MAX_ITEMS {
            return Outcome::none();
        }
        Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::PickPath { folder }] }
    }
}

impl Widget for Launcher {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Clicked { button } if button.0 == EMPTY_ADD => self.pick(false),
            Event::MenuCommand(ADD_APP) => self.pick(false),
            Event::MenuCommand(ADD_FOLDER) => self.pick(true),
            Event::Clicked { button } => {
                match (button.0 as usize).checked_sub(1).and_then(|i| self.items.get(i)) {
                    Some(s) => Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::Launch { path: s.path.clone(), folder: s.folder }] },
                    None => Outcome::none(),
                }
            }
            Event::PathPicked { path, folder } => match launcher::add(&mut self.items, path, *folder) {
                Ok(()) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged] },
                Err(e) => Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::InputError(e.message().into())] },
            },
            Event::MenuCommand(c) if *c >= REMOVE_BASE && ((*c - REMOVE_BASE) as usize) < self.items.len() => {
                self.items.remove((*c - REMOVE_BASE) as usize);
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged] }
            }
            Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        Some(self.view_for(ctx.shape))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        let mut m = vec![
            MenuItem { id: ADD_APP, label: "Adicionar programa\u{2026}".into(), checked: false, group: None },
            MenuItem { id: ADD_FOLDER, label: "Adicionar pasta\u{2026}".into(), checked: false, group: None },
        ];
        for (i, s) in self.items.iter().enumerate() {
            m.push(MenuItem { id: REMOVE_BASE + i as u32, label: s.name.clone(), checked: false, group: Some("Remover atalho") });
        }
        m
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&Saved { items: self.items.clone() }).unwrap_or_default() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        self.stopped = true;
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::time::{weekday_of, TimeInfo};

    fn ctx(shape: Shape) -> ViewCtx {
        ViewCtx { shape, time: TimeInfo { year: 2026, month: 10, day: 19, weekday: weekday_of(2026, 10, 19), hour: 9, minute: 0, second: 0, millis: 0 } }
    }

    fn with(n: usize) -> Box<dyn Widget> {
        let mut w = create(None).unwrap();
        for i in 0..n {
            w.handle(&Event::PathPicked { path: format!("C:\\Apps\\app{i}.exe"), folder: false });
        }
        w
    }

    #[test]
    fn picking_adds_validated_shortcuts_and_clicking_launches_them() {
        let mut w = with(0);
        assert_eq!(w.handle(&Event::Clicked { button: LocalId(EMPTY_ADD) }).effects, vec![Effect::PickPath { folder: false }]);
        assert_eq!(w.handle(&Event::MenuCommand(ADD_FOLDER)).effects, vec![Effect::PickPath { folder: true }]);
        let bad = w.handle(&Event::PathPicked { path: "C:\\x\\run.bat".into(), folder: false });
        assert!(matches!(bad.effects.as_slice(), [Effect::InputError(_)]));
        let ok = w.handle(&Event::PathPicked { path: "C:\\Apps\\Notepad.exe".into(), folder: false });
        assert_eq!(ok.effects, vec![Effect::StateChanged]);
        assert_eq!(
            w.handle(&Event::Clicked { button: LocalId(1) }).effects,
            vec![Effect::Launch { path: "C:\\Apps\\Notepad.exe".into(), folder: false }]
        );
        assert_eq!(w.handle(&Event::Clicked { button: LocalId(7) }), Outcome::none());
    }

    #[test]
    fn state_roundtrips_and_hand_edited_junk_is_dropped() {
        let w = with(3);
        let again = create(Some(&w.snapshot())).unwrap();
        assert_eq!(again.snapshot().data, w.snapshot().data);
        let junk = Snapshot { version: 1, data: r#"{"items":[{"name":"x","path":"calc.exe","folder":false},{"name":"y","path":"C:\\a\\b.bat","folder":false},{"name":"z","path":"C:\\ok\\z.exe","folder":false}]}"#.into() };
        let w = create(Some(&junk)).unwrap();
        assert!(w.snapshot().data.contains("z.exe") && !w.snapshot().data.contains("calc") && !w.snapshot().data.contains(".bat"));
        assert!(create(Some(&Snapshot { version: 1, data: "{".into() })).is_err());
        assert!(create(Some(&Snapshot { version: 2, data: String::new() })).is_err());
    }

    #[test]
    fn menu_removes_and_every_shape_is_valid() {
        let mut w = with(5);
        assert_eq!(w.menu_items().len(), 2 + 5);
        w.handle(&Event::MenuCommand(REMOVE_BASE + 2));
        assert_eq!(w.menu_items().len(), 2 + 4);
        for n in [0, 1, 3, 12] {
            let w = with(n);
            for shape in descriptor().shapes {
                assert!(w.tile_view(&ctx(shape)).unwrap().validate().is_ok(), "{n} {shape}");
            }
        }
    }
}
