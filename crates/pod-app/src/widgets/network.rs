//! Widget Rede: velocidade de download e upload com gráfico dos últimos ~2 minutos (1×1, 2×1, 2×2).
//! Assina `SystemMetrics` (rede) enquanto visível; o histórico é só da sessão (não é durável).

use pod_model::ids::WidgetTypeId;
use pod_model::metrics::{format_rate, MetricsNeeds, NetRate};
use pod_model::shape::Shape;
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, Outcome, SizeDip, Snapshot, StopReason, ViewCtx, Widget,
    WidgetError,
};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.network");
pub const STATE_VERSION: u32 = 1;
/// 60 leituras a cada 2 s = 2 minutos.
const HISTORY: usize = 60;
/// Piso da escala do gráfico (100 KB/s): ruído de fundo não vira montanha.
const MIN_SCALE: f32 = 100.0 * 1024.0;

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
    Ok(Box::new(Network { down: Vec::new(), last: None, stopped: false }))
}

struct Network {
    /// Download em bytes/s, do mais antigo ao mais novo.
    down: Vec<f32>,
    last: Option<NetRate>,
    stopped: bool,
}

/// Mostra a taxa em passos que o olho percebe: sem isso, flutuações de bytes redesenhariam tudo.
fn rounded(v: f32) -> f32 {
    (v / 1024.0).round() * 1024.0
}

fn text(s: String, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s, style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

fn chart(history: &[f32]) -> Node {
    let top = history.iter().copied().fold(MIN_SCALE, f32::max);
    let label = match history.last() {
        Some(v) => format!("Download nos \u{fa}ltimos minutos, agora {}", format_rate(*v)),
        None => "Download, sem leituras".to_string(),
    };
    Node::chart(history.to_vec(), top, Role::Accent, label)
}

/// Visual puro: `down`/`up` são o último valor mostrado (`None` = ainda lendo).
fn network_view(shape: Shape, history: &[f32], last: Option<NetRate>) -> View {
    let down = last.map_or_else(|| "\u{2014}".to_string(), |r| format_rate(r.down));
    let up = last.map_or_else(|| "\u{2014}".to_string(), |r| format_rate(r.up));
    let root = match shape {
        s if s == Shape::S1X1 => Node::column(2.0, 6.0, vec![
            text(format!("\u{2193} {down}"), TextStyle::Caption, Role::Primary, Align::Center),
            text(format!("\u{2191} {up}"), TextStyle::Caption, Role::Secondary, Align::Center),
            text("REDE".into(), TextStyle::Caption, Role::Tertiary, Align::Center),
        ])
        .centered(),
        s if s == Shape::S2X2 => Node::column(8.0, 14.0, vec![
            super::tkit::chip_header('\u{21c5}', "Rede", true),
            Node::row(12.0, 0.0, vec![
                text(format!("\u{2193} {down}"), TextStyle::Body, Role::Primary, Align::Center),
                text(format!("\u{2191} {up}"), TextStyle::Body, Role::Secondary, Align::Center),
            ])
            .size(Len::Fill(1), Len::Auto),
            chart(history),
        ]),
        _ => Node::row(12.0, 12.0, vec![
            Node::column(4.0, 0.0, vec![
                super::tkit::chip_header('\u{21c5}', "Rede", false),
                text(format!("\u{2193} {down}"), TextStyle::Caption, Role::Primary, Align::Start),
                text(format!("\u{2191} {up}"), TextStyle::Caption, Role::Secondary, Align::Start),
            ])
            .size(Len::Fixed(100.0), Len::Auto),
            chart(history),
        ]),
    };
    View::new(root.fill())
}

impl Widget for Network {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => {
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::Metrics(Some(MetricsNeeds::NETWORK))] }
            }
            Event::Metrics(sample) => {
                // Sem taxa (primeira leitura ou contador que voltou) não entra no gráfico.
                let Some(rate) = sample.net else { return Outcome::none() };
                let rate = NetRate { down: rounded(rate.down), up: rounded(rate.up) };
                if self.down.len() == HISTORY {
                    self.down.remove(0);
                }
                self.down.push(rate.down);
                self.last = Some(rate);
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
        Some(network_view(ctx.shape, &self.down, self.last))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: String::new() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        vec![Effect::Metrics(None)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::layout::{layout, Rect, TextMeasure};
    use pod_model::metrics::Sample;
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

    fn sample(down: f32, up: f32) -> Event {
        Event::Metrics(Sample { net: Some(NetRate { down, up }), ..Default::default() })
    }

    #[test]
    fn subscribes_to_network_only_and_stop_unsubscribes() {
        let mut w = create(None).unwrap();
        assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::Metrics(Some(MetricsNeeds::NETWORK))]);
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::Metrics(None)]);
        assert!(w.stop(StopReason::Removed).is_empty());
        assert_eq!(w.handle(&sample(1.0, 1.0)), Outcome::none(), "parado ignora eventos");
    }

    #[test]
    fn samples_without_rate_are_ignored_and_history_is_bounded() {
        let mut w = Network { down: Vec::new(), last: None, stopped: false };
        let none = Event::Metrics(Sample::default());
        assert_eq!(w.handle(&none), Outcome::none());
        for i in 0..(HISTORY + 10) {
            assert_eq!(w.handle(&sample(i as f32 * 2048.0, 0.0)).invalidate, Invalidation::CONTENT);
        }
        assert_eq!(w.down.len(), HISTORY);
        assert_eq!(*w.down.last().unwrap(), (HISTORY + 9) as f32 * 2048.0);
        assert_eq!(w.down[0], 10.0 * 2048.0, "os mais antigos saem primeiro");
    }

    #[test]
    fn rates_are_shown_in_portuguese() {
        let v = network_view(Shape::S2X2, &[], Some(NetRate { down: 1_258_291.0, up: 340.0 * 1024.0 }));
        let mut found = Vec::new();
        fn walk(n: &Node, out: &mut Vec<String>) {
            if let Kind::Text { text, .. } = &n.kind {
                out.push(text.clone());
            }
            n.children().iter().for_each(|c| walk(c, out));
        }
        walk(&v.root, &mut found);
        assert!(found.contains(&"\u{2193} 1 MB/s".to_string()), "{found:?}");
        assert!(found.contains(&"\u{2191} 340 KB/s".to_string()), "{found:?}");
    }

    #[test]
    fn every_shape_fits_with_and_without_data() {
        let mut w = create(None).unwrap();
        for with_data in [false, true] {
            if with_data {
                for i in 0..30 {
                    w.handle(&sample(i as f32 * 50_000.0, 1_000.0));
                }
            }
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
