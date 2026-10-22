//! Widget Weather: temperatura e condição atuais da cidade escolhida (Open-Meteo), em 1×1, 2×1 e 2×2.
//! **Usa a internet**, então só funciona com "Widgets de internet" ligado no menu da bandeja (desligado
//! por padrão); sem isso o cartão diz como ligar e nada sai do computador. A única informação enviada é o
//! nome da cidade (e depois as coordenadas dela). A cidade é o único estado durável.

use serde::{Deserialize, Serialize};

use pod_model::ids::WidgetTypeId;
use pod_model::net::{clean_city, describe_code, format_temp, WeatherReport, WeatherState};
use pod_model::shape::Shape;
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, InputSpec, Invalidation, LocalId, MenuItem, Outcome, SizeDip, Snapshot,
    StopReason, ViewCtx, Widget, WidgetError,
};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.weather");
pub const STATE_VERSION: u32 = 1;

const SET_CITY: u32 = 1;

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

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    #[serde(default)]
    city: String,
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let saved = match restore {
        None => Saved::default(),
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) if s.data.is_empty() => Saved::default(),
        Some(s) => serde_json::from_str::<Saved>(&s.data).map_err(|e| WidgetError::InvalidState(e.to_string()))?,
    };
    // Estado gravado com cidade inválida (editado à mão) não derruba nada: vira "sem cidade".
    let city = clean_city(&saved.city).unwrap_or_default();
    let state = if city.is_empty() { WeatherState::NoCity } else { WeatherState::Loading };
    Ok(Box::new(Weather { city, state, stopped: false }))
}

struct Weather {
    city: String,
    state: WeatherState,
    stopped: bool,
}

fn text(s: &str, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

fn hilo(r: &WeatherReport) -> String {
    format!("m\u{e1}x {} \u{b7} m\u{ed}n {}", format_temp(r.hi_c), format_temp(r.lo_c))
}

fn set_city_button() -> Node {
    Node::new(Kind::Button { id: LocalId(SET_CITY), label: "Definir cidade".into() })
}

/// Cartão sem previsão: (linha principal, dica, mostra o botão de cidade).
fn message(state: &WeatherState) -> (String, &'static str, bool) {
    match state {
        WeatherState::Off => (
            "Internet desligada".into(),
            "Ligue \"Widgets de internet\" no menu da bandeja para ver o tempo.",
            false,
        ),
        WeatherState::Loading => ("Buscando\u{2026}".into(), "", false),
        WeatherState::NoCity => ("Escolha a cidade".into(), "O nome da cidade \u{e9} a \u{fa}nica coisa enviada \u{e0} internet.", true),
        WeatherState::NotFound => ("Cidade n\u{e3}o encontrada".into(), "Confira o nome (ex.: Recife, S\u{e3}o Paulo).", true),
        WeatherState::Failed(m) => (m.clone(), "Tentarei de novo em 1 minuto.", false),
        WeatherState::Ready { .. } => (String::new(), "", false),
    }
}

fn view_for(shape: Shape, state: &WeatherState) -> View {
    let root = match state {
        WeatherState::Ready { place, report } => ready(shape, place, report),
        other => {
            let (main, hint, button) = message(other);
            match shape {
                s if s == Shape::S1X1 => {
                    Node::column(2.0, 6.0, vec![
                        text("\u{2601}", TextStyle::Headline, Role::Accent, Align::Center),
                        text(&short_label(other), TextStyle::Caption, Role::Secondary, Align::Center),
                    ])
                    .centered()
                }
                s if s == Shape::S2X2 => {
                    let mut col = vec![text(&main, TextStyle::Body, Role::Primary, Align::Center)];
                    if !hint.is_empty() {
                        col.push(Node::new(Kind::Text { text: hint.to_string(), style: TextStyle::Caption, role: Role::Secondary, align: Align::Center, max_lines: 3 }).size(Len::Fill(1), Len::Auto));
                    }
                    if button {
                        col.push(set_city_button());
                    }
                    Node::column(8.0, 14.0, col).centered()
                }
                _ => {
                    let mut col = vec![text(&main, TextStyle::Body, Role::Primary, Align::Center)];
                    if button {
                        col.push(set_city_button());
                    } else if !hint.is_empty() {
                        col.push(Node::new(Kind::Text { text: hint.to_string(), style: TextStyle::Caption, role: Role::Secondary, align: Align::Center, max_lines: 2 }).size(Len::Fill(1), Len::Auto));
                    }
                    Node::column(4.0, 10.0, col).centered()
                }
            }
        }
    };
    View::new(root.fill())
}

/// Rótulo curto do 1×1 sem previsão.
fn short_label(s: &WeatherState) -> String {
    match s {
        WeatherState::Off => "OFFLINE".into(),
        WeatherState::Loading => "TEMPO".into(),
        WeatherState::NoCity => "CIDADE?".into(),
        WeatherState::NotFound => "N\u{c3}O ACHEI".into(),
        WeatherState::Failed(_) => "SEM REDE".into(),
        WeatherState::Ready { .. } => String::new(),
    }
}

/// Faixa de previsão por hora: uma coluna por hora (hora, ícone, temperatura).
fn hourly_strip(r: &WeatherReport, n: usize) -> Node {
    let cols: Vec<Node> = r
        .hourly
        .iter()
        .take(n)
        .map(|h| {
            Node::column(3.0, 0.0, vec![
                text(&format!("{:02}h", h.hour), TextStyle::Caption, Role::Secondary, Align::Center),
                text(describe_code(h.code, h.is_day).1, TextStyle::Body, Role::Accent, Align::Center),
                text(&format_temp(h.temp_c), TextStyle::Body, Role::Primary, Align::Center),
            ])
            .size(Len::Fill(1), Len::Auto)
        })
        .collect();
    if cols.is_empty() {
        return text("Previs\u{e3}o por hora indispon\u{ed}vel", TextStyle::Caption, Role::Secondary, Align::Center);
    }
    Node::row(4.0, 0.0, cols).size(Len::Fill(1), Len::Auto)
}

fn ready(shape: Shape, place: &str, r: &WeatherReport) -> Node {
    let (desc, glyph) = describe_code(r.code, r.is_day);
    let temp = format_temp(r.temp_c);
    // Só a cidade, sem o país, onde falta espaço.
    let city_only = place.split(',').next().unwrap_or(place).trim();
    match shape {
        s if s.cols >= 3 && s.rows == 1 => Node::row(12.0, 12.0, vec![
            Node::column(0.0, 0.0, vec![
                text(glyph, TextStyle::Headline, Role::Accent, Align::Center),
                text(&temp, TextStyle::Headline, Role::Primary, Align::Center),
            ])
            .size(Len::Fixed(64.0), Len::Auto),
            Node::column(2.0, 0.0, vec![
                text(city_only, TextStyle::Body, Role::Primary, Align::Start),
                text(desc, TextStyle::Caption, Role::Secondary, Align::Start),
                text(&hilo(r), TextStyle::Caption, Role::Secondary, Align::Start),
            ])
            .size(Len::Fixed(120.0), Len::Auto),
            hourly_strip(r, 5),
        ]),
        s if s.cols >= 3 => Node::column(8.0, 14.0, vec![
            Node::row(12.0, 0.0, vec![
                Node::column(2.0, 0.0, vec![
                    text(place, TextStyle::Title, Role::Primary, Align::Start),
                    text(desc, TextStyle::Body, Role::Secondary, Align::Start),
                ])
                .size(Len::Fill(1), Len::Auto),
                text(glyph, TextStyle::Display, Role::Accent, Align::End).size(Len::Fixed(48.0), Len::Auto),
                Node::column(0.0, 0.0, vec![
                    text(&temp, TextStyle::Display, Role::Primary, Align::End),
                    text(&hilo(r), TextStyle::Caption, Role::Secondary, Align::End),
                ])
                .size(Len::Fixed(120.0), Len::Auto),
            ])
            .size(Len::Fill(1), Len::Auto),
            Node::divider(),
            hourly_strip(r, 6),
        ]),
        s if s == Shape::S1X1 => Node::column(2.0, 6.0, vec![
            text(glyph, TextStyle::Title, Role::Accent, Align::Center),
            text(&temp, TextStyle::Headline, Role::Primary, Align::Center),
            text(city_only, TextStyle::Caption, Role::Secondary, Align::Center),
        ])
        .centered(),
        s if s == Shape::S2X2 => Node::column(4.0, 14.0, vec![
            text(place, TextStyle::Title, Role::Primary, Align::Start),
            Node::row(10.0, 0.0, vec![
                text(glyph, TextStyle::Display, Role::Accent, Align::Start).size(Len::Auto, Len::Auto),
                text(&temp, TextStyle::Display, Role::Primary, Align::Start),
            ])
            .size(Len::Fill(1), Len::Auto),
            text(desc, TextStyle::Body, Role::Secondary, Align::Start),
            Node::divider(),
            text(&hilo(r), TextStyle::Caption, Role::Primary, Align::Start),
            text(&format!("Sensa\u{e7}\u{e3}o {} \u{b7} Umidade {}%", format_temp(r.feels_c), r.humidity), TextStyle::Caption, Role::Secondary, Align::Start),
            text(&format!("Vento {} km/h", r.wind_kmh.round() as i32), TextStyle::Caption, Role::Secondary, Align::Start),
        ]),
        _ => Node::row(10.0, 12.0, vec![
            Node::column(0.0, 0.0, vec![
                text(glyph, TextStyle::Headline, Role::Accent, Align::Center),
                text(&temp, TextStyle::Headline, Role::Primary, Align::Center),
            ])
            .size(Len::Fixed(64.0), Len::Auto),
            Node::column(2.0, 0.0, vec![
                text(city_only, TextStyle::Body, Role::Primary, Align::Start),
                text(desc, TextStyle::Caption, Role::Secondary, Align::Start),
                text(&hilo(r), TextStyle::Caption, Role::Secondary, Align::Start),
            ])
            .size(Len::Fill(1), Len::Auto),
        ]),
    }
}

impl Weather {
    fn ask_city(&self) -> Outcome {
        Outcome {
            invalidate: Invalidation::NONE,
            effects: vec![Effect::OpenInput(InputSpec {
                title: "Weather".into(),
                label: "Cidade (o nome \u{e9} enviado ao servi\u{e7}o de previs\u{e3}o)".into(),
                initial: self.city.clone(),
                secret: false,
            })],
        }
    }
}

impl Widget for Weather {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => {
                let effects = if self.city.is_empty() { vec![] } else { vec![Effect::Weather(Some(self.city.clone()))] };
                Outcome { invalidate: Invalidation::CONTENT, effects }
            }
            Event::Weather(s) => {
                if self.city.is_empty() || *s == self.state {
                    return Outcome::none();
                }
                self.state = s.clone();
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![] }
            }
            Event::Clicked { button } if button.0 == SET_CITY => self.ask_city(),
            Event::MenuCommand(SET_CITY) => self.ask_city(),
            Event::TextChanged { text, .. } => match clean_city(text) {
                Some(city) => {
                    self.city = city.clone();
                    self.state = WeatherState::Loading;
                    Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged, Effect::Weather(Some(city))] }
                }
                None => Outcome {
                    invalidate: Invalidation::NONE,
                    effects: vec![Effect::InputError("Digite o nome da cidade (2 a 60 letras).".into())],
                },
            },
            Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        Some(view_for(ctx.shape, &self.state))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        vec![MenuItem { id: SET_CITY, label: "Definir cidade\u{2026}".into(), checked: false, group: None }]
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: serde_json::to_string(&Saved { city: self.city.clone() }).unwrap_or_default() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        vec![Effect::Weather(None)]
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

    pub(crate) fn demo() -> WeatherState {
        WeatherState::Ready {
            place: "Recife, Brasil".into(),
            report: WeatherReport { temp_c: 28.4, feels_c: 31.2, humidity: 74, wind_kmh: 14.7, code: 2, is_day: true, hi_c: 30.1, lo_c: 23.6, hourly: (13..19u8).map(|h| pod_model::net::HourPoint { hour: h, temp_c: 29.0 - (h as f32 - 13.0) * 0.8, code: [2, 2, 3, 61, 61, 3][(h - 13) as usize], is_day: h < 18 }).collect() },
        }
    }

    fn with_city(city: &str) -> Box<dyn Widget> {
        create(Some(&Snapshot { version: 1, data: format!("{{\"city\":\"{city}\"}}") })).unwrap()
    }

    #[test]
    fn without_a_city_it_does_not_subscribe_to_anything() {
        let mut w = create(None).unwrap();
        assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![], "sem cidade, nenhuma assinatura (nada sai)");
        assert_eq!(w.snapshot().data, "{\"city\":\"\"}");
    }

    #[test]
    fn with_a_city_it_subscribes_and_stop_unsubscribes() {
        let mut w = with_city("Recife");
        assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::Weather(Some("Recife".into()))]);
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::Weather(None)]);
        assert!(w.stop(StopReason::Removed).is_empty());
        assert_eq!(w.handle(&Event::Weather(demo())), Outcome::none());
    }

    #[test]
    fn setting_a_city_validates_persists_and_resubscribes() {
        let mut w = create(None).unwrap();
        let bad = w.handle(&Event::TextChanged { field: LocalId(0), text: "x".into() });
        assert!(matches!(bad.effects.as_slice(), [Effect::InputError(_)]));
        let ok = w.handle(&Event::TextChanged { field: LocalId(0), text: "  S\u{e3}o   Paulo ".into() });
        assert_eq!(ok.effects, vec![Effect::StateChanged, Effect::Weather(Some("S\u{e3}o Paulo".into()))]);
        assert_eq!(w.snapshot().data, "{\"city\":\"S\u{e3}o Paulo\"}");
        // A mesma cidade volta no restore.
        let again = create(Some(&w.snapshot())).unwrap();
        assert_eq!(again.snapshot().data, w.snapshot().data);
    }

    #[test]
    fn only_a_changed_state_invalidates_and_no_city_ignores_the_service() {
        let mut w = with_city("Recife");
        assert_eq!(w.handle(&Event::Weather(demo())).invalidate, Invalidation::CONTENT);
        assert_eq!(w.handle(&Event::Weather(demo())), Outcome::none());
        let mut none = create(None).unwrap();
        assert_eq!(none.handle(&Event::Weather(demo())), Outcome::none());
    }

    #[test]
    fn the_set_city_button_and_menu_ask_for_the_city() {
        let mut w = create(None).unwrap();
        for e in [Event::Clicked { button: LocalId(SET_CITY) }, Event::MenuCommand(SET_CITY)] {
            assert!(matches!(w.handle(&e).effects.as_slice(), [Effect::OpenInput(_)]));
        }
        assert_eq!(w.menu_items().len(), 1);
    }

    #[test]
    fn bad_or_future_state_is_handled() {
        assert!(matches!(
            create(Some(&Snapshot { version: 2, data: String::new() })),
            Err(WidgetError::UnsupportedVersion { .. })
        ));
        assert!(matches!(create(Some(&Snapshot { version: 1, data: "{".into() })), Err(WidgetError::InvalidState(_))));
        // Cidade inválida gravada à mão: o widget abre sem cidade.
        let w = create(Some(&Snapshot { version: 1, data: "{\"city\":\"\\u0007\"}".into() })).unwrap();
        assert_eq!(w.snapshot().data, "{\"city\":\"\"}");
    }

    #[test]
    fn every_state_and_shape_fits_the_tile() {
        let states = [
            WeatherState::Off,
            WeatherState::Loading,
            WeatherState::NoCity,
            WeatherState::NotFound,
            WeatherState::Failed("Sem conex\u{e3}o".into()),
            demo(),
            WeatherState::Ready {
                place: "Cidade com um nome muito comprido, Pa\u{ed}s de nome longo".into(),
                report: WeatherReport { temp_c: -12.6, feels_c: -20.0, humidity: 100, wind_kmh: 120.0, code: 99, is_day: false, hi_c: -3.0, lo_c: -15.0, hourly: vec![] },
            },
        ];
        for s in states {
            for shape in descriptor().shapes {
                let v = view_for(shape, &s);
                assert_eq!(v.validate(), Ok(()), "{shape}");
                let size = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, size.w, size.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{shape} {s:?}: {:?}", l.rect);
                }
            }
        }
        let w = create(None).unwrap();
        let ctx = ViewCtx { shape: Shape::S2X1, time: TimeInfo { year: 2026, month: 10, day: 18, weekday: weekday_of(2026, 10, 18), hour: 1, minute: 0, second: 0, millis: 0 } };
        assert!(w.tile_view(&ctx).is_some());
    }

    #[test]
    fn off_state_explains_how_to_turn_the_internet_on() {
        let (main, hint, button) = message(&WeatherState::Off);
        assert_eq!(main, "Internet desligada");
        assert!(hint.contains("Widgets de internet"));
        assert!(!button);
    }
}
