//! Relógios do mundo: cidades conhecidas (nome e chave de fuso do Windows) e a hora lá, já convertida
//! pelo host. O modelo só conhece nomes, chaves e o formato do que mostrar.

/// (id estável, nome, chave de fuso do Windows)
pub const CITIES: [(&str, &str, &str); 20] = [
    ("brasilia", "Bras\u{ed}lia", "E. South America Standard Time"),
    ("manaus", "Manaus", "SA Western Standard Time"),
    ("lisboa", "Lisboa", "GMT Standard Time"),
    ("londres", "Londres", "GMT Standard Time"),
    ("paris", "Paris", "Romance Standard Time"),
    ("berlim", "Berlim", "W. Europe Standard Time"),
    ("moscou", "Moscou", "Russian Standard Time"),
    ("dubai", "Dubai", "Arabian Standard Time"),
    ("mumbai", "Mumbai", "India Standard Time"),
    ("pequim", "Pequim", "China Standard Time"),
    ("toquio", "T\u{f3}quio", "Tokyo Standard Time"),
    ("seul", "Seul", "Korea Standard Time"),
    ("sydney", "Sydney", "AUS Eastern Standard Time"),
    ("auckland", "Auckland", "New Zealand Standard Time"),
    ("nova-york", "Nova York", "Eastern Standard Time"),
    ("chicago", "Chicago", "Central Standard Time"),
    ("denver", "Denver", "Mountain Standard Time"),
    ("los-angeles", "Los Angeles", "Pacific Standard Time"),
    ("mexico", "Cidade do M\u{e9}xico", "Central Standard Time (Mexico)"),
    ("buenos-aires", "Buenos Aires", "Argentina Standard Time"),
];

pub const DEFAULT: [&str; 3] = ["nova-york", "londres", "toquio"];
pub const MAX_SELECTED: usize = 4;

pub fn city(id: &str) -> Option<(&'static str, &'static str, &'static str)> {
    CITIES.iter().copied().find(|c| c.0 == id)
}

/// Hora em outro fuso: horas, minutos e a diferença de dia em relação ao dia local (−1, 0, +1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneTime {
    pub hour: u8,
    pub minute: u8,
    pub day_diff: i8,
}

impl ZoneTime {
    pub fn text(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// "ontem", "hoje" ou "amanh\u{e3}" em rela\u{e7}\u{e3}o ao dia do usu\u{e1}rio.
    pub fn day_word(&self) -> &'static str {
        match self.day_diff {
            d if d < 0 => "ontem",
            0 => "hoje",
            _ => "amanh\u{e3}",
        }
    }
}

/// Diferença de dia entre duas datas (hoje do fuso menos hoje local), recortada em −1..=1.
pub fn day_diff(local: (i32, u8, u8), zone: (i32, u8, u8)) -> i8 {
    let n = |d: (i32, u8, u8)| crate::usage::days_from_civil(d.0 as i64, d.1 as u32, d.2 as u32);
    (n(zone) - n(local)).clamp(-1, 1) as i8
}

/// Escolhe/retira uma cidade da lista mantendo o limite e a ordem de escolha.
pub fn toggle(list: &mut Vec<String>, id: &str) -> bool {
    if city(id).is_none() {
        return false;
    }
    if let Some(i) = list.iter().position(|c| c == id) {
        if list.len() > 1 {
            list.remove(i);
            return true;
        }
        return false;
    }
    if list.len() >= MAX_SELECTED {
        return false;
    }
    list.push(id.to_string());
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn city_ids_are_unique_and_known() {
        let mut ids: Vec<&str> = CITIES.iter().map(|c| c.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CITIES.len());
        assert!(DEFAULT.iter().all(|d| city(d).is_some()));
        assert_eq!(city("toquio").unwrap().1, "T\u{f3}quio");
        assert!(city("atlantida").is_none());
    }

    #[test]
    fn day_difference_is_clamped_and_crosses_months() {
        assert_eq!(day_diff((2026, 10, 31), (2026, 11, 1)), 1);
        assert_eq!(day_diff((2026, 1, 1), (2025, 12, 31)), -1);
        assert_eq!(day_diff((2026, 5, 5), (2026, 5, 5)), 0);
        assert_eq!(ZoneTime { hour: 3, minute: 5, day_diff: 1 }.text(), "03:05");
        assert_eq!(ZoneTime { hour: 3, minute: 5, day_diff: -1 }.day_word(), "ontem");
    }

    #[test]
    fn toggle_respects_the_limits() {
        let mut l: Vec<String> = DEFAULT.iter().map(|s| s.to_string()).collect();
        assert!(toggle(&mut l, "paris"));
        assert_eq!(l.len(), 4);
        assert!(!toggle(&mut l, "sydney"), "m\u{e1}ximo de cidades");
        assert!(toggle(&mut l, "paris") && l.len() == 3);
        assert!(!toggle(&mut l, "inexistente"));
        let mut one = vec!["paris".to_string()];
        assert!(!toggle(&mut one, "paris"), "sempre fica pelo menos uma");
    }
}
