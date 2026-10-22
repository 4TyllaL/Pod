//! Bolsa: quadro de cotações em estilo de painel (fundo escuro, verde na alta, vermelho na baixa, ▲▼).
//! Três fontes à escolha no menu (AwesomeAPI: moedas e cripto; Brapi: ações da B3; Yahoo: mercado
//! mundial) e os ativos de cada uma também. **Usa a internet**: só com "Widgets de internet" ligado.
//! Os números são de serviços públicos, com atraso, e não são recomendação de investimento.

use pod_model::ids::WidgetTypeId;
use pod_model::quotes::{format_pct, format_price, is_down, is_up, Quote, QuoteRequest, QuoteState, Source, MAX_SYMBOLS};
use pod_model::shape::Shape;
use pod_model::view::{Align, Fill, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, MenuItem, Outcome, SizeDip, Snapshot, StopReason, ViewCtx,
    Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.stocks");
pub const STATE_VERSION: u32 = 1;
const SOURCE_BASE: u32 = 10;
const ASSET_BASE: u32 = 100;
const BOARD_TOP: [u8; 3] = [14, 16, 22];
const BOARD_BOTTOM: [u8; 3] = [3, 4, 7];

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X2.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S2X3, Shape::S2X4, Shape::S4X1, Shape::S4X2],
        default_shape: Some(Shape::S2X2),
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    source: String,
    symbols: Vec<String>,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let default = || Saved { source: Source::Awesome.id().into(), symbols: Source::Awesome.default_symbols() };
    let saved = match restore {
        None => default(),
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) if s.data.is_empty() => default(),
        Some(s) => serde_json::from_str::<Saved>(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    // Fonte desconhecida volta ao padrão; ativos fora do catálogo da fonte somem.
    let source = Source::from_id(&saved.source).unwrap_or(Source::Awesome);
    let req = QuoteRequest::clean(source, &saved.symbols);
    Ok(Box::new(Stocks { source, symbols: req.symbols, state: QuoteState::Loading, stopped: false }))
}

struct Stocks {
    source: Source,
    symbols: Vec<String>,
    state: QuoteState,
    stopped: bool,
}

fn text(s: &str, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

fn tone(q: &Quote) -> Role {
    if is_up(q) {
        Role::Up
    } else if is_down(q) {
        Role::Down
    } else {
        Role::OnFill
    }
}

fn arrow(q: &Quote) -> &'static str {
    if is_up(q) {
        "\u{25b2}"
    } else if is_down(q) {
        "\u{25bc}"
    } else {
        "\u{2022}"
    }
}

/// Uma linha do quadro: nome, preço, seta e variação.
fn board_row(q: &Quote, style: TextStyle) -> Node {
    let r = tone(q);
    Node::row(3.0, 0.0, vec![
        text(&q.label, style, Role::OnFill, Align::Start),
        text(&format_price(q.price), style, r, Align::End).size(Len::Fixed(56.0), Len::Auto),
        text(arrow(q), TextStyle::Caption, r, Align::Center).size(Len::Fixed(12.0), Len::Auto),
        text(&format_pct(q.change_pct), TextStyle::Caption, r, Align::End).size(Len::Fixed(46.0), Len::Auto),
    ])
    .size(Len::Fill(1), Len::Auto)
}

/// Uma coluna do quadro (formato largo de uma linha).
fn board_column(q: &Quote) -> Node {
    let r = tone(q);
    Node::column(2.0, 0.0, vec![
        text(&q.label, TextStyle::Caption, Role::OnFill, Align::Center),
        text(&format_price(q.price), TextStyle::Body, r, Align::Center),
        Node::row(3.0, 0.0, vec![
            text(arrow(q), TextStyle::Caption, r, Align::End).size(Len::Fixed(16.0), Len::Auto),
            text(&format_pct(q.change_pct), TextStyle::Caption, r, Align::Start),
        ])
        .centered()
        .size(Len::Fill(1), Len::Auto),
    ])
    .size(Len::Fill(1), Len::Auto)
}

fn board(root: Node) -> View {
    let plate = Node::plate(Fill::Gradient { top: BOARD_TOP, bottom: BOARD_BOTTOM }, crate::render::TILE_RADIUS_DIP, 0.0, root.fill());
    View::new(plate.fill())
}

fn message(state: &QuoteState) -> (String, String) {
    match state {
        QuoteState::Off => ("Internet desligada".into(), "Ligue \"Widgets de internet\" no menu da bandeja.".into()),
        QuoteState::Loading => ("Buscando\u{2026}".into(), String::new()),
        QuoteState::Failed(m) => (m.clone(), "Tentarei de novo em 1 minuto.".into()),
        QuoteState::Ready { .. } => (String::new(), String::new()),
    }
}

impl Stocks {
    fn request(&self) -> QuoteRequest {
        QuoteRequest { source: self.source, symbols: self.symbols.clone() }
    }

    fn view_for(&self, shape: Shape) -> View {
        let QuoteState::Ready { source, quotes } = &self.state else {
            let (main, hint) = message(&self.state);
            let mut col = vec![text("BOLSA", TextStyle::Caption, Role::OnFill, Align::Center), text(&main, TextStyle::Body, Role::OnFill, Align::Center)];
            if !hint.is_empty() && shape != Shape::S1X1 && shape.rows > 1 {
                col.push(Node::new(Kind::Text { text: hint, style: TextStyle::Caption, role: Role::OnFill, align: Align::Center, max_lines: 3 }).size(Len::Fill(1), Len::Auto));
            }
            return board(Node::column(4.0, 12.0, col).centered());
        };
        let root = if shape == Shape::S1X1 {
            let q = &quotes[0];
            let r = tone(q);
            Node::column(2.0, 8.0, vec![
                text(&q.label, TextStyle::Caption, Role::OnFill, Align::Center),
                text(&format_price(q.price), TextStyle::Headline, r, Align::Center),
                Node::row(3.0, 0.0, vec![
                    text(arrow(q), TextStyle::Caption, r, Align::End).size(Len::Fixed(16.0), Len::Auto),
                    text(&format_pct(q.change_pct), TextStyle::Caption, r, Align::Start),
                ])
                .centered()
                .size(Len::Fill(1), Len::Auto),
            ])
            .centered()
        } else if shape.cols >= 3 && shape.rows == 1 {
            Node::row(6.0, 12.0, quotes.iter().take(4).map(board_column).collect()).centered()
        } else if shape.cols >= 3 {
            // Duas colunas de linhas, até 4 em cada.
            let (a, b) = quotes.split_at(quotes.len().min(4));
            let col = |qs: &[Quote]| Node::column(8.0, 0.0, qs.iter().map(|q| board_row(q, TextStyle::Caption)).collect()).size(Len::Fill(1), Len::Auto);
            let mut cols = vec![col(a)];
            if !b.is_empty() {
                cols.push(col(b));
            }
            Node::column(6.0, 12.0, vec![
                text(&format!("BOLSA \u{b7} {}", source.short().to_uppercase()), TextStyle::Caption, Role::OnFill, Align::Start),
                Node::row(18.0, 0.0, cols).size(Len::Fill(1), Len::Auto),
            ])
        } else if shape.rows == 1 {
            Node::column(5.0, 10.0, quotes.iter().take(2).map(|q| board_row(q, TextStyle::Body)).collect()).centered().size(Len::Fill(1), Len::Fill(1))
        } else {
            let max = shape.rows as usize * 2; // 4 no 2x2, 6 no 2x3, 8 no 2x4
            let mut kids = vec![text(&format!("BOLSA \u{b7} {}", source.short().to_uppercase()), TextStyle::Caption, Role::OnFill, Align::Start)];
            kids.extend(quotes.iter().take(max.min(MAX_SYMBOLS)).map(|q| board_row(q, TextStyle::Body)));
            Node::column(7.0, 10.0, kids)
        };
        board(root)
    }

    fn changed(&mut self) -> Outcome {
        self.state = QuoteState::Loading;
        Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged, Effect::Quotes(Some(self.request()))] }
    }
}

impl Widget for Stocks {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::Quotes(Some(self.request()))] },
            Event::Quotes(s) => {
                if *s == self.state {
                    return Outcome::none();
                }
                self.state = s.clone();
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![] }
            }
            Event::MenuCommand(c) if (SOURCE_BASE..SOURCE_BASE + 3).contains(c) => {
                let source = Source::ALL[(c - SOURCE_BASE) as usize];
                if source == self.source {
                    return Outcome::none();
                }
                self.source = source;
                self.symbols = source.default_symbols();
                self.changed()
            }
            Event::MenuCommand(c) if *c >= ASSET_BASE && ((*c - ASSET_BASE) as usize) < self.source.assets().len() => {
                let sym = self.source.assets()[(*c - ASSET_BASE) as usize].0.to_string();
                if let Some(i) = self.symbols.iter().position(|s| *s == sym) {
                    if self.symbols.len() == 1 {
                        return Outcome::none(); // sempre fica pelo menos um
                    }
                    self.symbols.remove(i);
                } else if self.symbols.len() < MAX_SYMBOLS {
                    self.symbols.push(sym);
                } else {
                    return Outcome::none();
                }
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
        Some(self.view_for(ctx.shape))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        let mut m: Vec<MenuItem> = Source::ALL
            .iter()
            .enumerate()
            .map(|(i, s)| MenuItem { id: SOURCE_BASE + i as u32, label: s.label().into(), checked: *s == self.source, group: Some("Fonte") })
            .collect();
        for (i, a) in self.source.assets().iter().enumerate() {
            m.push(MenuItem { id: ASSET_BASE + i as u32, label: a.2.into(), checked: self.symbols.iter().any(|s| s == a.0), group: Some("Ativos") });
        }
        m
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&Saved { source: self.source.id().into(), symbols: self.symbols.clone() }).unwrap_or_default() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        self.state = QuoteState::Loading; // esquece as cotações
        vec![Effect::Quotes(None)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::time::{weekday_of, TimeInfo};

    fn ctx(shape: Shape) -> ViewCtx {
        ViewCtx { shape, time: TimeInfo { year: 2026, month: 10, day: 19, weekday: weekday_of(2026, 10, 19), hour: 9, minute: 0, second: 0, millis: 0 } }
    }

    fn q(label: &str, price: f64, pct: f64) -> Quote {
        Quote { label: label.into(), price, change_pct: pct }
    }

    pub(crate) fn demo(source: Source) -> QuoteState {
        QuoteState::Ready {
            source,
            quotes: vec![q("D\u{f3}lar", 5.04, -0.48), q("Euro", 5.65, -0.84), q("BTC", 413_798.0, 0.07), q("Libra", 6.71, 0.0), q("Iene", 0.034, 1.2), q("Peso", 0.0036, -0.2)],
        }
    }

    fn texts(v: &View) -> Vec<String> {
        fn walk(n: &Node, out: &mut Vec<String>) {
            if let Kind::Text { text, .. } = &n.kind {
                out.push(text.clone());
            }
            n.children().iter().for_each(|c| walk(c, out));
        }
        let mut t = Vec::new();
        walk(&v.root, &mut t);
        t
    }

    #[test]
    fn starting_subscribes_with_the_saved_request_and_stopping_cancels_and_forgets() {
        let mut w = create(None).unwrap();
        let o = w.handle(&Event::Started { now_ms: 0 });
        match o.effects.as_slice() {
            [Effect::Quotes(Some(r))] => assert_eq!((r.source, r.symbols.len()), (Source::Awesome, 4)),
            other => panic!("{other:?}"),
        }
        w.handle(&Event::Quotes(demo(Source::Awesome)));
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::Quotes(None)]);
        assert!(w.stop(StopReason::Removed).is_empty());
    }

    #[test]
    fn the_menu_switches_source_and_toggles_assets_with_limits() {
        let mut w = create(None).unwrap();
        let o = w.handle(&Event::MenuCommand(SOURCE_BASE + 2));
        assert!(matches!(o.effects.as_slice(), [Effect::StateChanged, Effect::Quotes(Some(r))] if r.source == Source::Yahoo && r.symbols == Source::Yahoo.default_symbols()));
        assert_eq!(w.handle(&Event::MenuCommand(SOURCE_BASE + 2)), Outcome::none(), "mesma fonte n\u{e3}o refaz o pedido");
        // Liga o 8\u{ba} ativo e recusa o 9\u{ba}; desliga at\u{e9} restar um.
        for i in 0..Source::Yahoo.assets().len() {
            w.handle(&Event::MenuCommand(ASSET_BASE + i as u32));
        }
        let snap: Saved = serde_json::from_str(&w.snapshot().data).unwrap();
        assert!(snap.symbols.len() <= MAX_SYMBOLS && snap.source == "yahoo");
        for i in 0..Source::Yahoo.assets().len() {
            w.handle(&Event::MenuCommand(ASSET_BASE + i as u32));
        }
        let snap: Saved = serde_json::from_str(&w.snapshot().data).unwrap();
        assert!(!snap.symbols.is_empty(), "sempre fica pelo menos um ativo");
        let items = w.menu_items();
        assert_eq!(items.iter().filter(|m| m.group == Some("Fonte") && m.checked).count(), 1);
        assert_eq!(items.iter().filter(|m| m.group == Some("Ativos")).count(), Source::Yahoo.assets().len());
    }

    #[test]
    fn state_roundtrips_and_bad_or_foreign_state_is_handled() {
        let w = create(None).unwrap();
        assert_eq!(create(Some(&w.snapshot())).unwrap().snapshot().data, w.snapshot().data);
        let odd = create(Some(&Snapshot { version: 1, data: r#"{"source":"outra","symbols":["PETR4","USD-BRL"]}"#.into() })).unwrap();
        let s: Saved = serde_json::from_str(&odd.snapshot().data).unwrap();
        assert_eq!(s.source, "awesome");
        assert_eq!(s.symbols, vec!["USD-BRL"], "s\u{ed}mbolo de outra fonte some");
        assert!(create(Some(&Snapshot { version: 1, data: "{".into() })).is_err());
        assert!(create(Some(&Snapshot { version: 2, data: String::new() })).is_err());
    }

    #[test]
    fn the_board_shows_prices_colors_and_arrows() {
        let mut w = create(None).unwrap();
        w.handle(&Event::Quotes(demo(Source::Awesome)));
        let t = texts(&w.tile_view(&ctx(Shape::S2X2)).unwrap());
        for want in ["D\u{f3}lar", "5,04", "\u{2212}0,48%", "\u{25bc}", "413.798", "+0,07%", "\u{25b2}"] {
            assert!(t.contains(&want.to_string()), "falta {want}: {t:?}");
        }
        assert!(t.iter().any(|s| s.starts_with("BOLSA")));
        let one = texts(&w.tile_view(&ctx(Shape::S1X1)).unwrap());
        assert!(one.contains(&"5,04".to_string()));
        assert_eq!(w.handle(&Event::Quotes(demo(Source::Awesome))), Outcome::none(), "sem mudan\u{e7}a n\u{e3}o repinta");
    }

    #[test]
    fn every_state_and_shape_is_valid() {
        let mut w = create(None).unwrap();
        for st in [QuoteState::Off, QuoteState::Loading, QuoteState::Failed("Sem cota\u{e7}\u{f5}es".into()), demo(Source::Awesome)] {
            w.handle(&Event::Quotes(st.clone()));
            for shape in descriptor().shapes {
                let v = w.tile_view(&ctx(shape)).unwrap();
                assert!(v.validate().is_ok(), "{st:?} {shape}");
            }
        }
    }
}
