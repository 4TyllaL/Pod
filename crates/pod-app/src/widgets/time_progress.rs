//! Progresso do tempo: quanto do dia, da semana, do mês ou do ano já passou (1×1, 2×1, 2×2).

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::time::{days_in_month, days_in_year, Granularity, TimeInfo, WeekStart};
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};

use super::gauge::{gauge_view, Gauge};
use super::tkit::chip_header;
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, MenuItem, Outcome, SizeDip, Snapshot, StopReason,
    ViewCtx, Widget, WidgetError,
};
use serde::{Deserialize, Serialize};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.time-progress");
pub const STATE_VERSION: u32 = 1;
const WEEK_START: WeekStart = WeekStart::Sunday;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Period {
    Day,
    Week,
    Month,
    Year,
}

impl Period {
    const ALL: [Period; 4] = [Period::Day, Period::Week, Period::Month, Period::Year];

    fn title(self) -> &'static str {
        match self {
            Period::Day => "Dia",
            Period::Week => "Semana",
            Period::Month => "M\u{ea}s",
            Period::Year => "Ano",
        }
    }

    pub fn progress(self, t: &TimeInfo) -> f32 {
        match self {
            Period::Day => t.day_progress(),
            Period::Week => t.week_progress(WEEK_START),
            Period::Month => t.month_progress(),
            Period::Year => t.year_progress(),
        }
    }

    /// Linha de detalhe em pt-BR.
    pub fn detail(self, t: &TimeInfo) -> String {
        match self {
            Period::Day => {
                let left = ((1.0 - t.day_progress() as f64) * 86_400.0) as u32;
                format!("restam {} h {:02} min", left / 3600, left % 3600 / 60)
            }
            Period::Week => format!("dia {} de 7", (t.weekday + 7 - week_start_offset()) % 7 + 1),
            Period::Month => format!("dia {} de {}", t.day, days_in_month(t.year, t.month)),
            Period::Year => format!("dia {} de {}", t.day_of_year(), days_in_year(t.year)),
        }
    }
}

fn week_start_offset() -> u8 {
    match WEEK_START {
        WeekStart::Sunday => 0,
        WeekStart::Monday => 1,
    }
}

/// Percentual inteiro sem arredondar para cima: 99,9% ainda é 99%.
pub fn percent(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * 100.0).floor() as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Settings {
    period: Period,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let settings = match restore {
        None => Settings { period: Period::Day },
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) => serde_json::from_str(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    Ok(Box::new(TimeProgress { settings, stopped: false }))
}

struct TimeProgress {
    settings: Settings,
    stopped: bool,
}

impl Widget for TimeProgress {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => Outcome {
                invalidate: Invalidation::CONTENT,
                effects: vec![Effect::TimeTicks(Some(Granularity::Minute))],
            },
            Event::Time(_) | Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            Event::MenuCommand(id) => match Period::ALL.get((*id as usize).wrapping_sub(1)) {
                Some(p) if *p != self.settings.period => {
                    self.settings.period = *p;
                    Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged] }
                }
                _ => Outcome::none(),
            },
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        let p = self.settings.period;
        let v = p.progress(&ctx.time);
        if p == Period::Year && ctx.shape.cols >= 2 && ctx.shape.rows == 1 {
            // Faixa larga do ano: percentual e 12 segmentos, um por mês.
            let style = if ctx.shape.cols < 4 { TextStyle::Title } else { TextStyle::Headline };
            let root = Node::column(4.0, 12.0, vec![
                chip_header('\u{23f1}', &format!("Ano de {}", ctx.time.year), false),
                Node::row(8.0, 0.0, vec![
                    Node::text(format!("{}%", percent(v)), style, Role::Primary),
                    Node::new(Kind::Text { text: p.detail(&ctx.time), style: TextStyle::Caption, role: Role::Secondary, align: Align::End, max_lines: 1 })
                        .size(Len::Fill(1), Len::Auto),
                ])
                .size(Len::Fill(1), Len::Auto),
                Node::segments(v, 12, 6.0, Role::Orange),
            ]);
            return Some(View::new(root.fill()));
        }
        Some(gauge_view(
            ctx.shape,
            &Gauge {
                title: p.title().into(),
                short: String::new(),
                value: Some(v),
                center: format!("{}%", percent(v)),
                detail: p.detail(&ctx.time),
                role: Role::Accent,
                icon: '\u{23f1}',
            },
        ))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        Period::ALL
            .iter()
            .enumerate()
            .map(|(i, p)| MenuItem { id: i as u32 + 1, label: p.title().into(), checked: *p == self.settings.period, group: Some("Per\u{ed}odo") })
            .collect()
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&self.settings).unwrap_or_default() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        vec![Effect::TimeTicks(None)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::layout::{layout, Rect, TextMeasure};
    use pod_model::time::weekday_of;
    use pod_model::view::TextStyle;

    struct Fake;
    impl TextMeasure for Fake {
        fn measure(&self, text: &str, style: TextStyle, max_w: Option<f32>, _lines: u8) -> (f32, f32) {
            let w = text.chars().count() as f32 * style.size_dip() * 0.55;
            (w.min(max_w.unwrap_or(w)), style.line_dip())
        }
    }

    fn at(day: u8, h: u8) -> TimeInfo {
        TimeInfo { year: 2026, month: 10, day, weekday: weekday_of(2026, 10, day), hour: h, minute: 0, second: 0, millis: 0 }
    }

    fn make(period: Period) -> Box<dyn Widget> {
        create(Some(&Snapshot { version: 1, data: serde_json::to_string(&Settings { period }).unwrap() })).unwrap()
    }

    #[test]
    fn percent_never_rounds_up_to_the_next_value() {
        assert_eq!(percent(0.0), 0);
        assert_eq!(percent(0.999), 99);
        assert_eq!(percent(1.0), 100);
        assert_eq!(percent(0.4217), 42);
        assert_eq!(percent(-1.0), 0);
        assert_eq!(percent(2.0), 100);
    }

    #[test]
    fn details_are_in_portuguese() {
        let t = at(18, 18); // domingo, 18h
        assert_eq!(Period::Day.detail(&t), "restam 6 h 00 min");
        assert_eq!(Period::Week.detail(&t), "dia 1 de 7");
        assert_eq!(Period::Month.detail(&t), "dia 18 de 31");
        assert_eq!(Period::Year.detail(&t), "dia 291 de 365");
        assert_eq!(Period::Week.detail(&at(24, 0)), "dia 7 de 7", "sábado fecha a semana que começa no domingo");
    }

    #[test]
    fn every_shape_and_period_fits_the_tile() {
        for period in Period::ALL {
            let w = make(period);
            for shape in descriptor().shapes {
                let v = w.tile_view(&ViewCtx { shape, time: at(18, 18) }).unwrap();
                assert_eq!(v.validate(), Ok(()));
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{period:?} {shape}: {:?}", l.rect);
                }
            }
        }
    }

    #[test]
    fn menu_selects_the_period_and_persists_it() {
        let mut w = make(Period::Day);
        assert!(w.menu_items()[0].checked);
        let out = w.handle(&Event::MenuCommand(4)); // Ano
        assert!(out.effects.contains(&Effect::StateChanged));
        assert!(w.menu_items()[3].checked);
        assert!(w.snapshot().data.contains("year"));
        assert_eq!(w.handle(&Event::MenuCommand(4)), Outcome::none(), "mesmo período: nada muda");
        assert_eq!(w.handle(&Event::MenuCommand(0)), Outcome::none());
        assert_eq!(w.handle(&Event::MenuCommand(5)), Outcome::none());
    }

    #[test]
    fn the_view_follows_time_and_stop_unsubscribes() {
        let w = make(Period::Day);
        let a = w.tile_view(&ViewCtx { shape: Shape::S2X1, time: at(18, 6) });
        let b = w.tile_view(&ViewCtx { shape: Shape::S2X1, time: at(18, 7) });
        assert_ne!(a, b);
        let mut w = make(Period::Day);
        assert_eq!(w.stop(StopReason::Shutdown), vec![Effect::TimeTicks(None)]);
        assert!(w.stop(StopReason::Shutdown).is_empty());
    }

    #[test]
    fn future_or_broken_state_is_refused() {
        assert!(matches!(create(Some(&Snapshot { version: 2, data: "{}".into() })), Err(WidgetError::UnsupportedVersion { .. })));
        assert!(matches!(create(Some(&Snapshot { version: 1, data: "{\"period\":\"decade\"}".into() })), Err(WidgetError::InvalidState(_))));
    }
}
