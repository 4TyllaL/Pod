//! Widget News: manchetes de um feed RSS/Atom do catálogo, em 1×1, 2×1 e 2×2. **Usa a internet**: só
//! funciona com "Widgets de internet" ligado na bandeja (desligado por padrão); sem isso explica como
//! ligar e nada sai. A fonte (um id do catálogo) é o único estado durável. As manchetes passam sozinhas a
//! cada 12 s; ◀ ▶ navegam e "Abrir" abre a matéria no navegador padrão (o host valida o endereço: o
//! widget só pede "abrir a manchete N", nunca fornece URL).

use serde::{Deserialize, Serialize};

use pod_model::ids::WidgetTypeId;
use pod_model::news::{feed_by_id, Headline, NewsState, DEFAULT_FEED, FEEDS};
use pod_model::shape::Shape;
use pod_model::view::{Align, Fill, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, LocalId, MenuItem, Outcome, SizeDip, Snapshot, StopReason,
    ViewCtx, Widget, WidgetError,
};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.news");
pub const STATE_VERSION: u32 = 1;

const PREV: LocalId = LocalId(1);
const OPEN: LocalId = LocalId(2);
const NEXT: LocalId = LocalId(3);
const FEED_BASE: u32 = 100;
const OPEN_CMD: u32 = 50;
const NEXT_CMD: u32 = 51;
const ROTATE_TOKEN: u64 = 1;
const ROTATE_MS: u64 = 12_000;

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
    feed: String,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let saved = match restore {
        None => Saved { feed: DEFAULT_FEED.into() },
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) if s.data.is_empty() => Saved { feed: DEFAULT_FEED.into() },
        Some(s) => serde_json::from_str::<Saved>(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    // Fonte que não existe mais no catálogo: volta ao padrão em vez de recusar o widget.
    let feed = if feed_by_id(&saved.feed).is_some() { saved.feed } else { DEFAULT_FEED.to_string() };
    Ok(Box::new(News { feed, state: NewsState::Loading, index: 0, now: 0, rotating: false, stopped: false }))
}

struct News {
    feed: String,
    state: NewsState,
    /// Manchete mostrada (não é durável).
    index: usize,
    now: u64,
    rotating: bool,
    stopped: bool,
}

fn text(s: &str, style: TextStyle, role: Role, align: Align, lines: u8) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align, max_lines: lines }).size(Len::Fill(1), Len::Auto)
}


/// Vermelho do cartão News (o mesmo do Dockn't): degradê e ponto "ao vivo".
const RED_TOP: [u8; 3] = [214, 40, 46];
const RED_BOTTOM: [u8; 3] = [132, 16, 26];
const NEWS_GLYPH: &str = "\u{1f4f0}";

fn red() -> Fill {
    Fill::Gradient { top: RED_TOP, bottom: RED_BOTTOM }
}

/// Selo redondo vermelho com o jornal branco (o "lead" do cartão).
fn badge(size: f32) -> Node {
    let inner = size * 0.5;
    Node::plate(red(), size / 2.0, (size - inner) / 2.0, text(NEWS_GLYPH, TextStyle::Body, Role::OnFill, Align::Center, 1).size(Len::Fixed(inner), Len::Fixed(inner)))
        .size(Len::Fixed(size), Len::Fixed(size))
}

/// Manchetes numa linha só, separadas por " • " (o letreiro).
fn ticker_text(items: &[Headline]) -> String {
    items.iter().map(|h| h.title.as_str()).collect::<Vec<_>>().join("     \u{2022}     ")
}

fn marquee(s: &str, style: TextStyle, role: Role) -> Node {
    Node::new(Kind::Marquee { text: s.to_string(), style, role }).size(Len::Fill(1), Len::Auto)
}

/// "● FONTE": ponto vermelho e o nome da fonte em caixa alta.
fn live_label(source: &str) -> Node {
    Node::row(5.0, 0.0, vec![
        text("\u{25cf}", TextStyle::Caption, Role::Danger, Align::Start, 1).size(Len::Fixed(8.0), Len::Auto),
        text(&source.to_uppercase(), TextStyle::Caption, Role::Secondary, Align::Start, 1),
    ])
    .size(Len::Fill(1), Len::Auto)
}

fn status_text(state: &NewsState) -> (String, String) {
    match state {
        NewsState::Off => ("News".into(), "Ligue \"Widgets de internet\" na bandeja".into()),
        NewsState::Loading => ("Carregando".into(), "Buscando manchetes\u{2026}".into()),
        NewsState::Failed(m) => ("Sem conex\u{e3}o".into(), m.clone()),
        NewsState::Ready { .. } => ("News".into(), "Sem manchetes".into()),
    }
}

fn view_for(shape: Shape, state: &NewsState, index: usize) -> View {
    let ready = match state {
        NewsState::Ready { source, items } if !items.is_empty() => Some((source, items)),
        _ => None,
    };
    // 1×1: cartão todo vermelho, jornal em cima e o letreiro embaixo.
    if shape == Shape::S1X1 {
        let bottom = match ready {
            Some((_, items)) => marquee(&ticker_text(items), TextStyle::Caption, Role::OnFill),
            None => text("NEWS", TextStyle::Caption, Role::OnFill, Align::Center, 1),
        };
        let col = Node::column(6.0, 12.0, vec![
            Node::spacer(),
            text(NEWS_GLYPH, TextStyle::Headline, Role::OnFill, Align::Center, 1),
            Node::spacer(),
            bottom,
        ]);
        return View::new(Node::plate(red(), crate::render::TILE_RADIUS_DIP, 0.0, col.fill()).fill());
    }
    let root = match ready {
        None => {
            let (title, sub) = status_text(state);
            Node::row(12.0, 14.0, vec![
                badge(40.0),
                Node::column(2.0, 0.0, vec![
                    text(&title, TextStyle::Title, Role::Primary, Align::Start, 1),
                    text(&sub, TextStyle::Caption, Role::Secondary, Align::Start, 2),
                ])
                .size(Len::Fill(1), Len::Auto),
            ])
            .centered()
        }
        // Faixa (2×1, 4×1): selo, "● FONTE" e o letreiro correndo — como no Dock.
        Some((source, items)) if shape.rows == 1 => Node::row(12.0, 14.0, vec![
            badge(40.0),
            Node::column(4.0, 0.0, vec![live_label(source), marquee(&ticker_text(items), TextStyle::Body, Role::Primary)])
                .size(Len::Fill(1), Len::Auto),
        ])
        .centered(),
        // Maiores: o letreiro no topo e a lista das manchetes embaixo.
        Some((source, items)) => {
            let i = index % items.len();
            let lines = (shape.rows as usize * 2).saturating_sub(1).max(2);
            let mut kids = vec![
                Node::row(10.0, 0.0, vec![
                    badge(32.0),
                    Node::column(3.0, 0.0, vec![live_label(source), marquee(&ticker_text(items), TextStyle::Body, Role::Primary)])
                        .size(Len::Fill(1), Len::Auto),
                ])
                .size(Len::Fill(1), Len::Auto),
                Node::divider(),
            ];
            for k in 0..lines.min(items.len()) {
                let h = &items[(i + k) % items.len()];
                kids.push(text(&h.title, TextStyle::Body, if k == 0 { Role::Primary } else { Role::Secondary }, Align::Start, 2));
            }
            Node::column(8.0, 14.0, kids)
        }
    };
    View::new(root.fill())
}

impl News {
    fn len(&self) -> usize {
        match &self.state {
            NewsState::Ready { items, .. } => items.len(),
            _ => 0,
        }
    }

    fn step(&mut self, delta: isize) {
        let n = self.len() as isize;
        if n > 0 {
            self.index = ((self.index as isize + delta).rem_euclid(n)) as usize;
        }
    }

    fn rotate_effect(&self) -> Effect {
        Effect::ScheduleAt { deadline_ms: self.now + ROTATE_MS, token: ROTATE_TOKEN }
    }

    /// Navegação manual: reinicia a contagem da troca automática.
    fn navigated(&mut self) -> Outcome {
        let mut effects = Vec::new();
        if self.rotating {
            effects.push(Effect::CancelTimer { token: ROTATE_TOKEN });
            effects.push(self.rotate_effect());
        }
        Outcome { invalidate: Invalidation::CONTENT, effects }
    }
}

impl Widget for News {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { now_ms } => {
                self.now = *now_ms;
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::News(Some(self.feed.clone()))] }
            }
            Event::News(s) => {
                if *s == self.state {
                    return Outcome::none();
                }
                self.state = s.clone();
                if self.index >= self.len().max(1) {
                    self.index = 0;
                }
                let mut effects = Vec::new();
                // Liga a troca automática quando há mais de uma manchete; desliga quando some.
                if self.len() > 1 && !self.rotating {
                    self.rotating = true;
                    effects.push(self.rotate_effect());
                } else if self.len() <= 1 && self.rotating {
                    self.rotating = false;
                    effects.push(Effect::CancelTimer { token: ROTATE_TOKEN });
                }
                Outcome { invalidate: Invalidation::CONTENT, effects }
            }
            Event::Timer { token: ROTATE_TOKEN, now_ms } => {
                self.now = *now_ms;
                if !self.rotating || self.len() < 2 {
                    return Outcome::none();
                }
                self.step(1);
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![self.rotate_effect()] }
            }
            Event::Clicked { button } if *button == PREV => {
                self.step(-1);
                self.navigated()
            }
            Event::Clicked { button } if *button == NEXT => {
                self.step(1);
                self.navigated()
            }
            Event::MenuCommand(NEXT_CMD) => {
                self.step(1);
                self.navigated()
            }
            Event::Clicked { button } if *button == OPEN => self.open(),
            Event::MenuCommand(OPEN_CMD) => self.open(),
            Event::MenuCommand(c) if (FEED_BASE..FEED_BASE + FEEDS.len() as u32).contains(c) => {
                let feed = FEEDS[(c - FEED_BASE) as usize].id.to_string();
                if feed == self.feed {
                    return Outcome::none();
                }
                self.feed = feed.clone();
                self.state = NewsState::Loading;
                self.index = 0;
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged, Effect::News(Some(feed))] }
            }
            Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            _ => Outcome::none(),
        }
    }

    fn set_now(&mut self, now_ms: u64) {
        self.now = now_ms;
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        Some(view_for(ctx.shape, &self.state, self.index))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        let mut items = vec![
            MenuItem { id: OPEN_CMD, label: "Abrir manchete no navegador".into(), checked: false, group: None },
            MenuItem { id: NEXT_CMD, label: "Pr\u{f3}xima manchete".into(), checked: false, group: None },
        ];
        for (i, f) in FEEDS.iter().enumerate() {
            items.push(MenuItem { id: FEED_BASE + i as u32, label: f.name.to_string(), checked: f.id == self.feed, group: Some("Fonte") });
        }
        items
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&Saved { feed: self.feed.clone() }).unwrap_or_default() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        self.state = NewsState::Loading; // esquece as manchetes
        vec![Effect::News(None)]
    }
}

impl News {
    fn open(&self) -> Outcome {
        if self.len() == 0 {
            return Outcome::none();
        }
        Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::OpenNews(self.index % self.len())] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::layout::{layout, Rect, TextMeasure};

    struct Fake;
    impl TextMeasure for Fake {
        fn measure(&self, text: &str, style: TextStyle, max_w: Option<f32>, _l: u8) -> (f32, f32) {
            let w = text.chars().count() as f32 * style.size_dip() * 0.55;
            (w.min(max_w.unwrap_or(w)), style.line_dip())
        }
    }

    pub(crate) fn demo() -> NewsState {
        let h = |t: &str, age| Headline { title: t.into(), link: "https://exemplo.com/a".into(), age_secs: age };
        NewsState::Ready {
            source: "BBC Brasil".into(),
            items: vec![
                h("Governo anuncia pacote de medidas para conter a alta dos alimentos nos pr\u{f3}ximos meses", Some(1_800)),
                h("Cientistas descobrem novo planeta potencialmente habit\u{e1}vel", Some(7_300)),
                h("Sem data de publica\u{e7}\u{e3}o neste item", None),
            ],
        }
    }

    fn started() -> Box<dyn Widget> {
        let mut w = create(None).unwrap();
        w.handle(&Event::Started { now_ms: 1_000 });
        w
    }

    #[test]
    fn subscribes_to_its_feed_and_stop_unsubscribes_and_forgets() {
        let mut w = create(None).unwrap();
        assert_eq!(
            w.handle(&Event::Started { now_ms: 0 }).effects,
            vec![Effect::News(Some(DEFAULT_FEED.to_string()))]
        );
        w.handle(&Event::News(demo()));
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::News(None)]);
        assert!(w.stop(StopReason::Removed).is_empty());
        assert_eq!(w.handle(&Event::News(demo())), Outcome::none());
    }

    #[test]
    fn headlines_rotate_by_timer_and_manual_navigation_restarts_the_clock() {
        let mut w = started();
        let first = w.handle(&Event::News(demo()));
        assert_eq!(first.effects, vec![Effect::ScheduleAt { deadline_ms: 13_000, token: ROTATE_TOKEN }]);
        let tick = w.handle(&Event::Timer { token: ROTATE_TOKEN, now_ms: 13_000 });
        assert_eq!(tick.invalidate, Invalidation::CONTENT);
        assert_eq!(tick.effects, vec![Effect::ScheduleAt { deadline_ms: 25_000, token: ROTATE_TOKEN }]);
        let next = w.handle(&Event::Clicked { button: NEXT });
        assert_eq!(next.effects.len(), 2, "cancela e reagenda");
        assert!(matches!(next.effects[0], Effect::CancelTimer { token: ROTATE_TOKEN }));
        // Voltar a partir da primeira da volta (indice 2 -> 1 -> 0 -> 2).
        let mut w2 = started();
        w2.handle(&Event::News(demo()));
        w2.handle(&Event::Clicked { button: PREV });
        let tile = w2.tile_view(&ViewCtx { shape: Shape::S2X2, time: pod_model::time::TimeInfo { year: 2026, month: 10, day: 18, weekday: pod_model::time::weekday_of(2026, 10, 18), hour: 0, minute: 0, second: 0, millis: 0 } }).unwrap();
        fn texts(n: &Node, out: &mut Vec<String>) {
            if let Kind::Text { text, .. } = &n.kind {
                out.push(text.clone());
            }
            n.children().iter().for_each(|c| texts(c, out));
        }
        let mut t = Vec::new();
        texts(&tile.root, &mut t);
        assert!(t.iter().any(|s| s.contains("Sem data")), "<| da primeira volta para a ultima: {t:?}");
    }

    #[test]
    fn a_single_headline_does_not_rotate_and_loss_of_data_stops_the_timer() {
        let mut w = started();
        let one = NewsState::Ready { source: "X".into(), items: vec![Headline { title: "So uma".into(), link: "https://x.com".into(), age_secs: None }] };
        assert_eq!(w.handle(&Event::News(one)).effects, vec![]);
        w.handle(&Event::News(demo()));
        let lost = w.handle(&Event::News(NewsState::Failed("Sem conex\u{e3}o".into())));
        assert!(lost.effects.iter().any(|e| matches!(e, Effect::CancelTimer { .. })), "{lost:?}");
        assert_eq!(w.handle(&Event::Timer { token: ROTATE_TOKEN, now_ms: 99_000 }), Outcome::none());
    }

    #[test]
    fn open_asks_for_the_current_index_only_and_needs_headlines() {
        let mut w = started();
        assert_eq!(w.handle(&Event::Clicked { button: OPEN }), Outcome::none(), "sem manchetes nada a abrir");
        w.handle(&Event::News(demo()));
        assert_eq!(w.handle(&Event::Clicked { button: OPEN }).effects, vec![Effect::OpenNews(0)]);
        w.handle(&Event::Clicked { button: NEXT });
        assert_eq!(w.handle(&Event::MenuCommand(OPEN_CMD)).effects, vec![Effect::OpenNews(1)]);
    }

    #[test]
    fn choosing_a_source_persists_and_resubscribes() {
        let mut w = started();
        let id = FEEDS.iter().position(|f| f.id == "hacker-news").unwrap() as u32;
        let o = w.handle(&Event::MenuCommand(FEED_BASE + id));
        assert_eq!(o.effects, vec![Effect::StateChanged, Effect::News(Some("hacker-news".into()))]);
        assert_eq!(w.snapshot().data, "{\"feed\":\"hacker-news\"}");
        assert_eq!(w.handle(&Event::MenuCommand(FEED_BASE + id)), Outcome::none(), "mesma fonte");
        let m = w.menu_items();
        assert_eq!(m.iter().filter(|i| i.checked).count(), 1);
        assert_eq!(m.len(), 2 + FEEDS.len());
        let again = create(Some(&w.snapshot())).unwrap();
        assert_eq!(again.snapshot().data, w.snapshot().data);
    }

    #[test]
    fn unknown_or_broken_saved_state_is_handled() {
        let w = create(Some(&Snapshot { version: 1, data: "{\"feed\":\"fonte-removida\"}".into() })).unwrap();
        assert_eq!(w.snapshot().data, format!("{{\"feed\":\"{DEFAULT_FEED}\"}}"), "cai no padrao");
        assert!(matches!(create(Some(&Snapshot { version: 1, data: "{".into() })), Err(WidgetError::InvalidState(_))));
        assert!(matches!(create(Some(&Snapshot { version: 2, data: String::new() })), Err(WidgetError::UnsupportedVersion { .. })));
    }

    #[test]
    fn every_state_and_shape_fits_the_tile() {
        let long = NewsState::Ready {
            source: "Fonte com nome comprido".into(),
            items: vec![Headline { title: "palavra".repeat(40), link: "https://x.com".into(), age_secs: Some(90_000) }; 3],
        };
        for s in [NewsState::Off, NewsState::Loading, NewsState::Failed("Sem conex\u{e3}o".into()), demo(), long] {
            for shape in descriptor().shapes {
                for index in [0, 1, 5] {
                    let v = view_for(shape, &s, index);
                    assert_eq!(v.validate(), Ok(()), "{shape}");
                    let size = shape.size_dip();
                    let laid = layout(&v, Rect::new(0.0, 0.0, size.w, size.h), &Fake);
                    for l in &laid {
                        assert!(laid[0].rect.contains(&l.rect), "{shape} {s:?}: {:?}", l.rect);
                    }
                }
            }
        }
    }
}
