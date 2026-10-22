//! Relógio mundial: a hora em até 4 cidades (escolhidas no menu), com "ontem/amanhã" quando o dia é
//! outro. O host converte os fusos (tabela do Windows, com horário de verão); o widget só mostra.

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::time::Granularity;
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, MenuItem, Outcome, SizeDip, Snapshot, StopReason, ViewCtx,
    Widget, WidgetError,
};
use pod_model::worldclock::{self, ZoneTime, CITIES, DEFAULT};
use serde::{Deserialize, Serialize};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.worldclock");
pub const STATE_VERSION: u32 = 1;
const CITY_BASE: u32 = 100;

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X2.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S4X1],
        default_shape: Some(Shape::S2X2),
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    cities: Vec<String>,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let default = || Saved { cities: DEFAULT.iter().map(|s| s.to_string()).collect() };
    let saved = match restore {
        None => default(),
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) if s.data.is_empty() => default(),
        Some(s) => serde_json::from_str::<Saved>(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    // Cidade que saiu do catálogo some; sem nenhuma, volta ao padrão.
    let mut cities: Vec<String> = Vec::new();
    for c in saved.cities {
        if worldclock::city(&c).is_some() && !cities.contains(&c) && cities.len() < worldclock::MAX_SELECTED {
            cities.push(c);
        }
    }
    if cities.is_empty() {
        cities = default().cities;
    }
    Ok(Box::new(WorldClock { cities, times: Vec::new(), stopped: false }))
}

struct WorldClock {
    cities: Vec<String>,
    /// Hora de cada cidade escolhida (na mesma ordem); `None` se o host não soube converter.
    times: Vec<Option<ZoneTime>>,
    stopped: bool,
}

fn text(s: &str, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

impl WorldClock {
    fn keys(&self) -> Vec<String> {
        self.cities.iter().filter_map(|c| worldclock::city(c)).map(|c| c.2.to_string()).collect()
    }

    fn entries(&self) -> Vec<(&'static str, Option<ZoneTime>)> {
        self.cities
            .iter()
            .enumerate()
            .filter_map(|(i, id)| worldclock::city(id).map(|c| (c.1, self.times.get(i).copied().flatten())))
            .collect()
    }

    fn view_for(&self, shape: Shape) -> View {
        let e = self.entries();
        let clock = |t: &Option<ZoneTime>| t.map_or("--:--".to_string(), |z| z.text());
        let day = |t: &Option<ZoneTime>| match t {
            Some(z) if z.day_diff != 0 => z.day_word().to_string(),
            _ => String::new(),
        };
        let root = if shape == Shape::S1X1 {
            let (name, t) = &e[0];
            Node::column(2.0, 6.0, vec![
                text(name, TextStyle::Caption, Role::Accent, Align::Center),
                text(&clock(t), TextStyle::Headline, Role::Primary, Align::Center),
                text(&day(t), TextStyle::Caption, Role::Secondary, Align::Center),
            ])
            .centered()
        } else if shape.cols >= 3 {
            // Colunas lado a lado.
            let cols: Vec<Node> = e
                .iter()
                .map(|(name, t)| {
                    Node::column(2.0, 0.0, vec![
                        text(name, TextStyle::Caption, Role::Accent, Align::Center),
                        text(&clock(t), TextStyle::Headline, Role::Primary, Align::Center),
                        text(&day(t), TextStyle::Caption, Role::Secondary, Align::Center),
                    ])
                    .size(Len::Fill(1), Len::Auto)
                })
                .collect();
            Node::row(6.0, 12.0, cols).centered()
        } else {
            let max = if shape.rows == 1 { 2 } else { 4 };
            let mut kids: Vec<Node> = Vec::new();
            if shape.rows >= 2 {
                kids.push(text("Hora no mundo", TextStyle::Title, Role::Primary, Align::Start));
            }
            for (name, t) in e.iter().take(max) {
                kids.push(
                    Node::row(6.0, 0.0, vec![
                        Node::column(0.0, 0.0, vec![
                            text(name, TextStyle::Body, Role::Primary, Align::Start),
                            text(&day(t), TextStyle::Caption, Role::Secondary, Align::Start),
                        ])
                        .size(Len::Fill(1), Len::Auto),
                        text(&clock(t), TextStyle::Headline, Role::Primary, Align::End).size(Len::Fixed(84.0), Len::Auto),
                    ])
                    .size(Len::Fill(1), Len::Auto),
                );
            }
            Node::column(if shape.rows == 1 { 4.0 } else { 8.0 }, 14.0, kids).centered().size(Len::Fill(1), Len::Fill(1))
        };
        View::new(root.fill())
    }
}

impl Widget for WorldClock {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => Outcome {
                invalidate: Invalidation::CONTENT,
                effects: vec![Effect::TimeTicks(Some(Granularity::Minute)), Effect::Zones(self.keys())],
            },
            Event::Zones(times) => {
                let new: Vec<Option<ZoneTime>> = times.clone();
                if new == self.times {
                    return Outcome::none();
                }
                self.times = new;
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![] }
            }
            Event::MenuCommand(c) if *c >= CITY_BASE && ((*c - CITY_BASE) as usize) < CITIES.len() => {
                let id = CITIES[(*c - CITY_BASE) as usize].0;
                if !worldclock::toggle(&mut self.cities, id) {
                    return Outcome::none();
                }
                self.times.clear();
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged, Effect::Zones(self.keys())] }
            }
            Event::ShapeChanged(_) | Event::Time(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
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
        CITIES
            .iter()
            .enumerate()
            .map(|(i, c)| MenuItem { id: CITY_BASE + i as u32, label: c.1.to_string(), checked: self.cities.iter().any(|x| x == c.0), group: Some("Cidades") })
            .collect()
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&Saved { cities: self.cities.clone() }).unwrap_or_default() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        vec![Effect::TimeTicks(None), Effect::Zones(Vec::new())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::time::{weekday_of, TimeInfo};

    fn ctx(shape: Shape) -> ViewCtx {
        ViewCtx { shape, time: TimeInfo { year: 2026, month: 10, day: 19, weekday: weekday_of(2026, 10, 19), hour: 9, minute: 0, second: 0, millis: 0 } }
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
    fn starting_subscribes_to_the_minute_and_the_chosen_zones() {
        let mut w = create(None).unwrap();
        let o = w.handle(&Event::Started { now_ms: 0 });
        assert_eq!(o.effects[0], Effect::TimeTicks(Some(Granularity::Minute)));
        match &o.effects[1] {
            Effect::Zones(k) => assert_eq!(k, &["Eastern Standard Time", "GMT Standard Time", "Tokyo Standard Time"]),
            other => panic!("{other:?}"),
        }
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::TimeTicks(None), Effect::Zones(Vec::new())]);
    }

    #[test]
    fn received_times_show_with_a_day_word_when_the_day_differs() {
        let mut w = create(None).unwrap();
        let z = |h, m, d| Some(ZoneTime { hour: h, minute: m, day_diff: d });
        assert_eq!(w.handle(&Event::Zones(vec![z(8, 0, 0), z(12, 0, 0), z(21, 5, 1)])).invalidate, Invalidation::CONTENT);
        assert_eq!(w.handle(&Event::Zones(vec![z(8, 0, 0), z(12, 0, 0), z(21, 5, 1)])), Outcome::none());
        let t = texts(&w.tile_view(&ctx(Shape::S2X2)).unwrap());
        assert!(t.contains(&"21:05".to_string()) && t.contains(&"amanh\u{e3}".to_string()), "{t:?}");
        let before = texts(&create(None).unwrap().tile_view(&ctx(Shape::S1X1)).unwrap());
        assert!(before.contains(&"--:--".to_string()), "sem hora ainda: {before:?}");
    }

    #[test]
    fn menu_toggles_cities_and_resubscribes() {
        let mut w = create(None).unwrap();
        let paris = CITIES.iter().position(|c| c.0 == "paris").unwrap() as u32;
        let o = w.handle(&Event::MenuCommand(CITY_BASE + paris));
        assert!(matches!(o.effects.as_slice(), [Effect::StateChanged, Effect::Zones(k)] if k.len() == 4));
        assert!(w.menu_items().iter().filter(|m| m.checked).count() == 4);
        let again = create(Some(&w.snapshot())).unwrap();
        assert_eq!(again.snapshot().data, w.snapshot().data);
    }

    #[test]
    fn bad_state_is_handled_and_every_shape_is_valid() {
        assert!(create(Some(&Snapshot { version: 1, data: "{".into() })).is_err());
        assert!(create(Some(&Snapshot { version: 2, data: String::new() })).is_err());
        let w = create(Some(&Snapshot { version: 1, data: r#"{"cities":["atlantida","paris","paris"]}"#.into() })).unwrap();
        assert_eq!(w.snapshot().data, r#"{"cities":["paris"]}"#);
        let empty = create(Some(&Snapshot { version: 1, data: r#"{"cities":[]}"#.into() })).unwrap();
        assert!(empty.snapshot().data.contains("nova-york"));
        for shape in descriptor().shapes {
            assert!(w.tile_view(&ctx(shape)).unwrap().validate().is_ok(), "{shape}");
        }
    }
}
