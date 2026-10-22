//! Favorites: atalhos para sites que o usuário salvou (1×1, 2×1, 2×2). Cada botão abre o link no
//! navegador padrão. O widget só conhece o id do link: o endereço é buscado e revalidado pelo host.

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::shared::{Link, SharedData};
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, LocalId, MenuItem, Outcome, PanelKind, SizeDip, Snapshot,
    StopReason, ViewCtx, Widget, WidgetError,
};

use super::tkit::{item, label};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.favorites");
pub const STATE_VERSION: u32 = 1;
const CMD_EDIT: u32 = 1;
/// Abrir um link pelo menu: `CMD_OPEN_BASE + id`.
const CMD_OPEN_BASE: u32 = 1000;

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

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(Favorites { shared: SharedData::default(), stopped: false }))
}

struct Favorites {
    shared: SharedData,
    stopped: bool,
}

fn link_button(l: &Link) -> Node {
    Node::new(Kind::Button { id: LocalId(l.id as u32), label: l.title.clone() }).size(Len::Fill(1), Len::Auto)
}

impl Favorites {
    fn open(&self, id: u64) -> Outcome {
        if self.shared.link(id).is_some() {
            Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::OpenUri(id)] }
        } else {
            Outcome::none()
        }
    }
}

impl Widget for Favorites {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Clicked { button } => self.open(button.0 as u64),
            Event::MenuCommand(CMD_EDIT) => {
                Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::OpenPanel(PanelKind::Links)] }
            }
            Event::MenuCommand(c) if *c > CMD_OPEN_BASE => self.open((*c - CMD_OPEN_BASE) as u64),
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
        let links = self.shared.links();
        let empty = |style| {
            Node::column(4.0, 10.0, vec![
                label("Sem favoritos".into(), style, Role::Secondary, Align::Center),
                label("Menu \u{2192} Editar favoritos\u{2026}".into(), TextStyle::Caption, Role::Tertiary, Align::Center),
            ])
            .centered()
        };
        let root = match (links.is_empty(), ctx.shape) {
            (true, s) if s == Shape::S1X1 => Node::column(2.0, 6.0, vec![
                label("Sem".into(), TextStyle::Body, Role::Secondary, Align::Center),
                label("favoritos".into(), TextStyle::Caption, Role::Tertiary, Align::Center),
            ])
            .centered(),
            (true, _) => empty(TextStyle::Title),
            (false, s) if s == Shape::S1X1 => Node::column(4.0, 6.0, vec![
                label(format!("{}", links.len()), TextStyle::Headline, Role::Primary, Align::Center),
                link_button(&links[0]),
            ])
            .centered(),
            (false, s) if s == Shape::S2X2 => {
                let mut rows: Vec<Node> = vec![label("Favoritos".into(), TextStyle::Title, Role::Primary, Align::Start)];
                for pair in links.chunks(2).take(3) {
                    let mut cells: Vec<Node> = pair.iter().map(link_button).collect();
                    if cells.len() == 1 {
                        cells.push(Node::spacer().size(Len::Fill(1), Len::Fixed(1.0)));
                    }
                    rows.push(Node::row(8.0, 0.0, cells).size(Len::Fill(1), Len::Auto));
                }
                if links.len() > 6 {
                    rows.push(label(format!("+{} mais", links.len() - 6), TextStyle::Caption, Role::Secondary, Align::Start));
                }
                Node::column(8.0, 14.0, rows)
            }
            (false, _) => {
                let mut cells: Vec<Node> = links.iter().take(2).map(link_button).collect();
                if cells.len() == 1 {
                    cells.push(Node::spacer().size(Len::Fill(1), Len::Fixed(1.0)));
                }
                Node::row(8.0, 12.0, cells).centered()
            }
        };
        Some(View::new(root.fill()))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        let mut items = vec![item(CMD_EDIT, "Editar favoritos\u{2026}", false, None)];
        for l in self.shared.links() {
            items.push(item(CMD_OPEN_BASE + l.id as u32, &l.title, false, Some("Abrir")));
        }
        items
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: String::new() }
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
            shape: shape.design(), // como o host faz
            time: TimeInfo { year: 2026, month: 10, day: 18, weekday: weekday_of(2026, 10, 18), hour: 10, minute: 0, second: 0, millis: 0 },
        }
    }

    fn data(n: usize) -> SharedData {
        let mut s = SharedData::default();
        for i in 0..n {
            s.add_link(&format!("Site n\u{ba} {}", i + 1), &format!("site{}.com.br", i + 1)).unwrap();
        }
        s
    }

    fn make(n: usize) -> Box<dyn Widget> {
        let mut w = create(None).unwrap();
        w.set_shared(&data(n));
        w
    }

    #[test]
    fn clicking_a_link_asks_the_host_to_open_it_by_id_only() {
        let s = data(3);
        let mut w = make(3);
        let id = s.links()[1].id;
        assert_eq!(w.handle(&Event::Clicked { button: LocalId(id as u32) }).effects, vec![Effect::OpenUri(id)]);
        assert_eq!(w.handle(&Event::Clicked { button: LocalId(9999) }), Outcome::none(), "id desconhecido não abre nada");
        assert_eq!(w.handle(&Event::MenuCommand(CMD_OPEN_BASE + id as u32)).effects, vec![Effect::OpenUri(id)]);
        assert_eq!(w.handle(&Event::MenuCommand(CMD_OPEN_BASE + 9999)), Outcome::none());
    }

    #[test]
    fn menu_lists_every_link_and_the_editor() {
        let mut w = make(4);
        assert_eq!(w.handle(&Event::MenuCommand(CMD_EDIT)).effects, vec![Effect::OpenPanel(PanelKind::Links)]);
        let items = w.menu_items();
        assert_eq!(items.iter().filter(|i| i.group == Some("Abrir")).count(), 4);
    }

    #[test]
    fn every_shape_fits_with_few_and_many_links() {
        for n in [0usize, 1, 2, 5, 6, 7, 30] {
            let w = make(n);
            for shape in descriptor().shapes {
                let v = w.tile_view(&ctx(shape)).unwrap();
                assert_eq!(v.validate(), Ok(()));
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{n} links {shape}: {:?}", l.rect);
                }
                let buttons = button_rects(&v, &laid).len();
                let want = match (n, shape.design()) {
                    (0, _) => 0,
                    (_, s) if s == Shape::S1X1 => 1,
                    (n, s) if s == Shape::S2X1 => n.min(2),
                    (n, _) => n.min(6),
                };
                assert_eq!(buttons, want, "{n} links {shape}");
            }
        }
    }

    #[test]
    fn rows_do_not_overflow_with_a_long_title() {
        let mut s = SharedData::default();
        s.add_link(&"T\u{ed}tulo muito longo ".repeat(4), "exemplo.com").unwrap();
        s.add_link("Curto", "exemplo.org").unwrap();
        let mut w = create(None).unwrap();
        w.set_shared(&s);
        let v = w.tile_view(&ctx(Shape::S2X1)).unwrap();
        let laid = layout(&v, Rect::new(0.0, 0.0, 188.0, 88.0), &Fake);
        for l in &laid {
            assert!(laid[0].rect.contains(&l.rect), "{:?}", l.rect);
        }
    }

    #[test]
    fn future_state_is_refused() {
        assert!(matches!(create(Some(&Snapshot { version: 2, data: String::new() })), Err(WidgetError::UnsupportedVersion { .. })));
    }
}
