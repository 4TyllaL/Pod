//! Sistema: CPU, memória e disco em anéis concêntricos (1×1, 2×1, 2×2). Assina as métricas enquanto
//! visível; o estado é só o último valor mostrado (não é durável).

use pod_model::ids::WidgetTypeId;
use pod_model::metrics::{percent, MetricsNeeds, Sample};
use pod_model::shape::Shape;
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, Outcome, SizeDip, Snapshot, StopReason, ViewCtx, Widget,
    WidgetError,
};

use super::tkit::chip_header;

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.overview");
pub const STATE_VERSION: u32 = 1;

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X1.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2],
        default_shape: Some(Shape::S2X1),
    }
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(Overview { shown: Shown::default(), stopped: false }))
}

/// Percentuais inteiros (o anel só precisa de 1% de resolução, então valores iguais não geram quadro).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Shown {
    cpu: Option<u8>,
    memory: Option<u8>,
    disk: Option<u8>,
}

fn used(pair: Option<(u64, u64)>) -> Option<u8> {
    pair.filter(|(_, t)| *t > 0).map(|(u, t)| percent(u as f32 / t as f32).min(100) as u8)
}

fn shown_for(s: &Sample) -> Shown {
    Shown { cpu: s.cpu.map(|v| percent(v).min(100) as u8), memory: used(s.memory), disk: used(s.disk) }
}

struct Overview {
    shown: Shown,
    stopped: bool,
}

fn pct(v: Option<u8>) -> String {
    v.map_or_else(|| "\u{2014}".to_string(), |p| format!("{p}%"))
}

fn text(s: String, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s, style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

impl Overview {
    /// Do anel externo ao interno: disco, memória, CPU.
    fn entries(&self) -> [(&'static str, Option<u8>, Role); 3] {
        [("Disco", self.shown.disk, Role::Red), ("Mem\u{f3}ria", self.shown.memory, Role::Indigo), ("CPU", self.shown.cpu, Role::Blue)]
    }

    fn rings(&self, size: f32, thickness: f32) -> Node {
        let rings = self.entries().iter().map(|(_, v, r)| (v.unwrap_or(0) as f32 / 100.0, *r)).collect();
        let label = format!(
            "Disco {}, mem\u{f3}ria {}, CPU {}",
            pct(self.shown.disk),
            pct(self.shown.memory),
            pct(self.shown.cpu)
        );
        Node::rings(rings, size, thickness, label)
    }
}

impl Widget for Overview {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => Outcome {
                invalidate: Invalidation::CONTENT,
                effects: vec![Effect::Metrics(Some(MetricsNeeds::CPU.union(MetricsNeeds::MEMORY).union(MetricsNeeds::DISK)))],
            },
            Event::Metrics(sample) => {
                let next = shown_for(sample);
                if next == self.shown {
                    return Outcome::none();
                }
                self.shown = next;
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
        let root = match ctx.shape {
            s if s == Shape::S1X1 => Node::column(0.0, 8.0, vec![self.rings(72.0, 8.0)]).centered(),
            s if s == Shape::S2X2 => Node::column(6.0, 14.0, vec![
                chip_header('\u{25a3}', "Sistema", true),
                self.rings(84.0, 10.0),
                Node::row(6.0, 0.0, self.entries().iter().map(|(name, v, role)| {
                    Node::column(1.0, 0.0, vec![
                        text(name.to_string(), TextStyle::Caption, *role, Align::Center),
                        text(pct(*v), TextStyle::Title, Role::Primary, Align::Center),
                    ])
                    .size(Len::Fill(1), Len::Auto)
                }).collect())
                .size(Len::Fill(1), Len::Auto),
            ])
            .centered(),
            _ => Node::row(14.0, 12.0, vec![
                self.rings(64.0, 7.0),
                Node::column(3.0, 0.0, self.entries().iter().map(|(name, v, role)| {
                    Node::row(6.0, 0.0, vec![
                        text(name.to_string(), TextStyle::Caption, *role, Align::Start),
                        Node::new(Kind::Text { text: pct(*v), style: TextStyle::Body, role: Role::Primary, align: Align::End, max_lines: 1 }),
                    ])
                    .size(Len::Fill(1), Len::Auto)
                }).collect())
                .size(Len::Fill(1), Len::Auto),
            ])
            .centered(),
        };
        Some(View::new(root.fill()))
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

    fn sample(cpu: f32, mem: (u64, u64), disk: (u64, u64)) -> Event {
        Event::Metrics(Sample { cpu: Some(cpu), memory: Some(mem), disk: Some(disk), ..Default::default() })
    }

    #[test]
    fn subscribes_to_cpu_memory_and_disk_and_unsubscribes_on_stop() {
        let mut w = create(None).unwrap();
        let out = w.handle(&Event::Started { now_ms: 0 });
        let needs = MetricsNeeds::CPU.union(MetricsNeeds::MEMORY).union(MetricsNeeds::DISK);
        assert_eq!(out.effects, vec![Effect::Metrics(Some(needs))]);
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::Metrics(None)]);
        assert!(w.stop(StopReason::Removed).is_empty());
    }

    #[test]
    fn view_changes_only_when_a_percent_changes() {
        let mut w = create(None).unwrap();
        assert_ne!(w.handle(&sample(0.341, (50, 100), (93, 100))), Outcome::none());
        assert_eq!(w.handle(&sample(0.3449, (50, 100), (93, 100))), Outcome::none(), "34% -> 34%");
        assert_ne!(w.handle(&sample(0.36, (50, 100), (93, 100))), Outcome::none());
    }

    #[test]
    fn every_shape_gives_a_valid_view_that_fits_the_tile() {
        let mut w = create(None).unwrap();
        w.handle(&sample(0.34, (61, 100), (93, 100)));
        for shape in descriptor().shapes {
            let v = w.tile_view(&ctx(shape)).expect("cartão");
            assert_eq!(v.validate(), Ok(()), "{shape}");
            let s = shape.size_dip();
            let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
            let whole = laid[0].rect;
            for l in &laid {
                assert!(whole.contains(&l.rect), "{shape}: {:?} fora do cartão", l.rect);
            }
        }
    }

    #[test]
    fn rings_are_announced_with_the_three_values() {
        let mut w = create(None).unwrap();
        w.handle(&sample(0.34, (61, 100), (93, 100)));
        let v = w.tile_view(&ctx(Shape::S2X1)).unwrap();
        let laid = layout(&v, Rect::new(0.0, 0.0, 188.0, 88.0), &Fake);
        let items = pod_model::a11y::items(&v, &laid);
        assert!(items.iter().any(|i| i.name == "Disco 93%, mem\u{f3}ria 61%, CPU 34%"), "{items:?}");
    }
}
