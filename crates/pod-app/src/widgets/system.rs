//! Widgets de métricas do sistema: CPU, memória, disco e bateria (1×1, 2×1, 2×2). Assinam
//! `SystemMetrics` enquanto visíveis; o estado é só o último valor mostrado (não é durável).

use pod_model::ids::WidgetTypeId;
use pod_model::metrics::{format_bytes, format_minutes, percent, Battery, MetricsNeeds, PowerSource, Sample};
use pod_model::shape::Shape;
use pod_model::view::{Role, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, Outcome, SizeDip, Snapshot, StopReason, ViewCtx, Widget,
    WidgetError,
};

use super::gauge::{gauge_view, Gauge};

pub const CPU: WidgetTypeId = WidgetTypeId("pod.cpu");
pub const MEMORY: WidgetTypeId = WidgetTypeId("pod.memory");
pub const DISK: WidgetTypeId = WidgetTypeId("pod.disk");
pub const BATTERY: WidgetTypeId = WidgetTypeId("pod.battery");
pub const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Cpu,
    Memory,
    Disk,
    Battery,
}

impl Kind {
    fn type_id(self) -> WidgetTypeId {
        match self {
            Kind::Cpu => CPU,
            Kind::Memory => MEMORY,
            Kind::Disk => DISK,
            Kind::Battery => BATTERY,
        }
    }
    fn needs(self) -> MetricsNeeds {
        match self {
            Kind::Cpu => MetricsNeeds::CPU,
            Kind::Memory => MetricsNeeds::MEMORY,
            Kind::Disk => MetricsNeeds::DISK,
            Kind::Battery => MetricsNeeds::BATTERY,
        }
    }
    fn icon(self) -> char {
        match self {
            Kind::Battery => '\u{1f50b}',
            _ => '\u{25a3}',
        }
    }
    fn short(self) -> &'static str {
        match self {
            Kind::Cpu => "CPU",
            other => other.title(),
        }
    }
    fn title(self) -> &'static str {
        match self {
            Kind::Cpu => "CPU",
            Kind::Memory => "Mem\u{f3}ria",
            Kind::Disk => "Disco",
            Kind::Battery => "Bateria",
        }
    }
}

pub fn descriptor(kind: Kind) -> Descriptor {
    Descriptor {
        type_id: kind.type_id(),
        state_version: STATE_VERSION,
        default_size: Shape::S2X1.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S4X1, Shape::S4X2],
        default_shape: Some(Shape::S2X1),
    }
}

fn create_kind(kind: Kind, restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(Metric { kind, shown: None, stopped: false }))
}

pub fn create_cpu(r: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    create_kind(Kind::Cpu, r)
}
pub fn create_memory(r: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    create_kind(Kind::Memory, r)
}
pub fn create_disk(r: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    create_kind(Kind::Disk, r)
}
pub fn create_battery(r: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    create_kind(Kind::Battery, r)
}

/// O que o cartão mostra. Só muda de valor quando o texto ou o anel mudariam (anel em 1%).
#[derive(Debug, Clone, PartialEq)]
struct Shown {
    value: Option<f32>,
    center: String,
    detail: String,
    role: Role,
}

/// Faixa de alerta por uso: acento normal, aviso a partir de 80%, perigo a partir de 95%.
fn usage_role(v: f32) -> Role {
    if v >= 0.95 {
        Role::Danger
    } else if v >= 0.80 {
        Role::Warning
    } else {
        Role::Accent
    }
}

fn used_of(pair: Option<(u64, u64)>) -> Option<f32> {
    pair.filter(|(_, t)| *t > 0).map(|(u, t)| u as f32 / t as f32)
}

fn battery_shown(b: &Battery) -> Shown {
    let value = b.percent.map(|p| p as f32 / 100.0);
    let (detail, role) = match (b.source, b.percent) {
        (PowerSource::Charging, _) => ("Carregando".to_string(), Role::Positive),
        (PowerSource::PluggedIn, _) => ("Na tomada".to_string(), if value.is_some() { Role::Positive } else { Role::Accent }),
        (PowerSource::Battery, p) => {
            let detail = match b.minutes_left {
                Some(m) => format_minutes(m),
                None => "Descarregando".to_string(),
            };
            let role = match p {
                Some(p) if p <= 10 => Role::Danger,
                Some(p) if p <= 20 => Role::Warning,
                _ => Role::Accent,
            };
            (detail, role)
        }
    };
    Shown { value, center: b.percent.map_or_else(|| "\u{2014}".into(), |p| format!("{p}%")), detail, role }
}

fn shown_for(kind: Kind, s: &Sample) -> Option<Shown> {
    match kind {
        Kind::Cpu => s.cpu.map(|v| Shown {
            value: Some(v),
            center: format!("{}%", percent(v)),
            detail: "uso total".into(),
            role: usage_role(v),
        }),
        Kind::Memory => used_of(s.memory).zip(s.memory).map(|(v, (u, t))| Shown {
            value: Some(v),
            center: format!("{}%", percent(v)),
            detail: format!("{} / {}", format_bytes(u), format_bytes(t)),
            role: usage_role(v),
        }),
        Kind::Disk => used_of(s.disk).zip(s.disk).map(|(v, (u, t))| Shown {
            value: Some(v),
            center: format!("{}%", percent(v)),
            detail: format!("{} livres", format_bytes(t - u)),
            role: usage_role(v),
        }),
        Kind::Battery => s.battery.as_ref().map(battery_shown),
    }
    // O anel só precisa de 1% de resolução: valores quase iguais não geram quadro novo.
    .map(|mut sh| {
        sh.value = sh.value.map(|v| percent(v) as f32 / 100.0);
        sh
    })
}

struct Metric {
    kind: Kind,
    shown: Option<Shown>,
    stopped: bool,
}

impl Widget for Metric {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => {
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::Metrics(Some(self.kind.needs()))] }
            }
            Event::Metrics(sample) => {
                let next = shown_for(self.kind, sample);
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
        let g = match &self.shown {
            Some(s) => Gauge {
                title: self.kind.title().into(),
                short: self.kind.short().into(),
                value: s.value,
                center: s.center.clone(),
                detail: s.detail.clone(),
                role: s.role,
                icon: self.kind.icon(),
            },
            None => Gauge {
                title: self.kind.title().into(),
                short: self.kind.short().into(),
                value: None,
                center: String::new(),
                detail: "lendo\u{2026}".into(),
                role: Role::Accent,
                icon: self.kind.icon(),
            },
        };
        Some(gauge_view(ctx.shape, &g))
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
    use pod_model::view::TextStyle;

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

    fn make(kind: Kind) -> Box<dyn Widget> {
        create_kind(kind, None).unwrap()
    }

    fn sample() -> Sample {
        Sample {
            cpu: Some(0.37),
            memory: Some((9_000_000_000, 17_179_869_184)),
            disk: Some((300 * 1024 * 1024 * 1024, 512 * 1024 * 1024 * 1024)),
            battery: Some(Battery::from_status(0, 1, 63, 7_800, false)),
            net: None,
        }
    }

    #[test]
    fn each_kind_subscribes_to_exactly_what_it_shows_and_stop_unsubscribes() {
        for (kind, needs) in [
            (Kind::Cpu, MetricsNeeds::CPU),
            (Kind::Memory, MetricsNeeds::MEMORY),
            (Kind::Disk, MetricsNeeds::DISK),
            (Kind::Battery, MetricsNeeds::BATTERY),
        ] {
            let mut w = make(kind);
            assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::Metrics(Some(needs))]);
            assert_eq!(w.stop(StopReason::Removed), vec![Effect::Metrics(None)]);
            assert!(w.stop(StopReason::Removed).is_empty());
            assert_eq!(w.handle(&Event::Metrics(sample())), Outcome::none(), "parado ignora eventos");
        }
    }

    #[test]
    fn only_a_visible_change_invalidates() {
        let mut w = make(Kind::Cpu);
        let first = w.handle(&Event::Metrics(sample()));
        assert_eq!(first.invalidate, Invalidation::CONTENT);
        // Mesmo percentual inteiro (37,0% e 37,9%): nada a redesenhar.
        let near = Sample { cpu: Some(0.379), ..sample() };
        assert_eq!(w.handle(&Event::Metrics(near)), Outcome::none());
        let moved = Sample { cpu: Some(0.38), ..sample() };
        assert_eq!(w.handle(&Event::Metrics(moved)).invalidate, Invalidation::CONTENT);
        // Outras métricas do mesmo Sample não afetam o cartão de CPU.
        let other = Sample { memory: None, ..moved };
        assert_eq!(w.handle(&Event::Metrics(other)), Outcome::none());
    }

    #[test]
    fn texts_in_portuguese() {
        let s = sample();
        let mem = shown_for(Kind::Memory, &s).unwrap();
        assert_eq!((mem.center.as_str(), mem.detail.as_str()), ("52%", "8,4 GB / 16,0 GB"));
        let disk = shown_for(Kind::Disk, &s).unwrap();
        assert_eq!((disk.center.as_str(), disk.detail.as_str()), ("58%", "212,0 GB livres"));
        let bat = shown_for(Kind::Battery, &s).unwrap();
        assert_eq!((bat.center.as_str(), bat.detail.as_str(), bat.role), ("63%", "2 h 10 min", Role::Accent));
        assert_eq!(shown_for(Kind::Cpu, &s).unwrap().detail, "uso total");
    }

    #[test]
    fn alert_roles_follow_usage_and_battery_level() {
        assert_eq!(usage_role(0.79), Role::Accent);
        assert_eq!(usage_role(0.80), Role::Warning);
        assert_eq!(usage_role(0.95), Role::Danger);
        let low = |p| battery_shown(&Battery::from_status(0, 1, p, 600, false)).role;
        assert_eq!((low(50), low(20), low(10)), (Role::Accent, Role::Warning, Role::Danger));
        let charging = battery_shown(&Battery::from_status(1, 9, 30, u32::MAX, false));
        assert_eq!((charging.detail.as_str(), charging.role), ("Carregando", Role::Positive));
        let desktop = battery_shown(&Battery::from_status(1, 128, 255, u32::MAX, false));
        assert_eq!((desktop.value, desktop.center.as_str(), desktop.detail.as_str()), (None, "\u{2014}", "Na tomada"));
    }

    #[test]
    fn missing_readings_show_a_placeholder_not_a_wrong_value() {
        let mut w = make(Kind::Memory);
        assert!(w.tile_view(&ctx(Shape::S2X1)).unwrap().validate().is_ok());
        // Leitura que falhou (None) depois de uma boa limpa o cartão em vez de manter o valor velho.
        w.handle(&Event::Metrics(sample()));
        let out = w.handle(&Event::Metrics(Sample::default()));
        assert_eq!(out.invalidate, Invalidation::CONTENT);
    }

    #[test]
    fn every_kind_and_shape_fits_the_tile() {
        for kind in [Kind::Cpu, Kind::Memory, Kind::Disk, Kind::Battery] {
            let mut w = make(kind);
            for with_data in [false, true] {
                if with_data {
                    w.handle(&Event::Metrics(sample()));
                }
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
    fn future_state_is_refused_and_no_durable_state_is_kept() {
        assert!(matches!(
            create_kind(Kind::Cpu, Some(&Snapshot { version: 2, data: String::new() })),
            Err(WidgetError::UnsupportedVersion { .. })
        ));
        assert_eq!(make(Kind::Disk).snapshot().data, "");
    }
}
