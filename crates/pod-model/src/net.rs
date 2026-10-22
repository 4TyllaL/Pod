//! Serviço de rede por opt-in (ADR-0001, W5): modelos de endpoint registrados, validação e leitura
//! das respostas do clima (Open-Meteo). Tudo puro: **quem monta a URL é o host**, a partir de um modelo
//! registrado aqui (o widget nunca entrega uma URL), e só `https` para hosts da lista permitida.
//!
//! Privacidade: o único dado que sai do computador é o nome da cidade (geocodificação) e as coordenadas
//! dela (previsão), e só com "Widgets de internet" ligado pelo usuário. Sem telemetria, sem cookies.

/// Hosts que o corretor aceita. Qualquer outro é recusado antes de abrir a conexão.
pub const ALLOWED_HOSTS: [&str; 11] = [
    "geocoding-api.open-meteo.com",
    "api.open-meteo.com",
    // Feeds de noticias do catalogo (`news::FEEDS`).
    "feeds.bbci.co.uk",
    "agenciabrasil.ebc.com.br",
    "rss.uol.com.br",
    "hnrss.org",
    "www.theverge.com",
    // API do GitHub (widget GitHub): o unico host que recebe o token do cofre.
    "api.github.com",
    // Cotações (widget Bolsa): as três fontes selecionáveis.
    "economia.awesomeapi.com.br",
    "brapi.dev",
    "query1.finance.yahoo.com",
];

/// Tamanho máximo da resposta aceita, em bytes.
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// Pedido de rede que o host sabe montar. Não carrega URL de ninguém.
#[derive(Debug, Clone, PartialEq)]
pub enum Endpoint {
    /// Procura uma cidade pelo nome (1 resultado).
    Geocode { name: String },
    /// Previsão atual e mínima/máxima do dia para uma coordenada.
    Forecast { lat: f64, lon: f64 },
    /// Feed RSS/Atom do catalogo (`news::FEEDS`), por id: o widget nunca fornece a URL.
    Feed { id: String },
}

/// Codifica para a parte de consulta da URL (UTF-8, mantém só letras, dígitos e `-_.~`).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Nome de cidade aceito: sem espaços nas pontas, de 2 a 60 caracteres, sem controle.
pub fn clean_city(s: &str) -> Option<String> {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let n = one.chars().count();
    (2..=60).contains(&n).then_some(one).filter(|c| !c.chars().any(|ch| ch.is_control()))
}

impl Endpoint {
    /// A URL que o host vai buscar; `None` se algum parâmetro é inválido.
    pub fn url(&self) -> Option<String> {
        match self {
            Endpoint::Geocode { name } => {
                let name = clean_city(name)?;
                Some(format!(
                    "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=pt&format=json",
                    percent_encode(&name)
                ))
            }
            Endpoint::Feed { id } => crate::news::feed_by_id(id).map(|f| f.url.to_string()),
            Endpoint::Forecast { lat, lon } => {
                if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90.0 || lon.abs() > 180.0 {
                    return None;
                }
                Some(format!(
                    "https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}\
                     &current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day\
                     &daily=temperature_2m_max,temperature_2m_min\
                     &hourly=temperature_2m,weather_code,is_day&forecast_hours=12&timezone=auto&forecast_days=2"
                ))
            }
        }
    }
}

/// `https://host/...` com host na lista permitida (sem usuário, sem porta diferente).
pub fn url_allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    !host.contains('@') && !host.contains(':') && ALLOWED_HOSTS.contains(&host)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub country: String,
    pub lat: f64,
    pub lon: f64,
}

/// Primeiro resultado da geocodificação (`{"results":[{...}]}`), ou `None` se não achou.
pub fn parse_geocode(body: &str) -> Option<Place> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let r = v.get("results")?.as_array()?.first()?;
    let (lat, lon) = (r.get("latitude")?.as_f64()?, r.get("longitude")?.as_f64()?);
    if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90.0 || lon.abs() > 180.0 {
        return None;
    }
    let text = |k: &str| r.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let name = text("name");
    (!name.is_empty()).then(|| Place { name, country: text("country"), lat, lon })
}

/// Uma hora da previsão (hora local 0–23).
#[derive(Debug, Clone, PartialEq)]
pub struct HourPoint {
    pub hour: u8,
    pub temp_c: f32,
    pub code: u8,
    pub is_day: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherReport {
    pub temp_c: f32,
    pub feels_c: f32,
    pub humidity: u8,
    pub wind_kmh: f32,
    pub code: u8,
    pub is_day: bool,
    pub hi_c: f32,
    pub lo_c: f32,
    /// Próximas horas (até 6), depois da hora atual. Vazio se o serviço não mandou.
    pub hourly: Vec<HourPoint>,
}

/// Lê a resposta da previsão. `None` se faltar qualquer campo essencial.
pub fn parse_forecast(body: &str) -> Option<WeatherReport> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let cur = v.get("current")?;
    let f = |k: &str| cur.get(k).and_then(|x| x.as_f64()).filter(|x| x.is_finite());
    let day = v.get("daily")?;
    let first = |k: &str| day.get(k)?.as_array()?.first()?.as_f64().filter(|x| x.is_finite());
    Some(WeatherReport {
        temp_c: f("temperature_2m")? as f32,
        feels_c: f("apparent_temperature").or_else(|| f("temperature_2m"))? as f32,
        humidity: f("relative_humidity_2m")?.clamp(0.0, 100.0).round() as u8,
        wind_kmh: f("wind_speed_10m")?.max(0.0) as f32,
        code: f("weather_code")?.clamp(0.0, 255.0) as u8,
        is_day: f("is_day").is_none_or(|d| d != 0.0),
        hi_c: first("temperature_2m_max")? as f32,
        lo_c: first("temperature_2m_min")? as f32,
        hourly: parse_hourly(&v),
    })
}

/// Próximas 6 horas depois de `current.time` (as horas vêm em hora local por `timezone=auto`).
fn parse_hourly(v: &serde_json::Value) -> Vec<HourPoint> {
    let now = v.get("current").and_then(|c| c.get("time")).and_then(|t| t.as_str()).unwrap_or("");
    let Some(h) = v.get("hourly") else { return Vec::new() };
    let arr = |k: &str| h.get(k).and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let (times, temps, codes, days) = (arr("time"), arr("temperature_2m"), arr("weather_code"), arr("is_day"));
    let mut out = Vec::new();
    for (i, t) in times.iter().enumerate() {
        let Some(t) = t.as_str() else { continue };
        if t <= now {
            continue;
        }
        let hour = t.get(11..13).and_then(|x| x.parse::<u8>().ok()).filter(|h| *h < 24);
        let temp = temps.get(i).and_then(|x| x.as_f64()).filter(|x| x.is_finite());
        let (Some(hour), Some(temp)) = (hour, temp) else { continue };
        let code = codes.get(i).and_then(|x| x.as_f64()).map_or(3, |c| c.clamp(0.0, 255.0) as u8);
        let is_day = days.get(i).and_then(|x| x.as_f64()).is_none_or(|d| d != 0.0);
        out.push(HourPoint { hour, temp_c: temp as f32, code, is_day });
        if out.len() == 6 {
            break;
        }
    }
    out
}

/// Descrição em português e símbolo para um código WMO do Open-Meteo.
pub fn describe_code(code: u8, is_day: bool) -> (&'static str, &'static str) {
    match code {
        0 => ("C\u{e9}u limpo", if is_day { "\u{2600}" } else { "\u{263e}" }),
        1 => ("Quase limpo", if is_day { "\u{2600}" } else { "\u{263e}" }),
        2 => ("Parcialmente nublado", "\u{26c5}"),
        3 => ("Nublado", "\u{2601}"),
        45 | 48 => ("Neblina", "\u{2601}"),
        51..=57 => ("Garoa", "\u{2602}"),
        61..=67 => ("Chuva", "\u{2602}"),
        71..=77 | 85 | 86 => ("Neve", "\u{2744}"),
        80..=82 => ("Pancadas de chuva", "\u{2602}"),
        95..=99 => ("Tempestade", "\u{26a1}"),
        _ => ("Tempo inst\u{e1}vel", "\u{2601}"),
    }
}

/// "23°" (arredondado, sem casa decimal; "−3°" com o sinal de menos tipográfico).
pub fn format_temp(c: f32) -> String {
    let r = c.round() as i32;
    if r < 0 {
        format!("\u{2212}{}\u{b0}", -r)
    } else {
        format!("{r}\u{b0}")
    }
}

/// Estado que o widget Weather recebe do host.
#[derive(Debug, Clone, PartialEq)]
pub enum WeatherState {
    /// "Widgets de internet" está desligado (o padrão): nada foi enviado.
    Off,
    /// Ainda buscando.
    Loading,
    /// O widget não tem cidade definida.
    NoCity,
    /// A geocodificação não achou a cidade.
    NotFound,
    /// Sem conexão, tempo esgotado ou resposta inválida; mensagem curta para o cartão.
    Failed(String),
    Ready { place: String, report: WeatherReport },
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEO: &str = r#"{"results":[{"id":1,"name":"Recife","latitude":-8.05389,"longitude":-34.88111,"country":"Brasil","admin1":"Pernambuco"}],"generationtime_ms":0.5}"#;
    const FC: &str = r#"{"latitude":-8.05,"longitude":-34.88,"current":{"time":"2026-10-18T12:00","interval":900,"temperature_2m":28.4,"apparent_temperature":31.2,"relative_humidity_2m":74,"is_day":1,"weather_code":2,"wind_speed_10m":14.7},"daily":{"time":["2026-10-18"],"temperature_2m_max":[30.1],"temperature_2m_min":[23.6]}}"#;

    #[test]
    fn urls_are_built_from_templates_with_encoded_parameters() {
        let u = Endpoint::Geocode { name: "  S\u{e3}o   Paulo ".into() }.url().unwrap();
        assert_eq!(u, "https://geocoding-api.open-meteo.com/v1/search?name=S%C3%A3o%20Paulo&count=1&language=pt&format=json");
        let f = Endpoint::Forecast { lat: -8.05389, lon: -34.88111 }.url().unwrap();
        assert!(f.starts_with("https://api.open-meteo.com/v1/forecast?latitude=-8.0539&longitude=-34.8811&current="), "{f}");
        assert!(url_allowed(&u) && url_allowed(&f), "o que o host monta sempre passa na lista");
    }

    #[test]
    fn hourly_points_after_the_current_hour() {
        let body = r#"{"current":{"time":"2026-10-18T12:00","temperature_2m":28,"relative_humidity_2m":70,"weather_code":2,"wind_speed_10m":10,"is_day":1},
            "daily":{"temperature_2m_max":[30],"temperature_2m_min":[23]},
            "hourly":{"time":["2026-10-18T11:00","2026-10-18T12:00","2026-10-18T13:00","2026-10-18T14:00"],
                      "temperature_2m":[27,28,29.4,null],"weather_code":[1,2,61,3],"is_day":[1,1,1,0]}}"#;
        let r = parse_forecast(body).unwrap();
        assert_eq!(r.hourly, vec![HourPoint { hour: 13, temp_c: 29.4, code: 61, is_day: true }]);
        assert!(parse_forecast(FC).unwrap().hourly.is_empty(), "sem bloco hourly continua valendo");
    }

    #[test]
    fn invalid_parameters_make_no_url() {
        assert_eq!(clean_city("linha\nquebrada").as_deref(), Some("linha quebrada"), "quebra de linha vira espaço");
        for bad in ["", "a", &"x".repeat(61), "sino\u{7}x"] {
            assert_eq!(Endpoint::Geocode { name: bad.into() }.url(), None, "{bad:?}");
        }
        for (lat, lon) in [(91.0, 0.0), (0.0, 181.0), (f64::NAN, 0.0), (0.0, f64::INFINITY)] {
            assert_eq!(Endpoint::Forecast { lat, lon }.url(), None);
        }
        // Tentativa de injetar parâmetros no nome vira texto codificado, não novo parâmetro.
        let u = Endpoint::Geocode { name: "x&count=999#".into() }.url().unwrap();
        assert!(u.contains("name=x%26count%3D999%23&count=1"), "{u}");
    }

    #[test]
    fn feeds_come_only_from_the_catalog() {
        assert_eq!(
            Endpoint::Feed { id: "bbc-pt".into() }.url().as_deref(),
            Some("https://feeds.bbci.co.uk/portuguese/rss.xml")
        );
        for bad in ["", "https://evil.example.com/rss", "BBC-PT", "../x"] {
            assert_eq!(Endpoint::Feed { id: bad.into() }.url(), None, "{bad}");
        }
        assert!(url_allowed("https://hnrss.org/frontpage"));
        assert!(!url_allowed("https://hnrss.org.evil.com/frontpage"));
    }

    #[test]
    fn only_https_to_allowed_hosts_pass() {
        assert!(url_allowed("https://api.open-meteo.com/v1/forecast?x=1"));
        for bad in [
            "http://api.open-meteo.com/v1/forecast",
            "https://evil.example.com/v1/forecast",
            "https://api.open-meteo.com.evil.com/x",
            "https://api.open-meteo.com@evil.com/x",
            "https://user:pw@api.open-meteo.com/x",
            "https://api.open-meteo.com:8443/x",
            "ftp://api.open-meteo.com/x",
            "//api.open-meteo.com/x",
            "",
        ] {
            assert!(!url_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn geocode_and_forecast_are_parsed() {
        let p = parse_geocode(GEO).unwrap();
        assert_eq!((p.name.as_str(), p.country.as_str()), ("Recife", "Brasil"));
        assert!((p.lat + 8.05389).abs() < 1e-9);
        let w = parse_forecast(FC).unwrap();
        assert_eq!((w.code, w.humidity, w.is_day), (2, 74, true));
        assert!((w.temp_c - 28.4).abs() < 1e-4 && (w.hi_c - 30.1).abs() < 1e-4 && (w.lo_c - 23.6).abs() < 1e-4);
        assert!((w.wind_kmh - 14.7).abs() < 1e-4);
    }

    #[test]
    fn broken_or_partial_answers_are_rejected() {
        assert_eq!(parse_geocode(r#"{"generationtime_ms":0.2}"#), None, "sem resultados");
        assert_eq!(parse_geocode(r#"{"results":[]}"#), None);
        assert_eq!(parse_geocode(r#"{"results":[{"name":"X","latitude":999,"longitude":0}]}"#), None, "fora do intervalo");
        assert_eq!(parse_geocode("html de erro"), None);
        assert_eq!(parse_forecast(r#"{"current":{"temperature_2m":20}}"#), None, "faltam campos");
        assert_eq!(parse_forecast("{"), None);
        assert_eq!(parse_forecast(r#"{"current":{"temperature_2m":"quente"}}"#), None);
    }

    #[test]
    fn descriptions_and_temperatures_in_portuguese() {
        assert_eq!(describe_code(0, true).0, "C\u{e9}u limpo");
        assert_ne!(describe_code(0, true).1, describe_code(0, false).1, "lua \u{e0} noite");
        assert_eq!(describe_code(63, true).0, "Chuva");
        assert_eq!(describe_code(96, true).0, "Tempestade");
        assert_eq!(describe_code(200, true).0, "Tempo inst\u{e1}vel");
        assert_eq!(format_temp(28.4), "28\u{b0}");
        assert_eq!(format_temp(-2.6), "\u{2212}3\u{b0}");
        assert_eq!(format_temp(-0.2), "0\u{b0}");
        assert_eq!(percent_encode("a b/\u{e7}"), "a%20b%2F%C3%A7");
    }
}
