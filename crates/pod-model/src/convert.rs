//! Conversor de unidades (Converter): categorias, unidades, conversão e formatação em pt-BR.
//! Puro. Sem moedas (exigiriam rede).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Length,
    Mass,
    Temperature,
    Volume,
    Speed,
}

pub struct Unit {
    pub symbol: &'static str,
    pub name: &'static str,
    /// Fator para a unidade-base da categoria (metro, grama, litro, m/s). Temperatura usa fórmulas.
    pub factor: f64,
}

pub const ALL: [Category; 5] =
    [Category::Length, Category::Mass, Category::Temperature, Category::Volume, Category::Speed];

const LENGTH: [Unit; 8] = [
    Unit { symbol: "mm", name: "mil\u{ed}metro", factor: 0.001 },
    Unit { symbol: "cm", name: "cent\u{ed}metro", factor: 0.01 },
    Unit { symbol: "m", name: "metro", factor: 1.0 },
    Unit { symbol: "km", name: "quil\u{f4}metro", factor: 1000.0 },
    Unit { symbol: "pol", name: "polegada", factor: 0.0254 },
    Unit { symbol: "p\u{e9}", name: "p\u{e9}", factor: 0.3048 },
    Unit { symbol: "jd", name: "jarda", factor: 0.9144 },
    Unit { symbol: "mi", name: "milha", factor: 1609.344 },
];
const MASS: [Unit; 6] = [
    Unit { symbol: "mg", name: "miligrama", factor: 0.001 },
    Unit { symbol: "g", name: "grama", factor: 1.0 },
    Unit { symbol: "kg", name: "quilograma", factor: 1000.0 },
    Unit { symbol: "t", name: "tonelada", factor: 1_000_000.0 },
    Unit { symbol: "oz", name: "on\u{e7}a", factor: 28.349_523_125 },
    Unit { symbol: "lb", name: "libra", factor: 453.592_37 },
];
const TEMPERATURE: [Unit; 3] = [
    Unit { symbol: "\u{b0}C", name: "Celsius", factor: 1.0 },
    Unit { symbol: "\u{b0}F", name: "Fahrenheit", factor: 1.0 },
    Unit { symbol: "K", name: "Kelvin", factor: 1.0 },
];
const VOLUME: [Unit; 6] = [
    Unit { symbol: "ml", name: "mililitro", factor: 0.001 },
    Unit { symbol: "l", name: "litro", factor: 1.0 },
    Unit { symbol: "copo", name: "copo (240 ml)", factor: 0.24 },
    Unit { symbol: "col", name: "colher de sopa (15 ml)", factor: 0.015 },
    Unit { symbol: "fl oz", name: "on\u{e7}a fluida (EUA)", factor: 0.029_573_529_562_5 },
    Unit { symbol: "gal", name: "gal\u{e3}o (EUA)", factor: 3.785_411_784 },
];
const SPEED: [Unit; 4] = [
    Unit { symbol: "m/s", name: "metro por segundo", factor: 1.0 },
    Unit { symbol: "km/h", name: "quil\u{f4}metro por hora", factor: 1.0 / 3.6 },
    Unit { symbol: "mph", name: "milha por hora", factor: 0.447_04 },
    Unit { symbol: "n\u{f3}", name: "n\u{f3}", factor: 0.514_444_444_444 },
];

impl Category {
    pub fn units(self) -> &'static [Unit] {
        match self {
            Category::Length => &LENGTH,
            Category::Mass => &MASS,
            Category::Temperature => &TEMPERATURE,
            Category::Volume => &VOLUME,
            Category::Speed => &SPEED,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Category::Length => "Comprimento",
            Category::Mass => "Massa",
            Category::Temperature => "Temperatura",
            Category::Volume => "Volume",
            Category::Speed => "Velocidade",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Category::Length => "length",
            Category::Mass => "mass",
            Category::Temperature => "temperature",
            Category::Volume => "volume",
            Category::Speed => "speed",
        }
    }

    pub fn from_key(k: &str) -> Option<Category> {
        ALL.into_iter().find(|c| c.key() == k)
    }
}

/// Converte `value` da unidade `from` para `to` (índices em `cat.units()`). `None` se algum índice não existe.
pub fn convert(cat: Category, from: usize, to: usize, value: f64) -> Option<f64> {
    let units = cat.units();
    let (f, t) = (units.get(from)?, units.get(to)?);
    if cat == Category::Temperature {
        // Passa por Celsius.
        let c = match from {
            0 => value,
            1 => (value - 32.0) * 5.0 / 9.0,
            _ => value - 273.15,
        };
        return Some(match to {
            0 => c,
            1 => c * 9.0 / 5.0 + 32.0,
            _ => c + 273.15,
        });
    }
    Some(value * f.factor / t.factor)
}

/// Lê um número digitado: vírgula ou ponto como separador decimal (um só), sem separador de milhar.
pub fn parse_number(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || s.len() > 24 {
        return None;
    }
    let (sign, body) = match s.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, s.strip_prefix('+').unwrap_or(s)),
    };
    let seps = body.chars().filter(|c| *c == ',' || *c == '.').count();
    if seps > 1 || body.is_empty() || !body.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.') {
        return None;
    }
    if !body.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let v: f64 = body.replace(',', ".").parse().ok()?;
    (v.is_finite() && v <= 1e15).then_some(sign * v)
}

/// "1.234,5" — até 6 algarismos significativos, vírgula decimal, ponto de milhar, sem zeros sobrando.
pub fn format_number(x: f64) -> String {
    if !x.is_finite() {
        return "\u{2014}".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let mag = x.abs().log10().floor() as i32;
    if !(-6..=14).contains(&mag) {
        let s = format!("{x:.4e}");
        return s.replace('.', ",");
    }
    let decimals = (5 - mag).clamp(0, 10) as usize;
    let mut s = format!("{:.*}", decimals, x.abs());
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (s, None),
    };
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    let sign = if x < 0.0 && (grouped != "0" || frac.is_some()) { "-" } else { "" };
    match frac {
        Some(f) => format!("{sign}{grouped},{f}"),
        None => format!("{sign}{grouped}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(cat: Category, symbol: &str) -> usize {
        cat.units().iter().position(|u| u.symbol == symbol).unwrap()
    }

    fn conv(cat: Category, from: &str, to: &str, v: f64) -> f64 {
        convert(cat, idx(cat, from), idx(cat, to), v).unwrap()
    }

    #[test]
    fn known_conversions() {
        assert!((conv(Category::Length, "km", "mi", 10.0) - 6.213_711_9).abs() < 1e-6);
        assert!((conv(Category::Length, "pol", "cm", 1.0) - 2.54).abs() < 1e-12);
        assert!((conv(Category::Mass, "lb", "kg", 1.0) - 0.453_592_37).abs() < 1e-12);
        assert!((conv(Category::Volume, "l", "copo", 1.0) - 4.166_666_7).abs() < 1e-6);
        assert!((conv(Category::Speed, "km/h", "m/s", 36.0) - 10.0).abs() < 1e-12);
        assert!((conv(Category::Speed, "mph", "km/h", 60.0) - 96.560_64).abs() < 1e-4);
    }

    #[test]
    fn temperature_goes_through_celsius() {
        assert!((conv(Category::Temperature, "\u{b0}C", "\u{b0}F", 100.0) - 212.0).abs() < 1e-9);
        assert!((conv(Category::Temperature, "\u{b0}F", "\u{b0}C", -40.0) + 40.0).abs() < 1e-9);
        assert!((conv(Category::Temperature, "K", "\u{b0}C", 0.0) + 273.15).abs() < 1e-9);
        assert!((conv(Category::Temperature, "\u{b0}C", "K", 25.0) - 298.15).abs() < 1e-9);
    }

    #[test]
    fn roundtrips_do_not_drift() {
        for cat in ALL {
            let n = cat.units().len();
            for a in 0..n {
                for b in 0..n {
                    let there = convert(cat, a, b, 123.456).unwrap();
                    let back = convert(cat, b, a, there).unwrap();
                    assert!((back - 123.456).abs() < 1e-9 * 123.456_f64.max(there.abs()), "{cat:?} {a}->{b}");
                }
            }
        }
        assert_eq!(convert(Category::Mass, 0, 99, 1.0), None);
    }

    #[test]
    fn every_unit_has_unique_symbols_and_positive_factors() {
        for cat in ALL {
            let mut seen = std::collections::HashSet::new();
            for u in cat.units() {
                assert!(seen.insert(u.symbol), "{} repetido em {cat:?}", u.symbol);
                assert!(u.factor > 0.0 && !u.name.is_empty());
            }
            assert_eq!(Category::from_key(cat.key()), Some(cat));
        }
        assert_eq!(Category::from_key("moeda"), None);
    }

    #[test]
    fn numbers_parse_with_comma_or_dot_but_not_both() {
        assert_eq!(parse_number("12,5"), Some(12.5));
        assert_eq!(parse_number(" 12.5 "), Some(12.5));
        assert_eq!(parse_number("-3"), Some(-3.0));
        assert_eq!(parse_number("+0,25"), Some(0.25));
        assert_eq!(parse_number("1000"), Some(1000.0));
        for bad in ["", " ", "abc", "1,2,3", "1.234,5", "1e5", "--1", ",", "-", "1 000", "9999999999999999999999999", "inf", "NaN"] {
            assert_eq!(parse_number(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn numbers_format_in_portuguese() {
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(6.213_711_9), "6,21371");
        assert_eq!(format_number(1234.5), "1.234,5");
        assert_eq!(format_number(1_000_000.0), "1.000.000");
        assert_eq!(format_number(-0.5), "-0,5");
        assert_eq!(format_number(100.0), "100");
        assert_eq!(format_number(0.000_123_456_7), "0,000123457");
        assert_eq!(format_number(1.5e20), "1,5000e20");
        assert_eq!(format_number(f64::NAN), "\u{2014}");
        assert_eq!(format_number(-0.000_000_000_1), "-1,0000e-10");
    }
}
