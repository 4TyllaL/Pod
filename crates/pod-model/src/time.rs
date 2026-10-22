//! Hora local e aritmética de calendário puras (sem Win32), com textos em pt-BR.
//!
//! O host lê a hora do sistema e entrega um `TimeInfo` ao widget; toda a regra (progresso do dia/
//! semana/mês/ano, número da semana, próxima fronteira de minuto/segundo) vive aqui e é testada com
//! datas fixas, sem relógio real.

/// Instante em hora local, já decomposto. `weekday`: 0 = domingo … 6 = sábado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeInfo {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub weekday: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millis: u16,
}

/// Quão fina precisa ser a assinatura de tempo de um widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Granularity {
    Minute,
    Second,
}

/// Primeiro dia da semana para o progresso semanal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeekStart {
    Sunday,
    Monday,
}

pub fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

pub fn days_in_year(year: i32) -> u16 {
    if is_leap(year) {
        366
    } else {
        365
    }
}

/// Dias desde 1970-01-01 de uma data civil (algoritmo de Howard Hinnant); negativo antes de 1970.
pub fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let y = year as i64 - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Dia da semana (0 = domingo) de uma data do calendário gregoriano (Sakamoto).
pub fn weekday_of(year: i32, month: u8, day: u8) -> u8 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    ((y + y / 4 - y / 100 + y / 400 + T[(month as usize - 1) % 12] + day as i32).rem_euclid(7)) as u8
}

impl TimeInfo {
    /// Campos dentro das faixas e `weekday` coerente com a data.
    pub fn is_valid(&self) -> bool {
        (1..=12).contains(&self.month)
            && self.day >= 1
            && self.day <= days_in_month(self.year, self.month)
            && self.weekday < 7
            && self.weekday == weekday_of(self.year, self.month, self.day)
            && self.hour < 24
            && self.minute < 60
            && self.second < 60
            && self.millis < 1000
    }

    pub fn day_of_year(&self) -> u16 {
        (1..self.month).map(|m| days_in_month(self.year, m) as u16).sum::<u16>() + self.day as u16
    }

    /// Segundos desde 00:00 (com fração de milissegundos).
    pub fn seconds_into_day(&self) -> f64 {
        self.hour as f64 * 3600.0 + self.minute as f64 * 60.0 + self.second as f64 + self.millis as f64 / 1000.0
    }

    /// Milissegundos até a próxima fronteira (> 0): o próximo minuto cheio ou o próximo segundo cheio.
    pub fn ms_until_next(&self, g: Granularity) -> u32 {
        match g {
            Granularity::Second => 1000 - self.millis as u32,
            Granularity::Minute => (60 - self.second as u32) * 1000 - self.millis as u32,
        }
    }

    /// Semana ISO 8601 (começa na segunda; a semana 1 tem a primeira quinta-feira do ano).
    pub fn iso_week(&self) -> u8 {
        let doy = self.day_of_year() as i32;
        let wd_mon = (self.weekday as i32 + 6) % 7 + 1; // segunda = 1 … domingo = 7
        let week = (doy - wd_mon + 10) / 7;
        if week < 1 {
            iso_weeks_in_year(self.year - 1)
        } else if week > iso_weeks_in_year(self.year) as i32 {
            1
        } else {
            week as u8
        }
    }

    pub fn day_progress(&self) -> f32 {
        unit(self.seconds_into_day() / 86_400.0)
    }

    pub fn week_progress(&self, start: WeekStart) -> f32 {
        let offset = match start {
            WeekStart::Sunday => self.weekday as i32,
            WeekStart::Monday => (self.weekday as i32 + 6) % 7,
        };
        unit((offset as f64 * 86_400.0 + self.seconds_into_day()) / (7.0 * 86_400.0))
    }

    pub fn month_progress(&self) -> f32 {
        let dim = days_in_month(self.year, self.month) as f64;
        unit(((self.day as f64 - 1.0) * 86_400.0 + self.seconds_into_day()) / (dim * 86_400.0))
    }

    pub fn year_progress(&self) -> f32 {
        let diy = days_in_year(self.year) as f64;
        unit(((self.day_of_year() as f64 - 1.0) * 86_400.0 + self.seconds_into_day()) / (diy * 86_400.0))
    }
}

/// Converte para f32 mantendo o intervalo [0, 1): o arredondamento nunca chega a 1.0.
fn unit(x: f64) -> f32 {
    (x as f32).clamp(0.0, 0.999_999_94)
}

fn iso_weeks_in_year(year: i32) -> u8 {
    // O ano tem 53 semanas se 1º de janeiro for quinta, ou se for quarta em ano bissexto.
    let jan1 = weekday_of(year, 1, 1);
    if jan1 == 4 || (jan1 == 3 && is_leap(year)) {
        53
    } else {
        52
    }
}

/// Semanas de um mês começando no domingo: 0 = célula vazia, 1..=31 = dia. De 4 a 6 linhas.
pub fn month_grid(year: i32, month: u8) -> Vec<[u8; 7]> {
    let first = weekday_of(year, month, 1) as usize;
    let days = days_in_month(year, month);
    let mut rows: Vec<[u8; 7]> = Vec::new();
    let mut cell = first;
    for d in 1..=days {
        if rows.is_empty() || cell % 7 == 0 && cell != 0 && (cell / 7) >= rows.len() {
            rows.push([0; 7]);
        }
        rows[cell / 7][cell % 7] = d;
        cell += 1;
    }
    rows
}

/// Nomes em português do Brasil.
pub mod ptbr {
    const WEEKDAY_SHORT: [&str; 7] = ["dom", "seg", "ter", "qua", "qui", "sex", "s\u{e1}b"];
    const WEEKDAY_LONG: [&str; 7] = [
        "domingo",
        "segunda-feira",
        "ter\u{e7}a-feira",
        "quarta-feira",
        "quinta-feira",
        "sexta-feira",
        "s\u{e1}bado",
    ];
    const MONTH_SHORT: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];
    const MONTH_LONG: [&str; 12] = [
        "janeiro",
        "fevereiro",
        "mar\u{e7}o",
        "abril",
        "maio",
        "junho",
        "julho",
        "agosto",
        "setembro",
        "outubro",
        "novembro",
        "dezembro",
    ];

    pub fn weekday_short(wd: u8) -> &'static str {
        WEEKDAY_SHORT[(wd % 7) as usize]
    }
    pub fn weekday_long(wd: u8) -> &'static str {
        WEEKDAY_LONG[(wd % 7) as usize]
    }
    pub fn month_short(m: u8) -> &'static str {
        MONTH_SHORT[((m + 11) % 12) as usize]
    }
    pub fn month_long(m: u8) -> &'static str {
        MONTH_LONG[((m + 11) % 12) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(year: i32, month: u8, day: u8, hour: u8, minute: u8, second: u8, millis: u16) -> TimeInfo {
        TimeInfo { year, month, day, weekday: weekday_of(year, month, day), hour, minute, second, millis }
    }

    #[test]
    fn leap_years_follow_the_gregorian_rules() {
        assert!(is_leap(2024) && is_leap(2000));
        assert!(!is_leap(1900) && !is_leap(2100) && !is_leap(2026));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 12), 31);
        assert_eq!(days_in_month(2026, 13), 0);
    }

    #[test]
    fn known_dates_have_known_weekdays_and_day_of_year() {
        assert_eq!(weekday_of(2026, 10, 18), 0, "18/10/2026 é domingo");
        assert_eq!(weekday_of(2024, 2, 29), 4, "29/02/2024 foi quinta");
        assert_eq!(weekday_of(2000, 1, 1), 6, "01/01/2000 foi sábado");
        assert_eq!(t(2026, 10, 18, 0, 0, 0, 0).day_of_year(), 291);
        assert_eq!(t(2024, 12, 31, 0, 0, 0, 0).day_of_year(), 366);
        assert_eq!(t(2026, 1, 1, 0, 0, 0, 0).day_of_year(), 1);
    }

    #[test]
    fn validity_rejects_impossible_instants() {
        assert!(t(2026, 10, 18, 23, 59, 59, 999).is_valid());
        assert!(!TimeInfo { weekday: 3, ..t(2026, 10, 18, 0, 0, 0, 0) }.is_valid(), "dia da semana incoerente");
        assert!(!TimeInfo { day: 31, ..t(2026, 4, 1, 0, 0, 0, 0) }.is_valid(), "abril não tem 31");
        assert!(!TimeInfo { hour: 24, ..t(2026, 4, 1, 0, 0, 0, 0) }.is_valid());
        assert!(!TimeInfo { millis: 1000, ..t(2026, 4, 1, 0, 0, 0, 0) }.is_valid());
    }

    #[test]
    fn next_boundary_is_always_positive_and_exact() {
        assert_eq!(t(2026, 10, 18, 10, 20, 30, 250).ms_until_next(Granularity::Minute), 29_750);
        assert_eq!(t(2026, 10, 18, 10, 20, 0, 0).ms_until_next(Granularity::Minute), 60_000, "nunca 0");
        assert_eq!(t(2026, 10, 18, 23, 59, 59, 999).ms_until_next(Granularity::Minute), 1);
        assert_eq!(t(2026, 10, 18, 10, 20, 30, 0).ms_until_next(Granularity::Second), 1000);
        assert_eq!(t(2026, 10, 18, 10, 20, 30, 999).ms_until_next(Granularity::Second), 1);
        for ms in [0u16, 1, 500, 999] {
            for s in [0u8, 1, 30, 59] {
                let x = t(2026, 1, 1, 0, 0, s, ms);
                assert!((1..=60_000).contains(&x.ms_until_next(Granularity::Minute)));
                assert!((1..=1000).contains(&x.ms_until_next(Granularity::Second)));
            }
        }
    }

    #[test]
    fn iso_weeks_match_published_values() {
        assert_eq!(t(2026, 1, 1, 0, 0, 0, 0).iso_week(), 1, "quinta: semana 1");
        assert_eq!(t(2024, 12, 30, 0, 0, 0, 0).iso_week(), 1, "pertence à semana 1 de 2025");
        assert_eq!(t(2021, 1, 3, 0, 0, 0, 0).iso_week(), 53, "pertence à semana 53 de 2020");
        assert_eq!(t(2026, 10, 5, 0, 0, 0, 0).iso_week(), 41);
        assert_eq!(t(2026, 12, 31, 0, 0, 0, 0).iso_week(), 53, "2026 tem 53 semanas ISO");
        assert_eq!(t(2025, 12, 31, 0, 0, 0, 0).iso_week(), 1);
    }

    #[test]
    fn progress_fractions_hit_the_obvious_points() {
        assert_eq!(t(2026, 10, 18, 0, 0, 0, 0).day_progress(), 0.0);
        assert!((t(2026, 10, 18, 12, 0, 0, 0).day_progress() - 0.5).abs() < 1e-6);
        assert!(t(2026, 10, 18, 23, 59, 59, 999).day_progress() < 1.0);

        // Domingo 00:00 abre a semana de início domingo; segunda 00:00 abre a de início segunda.
        assert_eq!(t(2026, 10, 18, 0, 0, 0, 0).week_progress(WeekStart::Sunday), 0.0);
        assert_eq!(t(2026, 10, 19, 0, 0, 0, 0).week_progress(WeekStart::Monday), 0.0);
        assert!((t(2026, 10, 18, 0, 0, 0, 0).week_progress(WeekStart::Monday) - 6.0 / 7.0).abs() < 1e-6);
        assert!((t(2026, 10, 21, 12, 0, 0, 0).week_progress(WeekStart::Sunday) - 3.5 / 7.0).abs() < 1e-6);

        // Fevereiro bissexto tem 29 dias: o dia 15 ao meio-dia está em 14,5/29.
        assert!((t(2024, 2, 15, 12, 0, 0, 0).month_progress() - 14.5 / 29.0).abs() < 1e-6);
        assert_eq!(t(2026, 10, 1, 0, 0, 0, 0).month_progress(), 0.0);

        assert_eq!(t(2026, 1, 1, 0, 0, 0, 0).year_progress(), 0.0);
        assert!((t(2026, 10, 18, 12, 0, 0, 0).year_progress() - 290.5 / 365.0).abs() < 1e-6);
        assert!((t(2024, 7, 2, 0, 0, 0, 0).year_progress() - 183.0 / 366.0).abs() < 1e-6, "2024 tem 366 dias");
    }

    #[test]
    fn all_progress_values_stay_in_unit_range_over_a_whole_year() {
        let mut checked = 0;
        for month in 1..=12u8 {
            for day in 1..=days_in_month(2024, month) {
                for hour in [0u8, 7, 23] {
                    let x = t(2024, month, day, hour, 59, 59, 999);
                    assert!(x.is_valid());
                    for p in [x.day_progress(), x.week_progress(WeekStart::Sunday), x.week_progress(WeekStart::Monday), x.month_progress(), x.year_progress()] {
                        assert!((0.0..1.0).contains(&p), "{x:?} -> {p}");
                    }
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 366 * 3);
    }

    #[test]
    fn month_grids_place_days_under_the_right_weekday() {
        // Outubro de 2026 começa numa quinta-feira (índice 4) e tem 31 dias: 5 linhas.
        let g = month_grid(2026, 10);
        assert_eq!(g.len(), 5);
        assert_eq!(g[0], [0, 0, 0, 0, 1, 2, 3]);
        assert_eq!(g[4], [25, 26, 27, 28, 29, 30, 31]);
        // Fevereiro de 2026 começa no domingo e tem 28 dias: exatamente 4 linhas cheias.
        let f = month_grid(2026, 2);
        assert_eq!(f.len(), 4);
        assert_eq!(f[0][0], 1);
        assert_eq!(f[3][6], 28);
        // O pior caso tem 6 linhas (31 dias começando na sexta ou sábado).
        assert_eq!(month_grid(2026, 8).len(), 6);
        // Todo dia aparece exatamente uma vez, em ordem.
        for (y, m) in [(2024, 2), (2026, 8), (2026, 11), (2027, 5)] {
            let flat: Vec<u8> = month_grid(y, m).iter().flatten().copied().filter(|d| *d != 0).collect();
            assert_eq!(flat, (1..=days_in_month(y, m)).collect::<Vec<_>>(), "{y}-{m}");
            for row in month_grid(y, m) {
                assert!(row.iter().any(|d| *d != 0), "sem linha vazia");
            }
        }
    }

    #[test]
    fn portuguese_names() {
        assert_eq!(ptbr::weekday_short(0), "dom");
        assert_eq!(ptbr::weekday_short(6), "s\u{e1}b");
        assert_eq!(ptbr::weekday_long(2), "ter\u{e7}a-feira");
        assert_eq!(ptbr::month_short(1), "jan");
        assert_eq!(ptbr::month_short(10), "out");
        assert_eq!(ptbr::month_long(3), "mar\u{e7}o");
        assert_eq!(ptbr::month_long(12), "dezembro");
    }
}
