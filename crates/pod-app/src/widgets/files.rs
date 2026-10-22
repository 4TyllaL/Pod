//! Widgets de pasta: Downloads e Capturas de tela (1×1, 2×1, 2×2). Mostram a contagem e os
//! arquivos mais recentes (nome, idade), só leitura: nenhum caminho chega ao widget e nada é
//! aberto. Assinam `Local` enquanto visíveis; o estado não é durável.

use pod_model::ids::WidgetTypeId;
use pod_model::local::{format_age, shorten_name, FolderListing, LocalNeeds, LocalSnapshot};
use pod_model::metrics::format_bytes;
use pod_model::shape::Shape;
use pod_model::view::{Align, Kind as VKind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, Outcome, SizeDip, Snapshot, StopReason, ViewCtx, Widget,
    WidgetError,
};

pub const DOWNLOADS: WidgetTypeId = WidgetTypeId("pod.downloads");
pub const SCREENSHOTS: WidgetTypeId = WidgetTypeId("pod.screenshots");
pub const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Downloads,
    Screenshots,
}

impl Kind {
    fn type_id(self) -> WidgetTypeId {
        match self {
            Kind::Downloads => DOWNLOADS,
            Kind::Screenshots => SCREENSHOTS,
        }
    }
    fn needs(self) -> LocalNeeds {
        match self {
            Kind::Downloads => LocalNeeds::DOWNLOADS,
            Kind::Screenshots => LocalNeeds::SCREENSHOTS,
        }
    }
    fn title(self) -> &'static str {
        match self {
            Kind::Downloads => "Downloads",
            Kind::Screenshots => "Capturas",
        }
    }
    fn pick(self, s: &LocalSnapshot) -> Option<&FolderListing> {
        match self {
            Kind::Downloads => s.downloads.as_ref(),
            Kind::Screenshots => s.screenshots.as_ref(),
        }
    }
}

pub fn descriptor(kind: Kind) -> Descriptor {
    Descriptor {
        type_id: kind.type_id(),
        state_version: STATE_VERSION,
        default_size: Shape::S2X2.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S4X1, Shape::S4X2],
        default_shape: Some(Shape::S2X2),
    }
}

fn create_kind(kind: Kind, restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(Folder { kind, listing: None, stopped: false }))
}

pub fn create_downloads(r: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    create_kind(Kind::Downloads, r)
}
pub fn create_screenshots(r: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    create_kind(Kind::Screenshots, r)
}

struct Folder {
    kind: Kind,
    /// `None` = ainda lendo ou pasta indisponível.
    listing: Option<FolderListing>,
    stopped: bool,
}

fn text(s: String, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(VKind::Text { text: s, style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

fn count_label(n: usize) -> String {
    match n {
        1 => "1 arquivo".to_string(),
        n => format!("{n} arquivos"),
    }
}

fn file_rows(l: &FolderListing, rows: usize, name_max: usize) -> Vec<Node> {
    l.recent
        .iter()
        .take(rows)
        .map(|f| {
            Node::row(6.0, 0.0, vec![
                text(shorten_name(&f.name, name_max), TextStyle::Caption, Role::Primary, Align::Start),
                text(format!("{} \u{b7} {}", format_bytes(f.size), format_age(f.age_secs)), TextStyle::Caption, Role::Secondary, Align::End)
                    .size(Len::Auto, Len::Auto),
            ])
            .size(Len::Fill(1), Len::Auto)
        })
        .collect()
}

fn folder_view(kind: Kind, shape: Shape, l: Option<&FolderListing>) -> View {
    let title = kind.title();
    let count = l.map_or_else(|| "\u{2014}".to_string(), |l| l.count.to_string());
    let sub = match l {
        None => "indispon\u{ed}vel".to_string(),
        Some(l) if l.count == 0 => "vazia".to_string(),
        Some(l) => count_label(l.count),
    };
    let root = match shape {
        s if s == Shape::S1X1 => Node::column(2.0, 6.0, vec![
            text(count, TextStyle::Headline, Role::Primary, Align::Center),
            text(title.to_uppercase(), TextStyle::Caption, Role::Secondary, Align::Center),
        ])
        .centered(),
        s if s == Shape::S2X1 => {
            let mut col = vec![Node::row(6.0, 0.0, vec![
                text(title.to_string(), TextStyle::Title, Role::Primary, Align::Start),
                text(sub, TextStyle::Caption, Role::Secondary, Align::End).size(Len::Auto, Len::Auto),
            ])
            .size(Len::Fill(1), Len::Auto)];
            if let Some(l) = l {
                col.extend(file_rows(l, 2, 16));
            }
            Node::column(4.0, 12.0, col)
        }
        _ => {
            let mut col = vec![
                text(title.to_string(), TextStyle::Title, Role::Primary, Align::Start),
                text(sub, TextStyle::Caption, Role::Secondary, Align::Start),
                Node::divider(),
            ];
            if let Some(l) = l {
                col.extend(file_rows(l, 5, 14));
            }
            Node::column(6.0, 14.0, col)
        }
    };
    View::new(root.fill())
}

impl Widget for Folder {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => {
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::Local(Some(self.kind.needs()))] }
            }
            Event::Local(snap) => {
                let next = self.kind.pick(snap).cloned();
                if next == self.listing {
                    return Outcome::none();
                }
                self.listing = next;
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
        Some(folder_view(self.kind, ctx.shape, self.listing.as_ref()))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: String::new() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        vec![Effect::Local(None)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::layout::{layout, Rect, TextMeasure};
    use pod_model::local::FileEntry;
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

    pub(crate) fn demo_listing() -> FolderListing {
        let f = |n: &str, size, age| FileEntry { name: n.into(), size, age_secs: age };
        FolderListing::from_files(vec![
            f("Relatorio-final-2026.pdf", 2_400_000, 120),
            f("foto-viagem.png", 5_800_000, 3_700),
            f("instalador.exe", 88_000_000, 90_000),
            f("nota.txt", 900, 30),
            f("planilha-orcamento-q4.xlsx", 310_000, 400),
            f("antigo.zip", 1_000_000, 900_000),
        ])
    }

    fn snap(l: Option<FolderListing>) -> Event {
        Event::Local(LocalSnapshot { downloads: l.clone(), screenshots: l, clipboard: None })
    }

    #[test]
    fn each_kind_subscribes_to_its_own_folder_and_stop_unsubscribes() {
        for (kind, needs) in [(Kind::Downloads, LocalNeeds::DOWNLOADS), (Kind::Screenshots, LocalNeeds::SCREENSHOTS)] {
            let mut w = create_kind(kind, None).unwrap();
            assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::Local(Some(needs))]);
            assert_eq!(w.stop(StopReason::Removed), vec![Effect::Local(None)]);
            assert!(w.stop(StopReason::Removed).is_empty());
            assert_eq!(w.handle(&snap(Some(demo_listing()))), Outcome::none(), "parado ignora eventos");
        }
    }

    #[test]
    fn only_a_changed_listing_invalidates() {
        let mut w = create_kind(Kind::Downloads, None).unwrap();
        assert_eq!(w.handle(&snap(Some(demo_listing()))).invalidate, Invalidation::CONTENT);
        assert_eq!(w.handle(&snap(Some(demo_listing()))), Outcome::none());
        // Pasta que some (movida/apagada) limpa o cartão em vez de manter a lista velha.
        assert_eq!(w.handle(&snap(None)).invalidate, Invalidation::CONTENT);
    }

    #[test]
    fn texts_in_portuguese() {
        assert_eq!(count_label(1), "1 arquivo");
        assert_eq!(count_label(42), "42 arquivos");
        let rows = file_rows(&demo_listing(), 5, 14);
        assert_eq!(rows.len(), 5, "no máximo as linhas pedidas");
    }

    #[test]
    fn every_kind_and_shape_fits_with_and_without_data() {
        for kind in [Kind::Downloads, Kind::Screenshots] {
            let mut w = create_kind(kind, None).unwrap();
            for data in [None, Some(FolderListing::default()), Some(demo_listing())] {
                w.handle(&snap(data));
                for shape in descriptor(kind).shapes {
                    let v = w.tile_view(&ctx(shape)).unwrap();
                    assert_eq!(v.validate(), Ok(()));
                    let s = shape.size_dip();
                    let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                    for l in &laid {
                        assert!(laid[0].rect.contains(&l.rect), "{kind:?} {shape}: {:?}", l.rect);
                    }
                }
            }
        }
    }

    #[test]
    fn future_state_is_refused_and_nothing_is_persisted() {
        assert!(matches!(
            create_kind(Kind::Downloads, Some(&Snapshot { version: 2, data: String::new() })),
            Err(WidgetError::UnsupportedVersion { .. })
        ));
        assert_eq!(create_kind(Kind::Screenshots, None).unwrap().snapshot().data, "");
    }
}
