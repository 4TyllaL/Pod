//! Hábitos diários: cada hábito guarda os dias (número de dias desde 1970) em que foi cumprido.
//! Tudo puro: marcar/desmarcar, sequência de dias seguidos e limites.

use serde::{Deserialize, Serialize};

pub const MAX_HABITS: usize = 12;
pub const MAX_NAME: usize = 30;
/// Dias guardados por hábito (o resto é descartado, o arquivo não cresce sem fim).
const KEEP_DAYS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Habit {
    pub name: String,
    /// Dias cumpridos, em ordem crescente e sem repetição.
    pub days: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddError {
    Empty,
    TooLong,
    Duplicate,
    Full,
}

impl AddError {
    pub fn message(&self) -> String {
        match self {
            AddError::Empty => "Digite o nome do h\u{e1}bito.".into(),
            AddError::TooLong => format!("No m\u{e1}ximo {MAX_NAME} letras."),
            AddError::Duplicate => "J\u{e1} existe um h\u{e1}bito com esse nome.".into(),
            AddError::Full => format!("No m\u{e1}ximo {MAX_HABITS} h\u{e1}bitos."),
        }
    }
}

/// Dia de hoje como número de dias desde 1970.
pub fn day_number(year: i32, month: u8, day: u8) -> i64 {
    crate::usage::days_from_civil(year as i64, month as u32, day as u32)
}

impl Habit {
    pub fn done_on(&self, day: i64) -> bool {
        self.days.binary_search(&day).is_ok()
    }

    /// Marca ou desmarca `day`; devolve o novo estado.
    pub fn toggle(&mut self, day: i64) -> bool {
        match self.days.binary_search(&day) {
            Ok(i) => {
                self.days.remove(i);
                false
            }
            Err(i) => {
                self.days.insert(i, day);
                if self.days.len() > KEEP_DAYS {
                    self.days.remove(0);
                }
                true
            }
        }
    }

    /// Dias seguidos até hoje. Se hoje ainda não foi marcado, a sequência de ontem continua valendo.
    pub fn streak(&self, today: i64) -> u32 {
        let mut d = if self.done_on(today) { today } else { today - 1 };
        let mut n = 0;
        while self.done_on(d) {
            n += 1;
            d -= 1;
        }
        n
    }
}

/// Nome aceito: sem espaços nas pontas nem repetidos no meio, sem caracteres de controle.
pub fn clean_name(s: &str) -> Result<String, AddError> {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.is_empty() || one.chars().any(|c| c.is_control()) {
        return Err(AddError::Empty);
    }
    if one.chars().count() > MAX_NAME {
        return Err(AddError::TooLong);
    }
    Ok(one)
}

pub fn add(list: &mut Vec<Habit>, name: &str) -> Result<(), AddError> {
    let name = clean_name(name)?;
    if list.len() >= MAX_HABITS {
        return Err(AddError::Full);
    }
    if list.iter().any(|h| h.name.to_lowercase() == name.to_lowercase()) {
        return Err(AddError::Duplicate);
    }
    list.push(Habit { name, days: Vec::new() });
    Ok(())
}

/// Quantos hábitos foram cumpridos hoje.
pub fn done_today(list: &[Habit], today: i64) -> usize {
    list.iter().filter(|h| h.done_on(today)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_marks_and_unmarks_and_keeps_order() {
        let mut h = Habit::default();
        assert!(h.toggle(10));
        assert!(h.toggle(8));
        assert_eq!(h.days, vec![8, 10]);
        assert!(!h.toggle(10));
        assert!(!h.done_on(10) && h.done_on(8));
    }

    #[test]
    fn streak_counts_consecutive_days_and_survives_an_unmarked_today() {
        let h = Habit { name: "x".into(), days: vec![3, 5, 6, 7] };
        assert_eq!(h.streak(7), 3);
        assert_eq!(h.streak(8), 3, "hoje ainda n\u{e3}o marcado: a sequ\u{ea}ncia de ontem vale");
        assert_eq!(h.streak(9), 0, "dois dias sem marcar zera");
        assert_eq!(Habit::default().streak(100), 0);
    }

    #[test]
    fn old_days_are_dropped() {
        let mut h = Habit::default();
        for d in 0..(KEEP_DAYS as i64 + 20) {
            h.toggle(d);
        }
        assert_eq!(h.days.len(), KEEP_DAYS);
        assert_eq!(*h.days.last().unwrap(), KEEP_DAYS as i64 + 19);
    }

    #[test]
    fn names_are_cleaned_and_limits_enforced() {
        assert_eq!(clean_name("  Ler   15  min "), Ok("Ler 15 min".to_string()));
        assert_eq!(clean_name("   "), Err(AddError::Empty));
        assert_eq!(clean_name(&"a".repeat(31)), Err(AddError::TooLong));
        let mut l = Vec::new();
        assert_eq!(add(&mut l, "Beber \u{e1}gua"), Ok(()));
        assert_eq!(add(&mut l, "beber \u{c1}GUA"), Err(AddError::Duplicate).or(Err(AddError::Duplicate)));
        for i in 1..MAX_HABITS {
            add(&mut l, &format!("h{i}")).unwrap();
        }
        assert_eq!(add(&mut l, "mais um"), Err(AddError::Full));
    }

    #[test]
    fn day_numbers_are_consecutive_across_months() {
        assert_eq!(day_number(2026, 11, 1), day_number(2026, 10, 31) + 1);
        assert_eq!(day_number(1970, 1, 1), 0);
    }
}
