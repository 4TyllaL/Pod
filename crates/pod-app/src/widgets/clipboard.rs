//! Widget Área de transferência: prévia do último texto copiado (1×1, 2×1, 2×2). Só leitura:
//! não copia nada de volta. Conteúdo marcado como privado pelo aplicativo de origem nunca é lido
//! (o provedor entrega `Clip::Private`). A prévia fica só na memória: o estado não é durável.

use pod_model::ids::WidgetTypeId;
use pod_model::local::{shorten_name, Clip, LocalNeeds};
use pod_model::shape::Shape;
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, Outcome, SizeDip, Snapshot, StopReason, ViewCtx, Widget,
    WidgetError,
};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.clipboard");
pub const STATE_VERSION: u32 = 1;

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

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(Clipboard { clip: None, stopped: false }))
}

struct Clipboard {
    clip: Option<Clip>,
    stopped: bool,
}

fn text(s: String, style: TextStyle, role: Role, align: Align, max_lines: u8) -> Node {
    Node::new(Kind::Text { text: s, style, role, align, max_lines }).size(Len::Fill(1), Len::Auto)
}

/// (texto do corpo, é conteúdo real?) para cada estado.
fn body(clip: Option<&Clip>) -> (String, bool) {
    match clip {
        None => ("lendo\u{2026}".into(), false),
        Some(Clip::Empty) => ("Vazia".into(), false),
        Some(Clip::Other) => ("Imagem ou arquivos".into(), false),
        Some(Clip::Private) => ("Conte\u{fa}do privado".into(), false),
        Some(Clip::Text(t)) => (t.clone(), true),
    }
}

fn clip_view(shape: Shape, clip: Option<&Clip>) -> View {
    let (body, real) = body(clip);
    let role = if real { Role::Primary } else { Role::Secondary };
    let root = match shape {
        s if s == Shape::S1X1 => Node::column(2.0, 6.0, vec![
            text(shorten_name(&body, 14), TextStyle::Caption, role, Align::Center, 1),
            text("COPIADO".into(), TextStyle::Caption, Role::Tertiary, Align::Center, 1),
        ])
        .centered(),
        s if s == Shape::S2X2 => Node::column(8.0, 14.0, vec![
            text("\u{c1}rea de transfer\u{ea}ncia".into(), TextStyle::Body, Role::Secondary, Align::Start, 1),
            Node::divider(),
            text(body, TextStyle::Body, role, Align::Start, 7),
        ]),
        _ => Node::column(4.0, 12.0, vec![
            text("\u{c1}rea de transfer\u{ea}ncia".into(), TextStyle::Caption, Role::Secondary, Align::Start, 1),
            text(body, TextStyle::Body, role, Align::Start, 3),
        ]),
    };
    View::new(root.fill())
}

impl Widget for Clipboard {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => {
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::Local(Some(LocalNeeds::CLIPBOARD))] }
            }
            Event::Local(snap) => {
                if snap.clipboard == self.clip {
                    return Outcome::none();
                }
                self.clip = snap.clipboard.clone();
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![] }
            }
            Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        Some(clip_view(ctx.shape, self.clip.as_ref()))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: String::new() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        // Descarta a prévia: o texto copiado não fica retido depois que o widget sai.
        self.clip = None;
        vec![Effect::Local(None)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::layout::{layout, Rect, TextMeasure};
    use pod_model::local::LocalSnapshot;
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

    fn ev(c: Option<Clip>) -> Event {
        Event::Local(LocalSnapshot { clipboard: c, ..Default::default() })
    }

    #[test]
    fn subscribes_to_the_clipboard_only_and_stop_drops_the_preview() {
        let mut w = create(None).unwrap();
        assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::Local(Some(LocalNeeds::CLIPBOARD))]);
        w.handle(&ev(Some(Clip::Text("segredo".into()))));
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::Local(None)]);
        assert!(w.stop(StopReason::Removed).is_empty());
        let (b, real) = body(None);
        assert!(!real && b != "segredo");
        assert_eq!(w.handle(&ev(Some(Clip::Text("x".into())))), Outcome::none());
    }

    #[test]
    fn only_a_changed_clip_invalidates() {
        let mut w = create(None).unwrap();
        let t = || ev(Some(Clip::Text("ol\u{e1}".into())));
        assert_eq!(w.handle(&t()).invalidate, Invalidation::CONTENT);
        assert_eq!(w.handle(&t()), Outcome::none());
        assert_eq!(w.handle(&ev(Some(Clip::Private))).invalidate, Invalidation::CONTENT);
    }

    #[test]
    fn private_content_never_shows_text_and_states_have_labels() {
        assert_eq!(body(Some(&Clip::Private)), ("Conte\u{fa}do privado".to_string(), false));
        assert_eq!(body(Some(&Clip::Empty)).0, "Vazia");
        assert_eq!(body(Some(&Clip::Other)).0, "Imagem ou arquivos");
        assert_eq!(body(Some(&Clip::Text("abc".into()))), ("abc".to_string(), true));
    }

    #[test]
    fn every_state_and_shape_fits_the_tile() {
        let long = "Texto longo copiado do navegador com muitas palavras ".repeat(10);
        let states = [
            None,
            Some(Clip::Empty),
            Some(Clip::Other),
            Some(Clip::Private),
            Some(Clip::Text("curto".into())),
            Some(Clip::Text(pod_model::local::preview_text(&long))),
        ];
        for st in states {
            let mut w = create(None).unwrap();
            w.handle(&ev(st));
            for shape in descriptor().shapes {
                let v = w.tile_view(&ctx(shape)).unwrap();
                assert_eq!(v.validate(), Ok(()));
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{shape}: {:?}", l.rect);
                }
            }
        }
    }

    #[test]
    fn future_state_is_refused() {
        assert!(matches!(
            create(Some(&Snapshot { version: 2, data: String::new() })),
            Err(WidgetError::UnsupportedVersion { .. })
        ));
    }
}
