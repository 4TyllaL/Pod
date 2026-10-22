//! Feriados nacionais do Brasil (calculados, sem internet): datas fixas e as que dependem da Páscoa.
//! Não inclui feriados estaduais/municipais. Carnaval e Corpus Christi são pontos facultativos mas
//! entram na lista porque a maioria das pessoas folga.

use crate::time::weekday_of;
use crate::usage::days_from_civil;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Holiday {
    pub name: &'static str,
    pub year: i32,
    pub month: u8,
    pub day: u8,
    /// Ponto facultativo (não é feriado por lei).
    pub optional: bool,
}

impl Holiday {
    pub fn weekday(&self) -> u8 {
        weekday_of(self.year, self.month, self.day)
    }
    pub fn number(&self) -> i64 {
        days_from_civil(self.year as i64, self.month as u32, self.day as u32)
    }
}

/// Domingo de Páscoa (algoritmo anônimo gregoriano, Meeus/Jones/Butcher): `(mês, dia)`.
pub fn easter(year: i32) -> (u8, u8) {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    (month as u8, day as u8)
}

/// Data (ano, mês, dia) a partir do número de dias desde 1970.
pub fn civil_from_days(z: i64) -> (i32, u8, u8) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    ((y + if m <= 2 { 1 } else { 0 }) as i32, m as u8, d as u8)
}

/// Feriados do ano em ordem de data.
pub fn of_year(year: i32) -> Vec<Holiday> {
    let fixed = |name, month, day| Holiday { name, year, month, day, optional: false };
    let (em, ed) = easter(year);
    let easter_n = days_from_civil(year as i64, em as u32, ed as u32);
    let by_easter = |name, offset: i64, optional| {
        let (y, m, d) = civil_from_days(easter_n + offset);
        Holiday { name, year: y, month: m, day: d, optional }
    };
    let mut v = vec![
        fixed("Confraterniza\u{e7}\u{e3}o Universal", 1, 1),
        by_easter("Carnaval", -47, true),
        by_easter("Sexta-feira Santa", -2, false),
        by_easter("P\u{e1}scoa", 0, false),
        fixed("Tiradentes", 4, 21),
        fixed("Dia do Trabalho", 5, 1),
        by_easter("Corpus Christi", 60, true),
        fixed("Independ\u{ea}ncia", 9, 7),
        fixed("N. Sra. Aparecida", 10, 12),
        fixed("Finados", 11, 2),
        fixed("Dia da Rep\u{fa}blica", 11, 15),
        fixed("Consci\u{ea}ncia Negra", 11, 20),
        fixed("Natal", 12, 25),
    ];
    v.sort_by_key(Holiday::number);
    v
}

/// Os próximos `n` feriados a partir de `from` (inclui o próprio dia), passando para o ano seguinte.
pub fn upcoming(from: (i32, u8, u8), n: usize) -> Vec<Holiday> {
    let today = days_from_civil(from.0 as i64, from.1 as u32, from.2 as u32);
    let mut out = Vec::new();
    for year in from.0..=from.0 + 1 {
        for h in of_year(year) {
            if h.number() >= today && out.len() < n {
                out.push(h);
            }
        }
    }
    out
}

/// Dias de `from` até o feriado.
pub fn days_until(from: (i32, u8, u8), h: &Holiday) -> i64 {
    h.number() - days_from_civil(from.0 as i64, from.1 as u32, from.2 as u32)
}

/// O feriado cai numa segunda ou sexta (fim de semana prolongado)?
pub fn long_weekend(h: &Holiday) -> bool {
    !h.optional && matches!(h.weekday(), 1 | 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easter_matches_known_dates() {
        assert_eq!(easter(2024), (3, 31));
        assert_eq!(easter(2025), (4, 20));
        assert_eq!(easter(2026), (4, 5));
        assert_eq!(easter(2027), (3, 28));
        assert_eq!(easter(2000), (4, 23));
    }

    #[test]
    fn moveable_holidays_follow_easter() {
        let h = of_year(2026);
        let find = |n: &str| h.iter().find(|x| x.name == n).map(|x| (x.month, x.day)).unwrap();
        assert_eq!(find("Carnaval"), (2, 17));
        assert_eq!(find("Sexta-feira Santa"), (4, 3));
        assert_eq!(find("Corpus Christi"), (6, 4));
        assert_eq!(h.len(), 13);
        assert!(h.windows(2).all(|w| w[0].number() < w[1].number()), "em ordem de data");
    }

    #[test]
    fn civil_from_days_inverts_days_from_civil() {
        for (y, m, d) in [(1970, 1, 1), (2026, 10, 19), (2024, 2, 29), (2000, 12, 31), (1999, 3, 1)] {
            assert_eq!(civil_from_days(days_from_civil(y, m as u32, d as u32)), (y as i32, m, d));
        }
    }

    #[test]
    fn upcoming_crosses_into_next_year_and_counts_days() {
        let u = upcoming((2026, 12, 20), 3);
        assert_eq!(u[0].name, "Natal");
        assert_eq!((u[1].year, u[1].month, u[1].day), (2027, 1, 1));
        assert_eq!(days_until((2026, 12, 20), &u[0]), 5);
        assert_eq!(days_until((2026, 12, 25), &u[0]), 0, "hoje conta");
        assert_eq!(upcoming((2026, 10, 19), 2).len(), 2);
    }

    #[test]
    fn long_weekends_are_mondays_and_fridays_only() {
        // 7/9/2026 \u{e9} segunda; 12/10/2026 tamb\u{e9}m; 25/12/2026 \u{e9} sexta; Carnaval \u{e9} ponto facultativo.
        let h = of_year(2026);
        let get = |n: &str| *h.iter().find(|x| x.name == n).unwrap();
        assert!(long_weekend(&get("Independ\u{ea}ncia")) && long_weekend(&get("Natal")));
        assert!(!long_weekend(&get("Carnaval")));
        assert!(!long_weekend(&get("Tiradentes")), "21/4/2026 \u{e9} ter\u{e7}a");
    }
}
