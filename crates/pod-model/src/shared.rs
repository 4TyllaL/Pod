//! Dados compartilhados entre widgets (ADR-0001, serviço `Shared`): eventos de calendário e
//! tarefas. Puro: validação, limites e regras; o shell guarda e persiste o JSON em `extra["shared"]`.

use serde::{Deserialize, Serialize};

use crate::time::{days_from_civil, days_in_month, TimeInfo};

pub const MAX_EVENTS: usize = 200;
pub const MAX_TASKS: usize = 200;
pub const MAX_TITLE_CHARS: usize = 80;
pub const MAX_LINKS: usize = 30;
pub const MAX_PARCELS: usize = 50;
pub const MAX_URL_CHARS: usize = 300;
pub const MIN_YEAR: i32 = 1900;
pub const MAX_YEAR: i32 = 2200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Date {
    pub fn new(year: i32, month: u8, day: u8) -> Option<Date> {
        let ok = (MIN_YEAR..=MAX_YEAR).contains(&year) && (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month);
        ok.then_some(Date { year, month, day })
    }

    pub fn from_time(t: &TimeInfo) -> Date {
        Date { year: t.year, month: t.month, day: t.day }
    }

    /// "18/10/2026" ou "8/1/2026".
    pub fn parse(s: &str) -> Option<Date> {
        let mut it = s.trim().split('/');
        let (d, m, y) = (it.next()?, it.next()?, it.next()?);
        if it.next().is_some() || d.is_empty() || m.is_empty() || y.len() != 4 {
            return None;
        }
        if ![d, m, y].iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())) {
            return None;
        }
        Date::new(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)
    }

    pub fn format(&self) -> String {
        format!("{:02}/{:02}/{:04}", self.day, self.month, self.year)
    }

    pub fn days(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    /// Dias de `self` até `other` (negativo se `other` é anterior).
    pub fn days_until(&self, other: Date) -> i64 {
        other.days() - self.days()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalEvent {
    pub id: u64,
    pub date: Date,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub title: String,
    pub done: bool,
}

/// Link salvo (Favorites). Só http/https; o host abre o endereço, nunca o widget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub id: u64,
    pub title: String,
    pub url: String,
}

/// Encomenda (Parcels): só guarda o código de rastreio e se já chegou; não consulta nada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parcel {
    pub id: u64,
    pub label: String,
    pub code: String,
    pub delivered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedError {
    EmptyTitle,
    TitleTooLong,
    BadChars,
    BadDate,
    BadUrl,
    BadCode,
    Full,
    Invalid(String),
}

impl std::fmt::Display for SharedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTitle => write!(f, "Digite um t\u{ed}tulo."),
            Self::TitleTooLong => write!(f, "T\u{ed}tulo longo demais (m\u{e1}ximo {MAX_TITLE_CHARS} caracteres)."),
            Self::BadChars => write!(f, "O t\u{ed}tulo tem caracteres de controle."),
            Self::BadDate => write!(f, "Data inv\u{e1}lida. Use dia/m\u{ea}s/ano, por exemplo 18/10/2026."),
            Self::BadUrl => write!(f, "Endere\u{e7}o inv\u{e1}lido. Use http:// ou https:// e um site, sem usu\u{e1}rio nem senha."),
            Self::BadCode => write!(f, "C\u{f3}digo inv\u{e1}lido. Use de 8 a 30 letras e n\u{fa}meros."),
            Self::Full => write!(f, "Limite de itens atingido."),
            Self::Invalid(e) => write!(f, "{e}"),
        }
    }
}

/// Operações que um widget pode pedir sem abrir o painel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedOp {
    ToggleTask(u64),
    ToggleParcel(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SharedData {
    next_id: u64,
    events: Vec<CalEvent>,
    tasks: Vec<Task>,
    #[serde(default)]
    links: Vec<Link>,
    #[serde(default)]
    parcels: Vec<Parcel>,
}

fn clean_title(raw: &str) -> Result<String, SharedError> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(SharedError::EmptyTitle);
    }
    if t.chars().any(char::is_control) {
        return Err(SharedError::BadChars);
    }
    if t.chars().count() > MAX_TITLE_CHARS {
        return Err(SharedError::TitleTooLong);
    }
    Ok(t.to_string())
}

/// Valida e normaliza um endereço: sem esquema vira https://; só http(s); sem espaços, sem
/// usuário/senha, com host.
pub fn clean_url(raw: &str) -> Result<String, SharedError> {
    let t = raw.trim();
    if t.is_empty() || t.chars().count() > MAX_URL_CHARS || t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(SharedError::BadUrl);
    }
    let lower = t.to_ascii_lowercase();
    let full = if lower.starts_with("http://") || lower.starts_with("https://") {
        t.to_string()
    } else if t.contains("://") || t.contains(':') && !t.contains('.') {
        return Err(SharedError::BadUrl); // outro esquema (file:, javascript:, ...)
    } else {
        format!("https://{t}")
    };
    let rest = &full[full.find("://").map_or(0, |i| i + 3)..];
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return Err(SharedError::BadUrl);
    }
    let host = authority.rsplit_once(':').map_or(authority, |(h, port)| if port.chars().all(|c| c.is_ascii_digit()) { h } else { authority });
    let host_ok = !host.is_empty()
        && host.contains('.')
        && !host.starts_with(['.', '-'])
        && !host.ends_with(['.', '-'])
        && host.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-');
    if !host_ok {
        return Err(SharedError::BadUrl);
    }
    Ok(full)
}

/// Normaliza um código de rastreio: maiúsculas, sem espaços, 8 a 30 letras/números ASCII.
pub fn clean_code(raw: &str) -> Result<String, SharedError> {
    let c: String = raw.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    if (8..=30).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Ok(c)
    } else {
        Err(SharedError::BadCode)
    }
}

impl SharedData {
    fn take_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn events(&self) -> &[CalEvent] {
        &self.events
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn add_event(&mut self, date: Date, title: &str) -> Result<u64, SharedError> {
        let title = clean_title(title)?;
        if self.events.len() >= MAX_EVENTS {
            return Err(SharedError::Full);
        }
        let id = self.take_id();
        self.events.push(CalEvent { id, date, title });
        self.events.sort_by(|a, b| (a.date, a.id).cmp(&(b.date, b.id)));
        Ok(id)
    }

    pub fn remove_event(&mut self, id: u64) -> bool {
        let n = self.events.len();
        self.events.retain(|e| e.id != id);
        self.events.len() != n
    }

    pub fn add_task(&mut self, title: &str) -> Result<u64, SharedError> {
        let title = clean_title(title)?;
        if self.tasks.len() >= MAX_TASKS {
            return Err(SharedError::Full);
        }
        let id = self.take_id();
        self.tasks.push(Task { id, title, done: false });
        Ok(id)
    }

    pub fn toggle_task(&mut self, id: u64) -> bool {
        match self.tasks.iter_mut().find(|t| t.id == id) {
            Some(t) => {
                t.done = !t.done;
                true
            }
            None => false,
        }
    }

    pub fn remove_task(&mut self, id: u64) -> bool {
        let n = self.tasks.len();
        self.tasks.retain(|t| t.id != id);
        self.tasks.len() != n
    }

    pub fn apply(&mut self, op: SharedOp) -> bool {
        match op {
            SharedOp::ToggleTask(id) => self.toggle_task(id),
            SharedOp::ToggleParcel(id) => self.toggle_parcel(id),
        }
    }

    pub fn links(&self) -> &[Link] {
        &self.links
    }

    pub fn parcels(&self) -> &[Parcel] {
        &self.parcels
    }

    pub fn add_link(&mut self, title: &str, url: &str) -> Result<u64, SharedError> {
        let title = clean_title(title)?;
        let url = clean_url(url)?;
        if self.links.len() >= MAX_LINKS {
            return Err(SharedError::Full);
        }
        let id = self.take_id();
        self.links.push(Link { id, title, url });
        Ok(id)
    }

    pub fn remove_link(&mut self, id: u64) -> bool {
        let n = self.links.len();
        self.links.retain(|l| l.id != id);
        self.links.len() != n
    }

    pub fn link(&self, id: u64) -> Option<&Link> {
        self.links.iter().find(|l| l.id == id)
    }

    pub fn add_parcel(&mut self, label: &str, code: &str) -> Result<u64, SharedError> {
        let label = clean_title(label)?;
        let code = clean_code(code)?;
        if self.parcels.len() >= MAX_PARCELS {
            return Err(SharedError::Full);
        }
        let id = self.take_id();
        self.parcels.push(Parcel { id, label, code, delivered: false });
        Ok(id)
    }

    pub fn toggle_parcel(&mut self, id: u64) -> bool {
        match self.parcels.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                p.delivered = !p.delivered;
                true
            }
            None => false,
        }
    }

    pub fn remove_parcel(&mut self, id: u64) -> bool {
        let n = self.parcels.len();
        self.parcels.retain(|p| p.id != id);
        self.parcels.len() != n
    }

    pub fn pending_parcels(&self) -> usize {
        self.parcels.iter().filter(|p| !p.delivered).count()
    }

    pub fn pending_tasks(&self) -> usize {
        self.tasks.iter().filter(|t| !t.done).count()
    }

    /// Primeiro evento em `from` ou depois (os eventos já ficam ordenados por data).
    pub fn next_event(&self, from: Date) -> Option<&CalEvent> {
        self.events.iter().find(|e| e.date >= from)
    }

    pub fn events_on(&self, date: Date) -> impl Iterator<Item = &CalEvent> {
        self.events.iter().filter(move |e| e.date == date)
    }

    pub fn event(&self, id: u64) -> Option<&CalEvent> {
        self.events.iter().find(|e| e.id == id)
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    /// Lê e valida: limites, ids únicos e menores que `next_id`, datas e títulos válidos.
    pub fn from_json(v: &serde_json::Value) -> Result<Self, SharedError> {
        let d: SharedData = serde_json::from_value(v.clone()).map_err(|e| SharedError::Invalid(e.to_string()))?;
        if d.events.len() > MAX_EVENTS || d.tasks.len() > MAX_TASKS || d.links.len() > MAX_LINKS || d.parcels.len() > MAX_PARCELS {
            return Err(SharedError::Full);
        }
        let mut ids: Vec<u64> = d
            .events
            .iter()
            .map(|e| e.id)
            .chain(d.tasks.iter().map(|t| t.id))
            .chain(d.links.iter().map(|l| l.id))
            .chain(d.parcels.iter().map(|p| p.id))
            .collect();
        ids.sort_unstable();
        if ids.windows(2).any(|w| w[0] == w[1]) || ids.last().is_some_and(|m| *m > d.next_id) {
            return Err(SharedError::Invalid("ids repetidos ou fora de ordem".into()));
        }
        for e in &d.events {
            if Date::new(e.date.year, e.date.month, e.date.day).is_none() {
                return Err(SharedError::BadDate);
            }
            clean_title(&e.title)?;
        }
        for t in &d.tasks {
            clean_title(&t.title)?;
        }
        for l in &d.links {
            clean_title(&l.title)?;
            if clean_url(&l.url).as_deref() != Ok(l.url.as_str()) {
                return Err(SharedError::BadUrl);
            }
        }
        for p in &d.parcels {
            clean_title(&p.label)?;
            if clean_code(&p.code).as_deref() != Ok(p.code.as_str()) {
                return Err(SharedError::BadCode);
            }
        }
        let mut d = d;
        d.events.sort_by(|a, b| (a.date, a.id).cmp(&(b.date, b.id)));
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).unwrap()
    }

    #[test]
    fn dates_parse_format_and_reject() {
        assert_eq!(Date::parse("18/10/2026"), Some(d(2026, 10, 18)));
        assert_eq!(Date::parse(" 8/1/2026 "), Some(d(2026, 1, 8)));
        assert_eq!(d(2026, 1, 8).format(), "08/01/2026");
        for bad in ["", "18/10", "18/10/26", "31/04/2026", "29/02/2026", "00/10/2026", "18/13/2026", "a/b/cccc", "18/10/2026/1", "18-10-2026", "18/10/1899", "18/10/2201", "+1/10/2026"] {
            assert_eq!(Date::parse(bad), None, "{bad:?}");
        }
        assert!(Date::parse("29/02/2028").is_some(), "2028 é bissexto");
    }

    #[test]
    fn day_arithmetic() {
        assert_eq!(d(2026, 10, 18).days(), 20_744);
        assert_eq!(d(2026, 10, 18).days_until(d(2027, 1, 1)), 75);
        assert_eq!(d(2024, 2, 28).days_until(d(2024, 3, 1)), 2);
        assert_eq!(d(2026, 10, 18).days_until(d(2026, 10, 18)), 0);
        assert_eq!(d(2026, 10, 18).days_until(d(2026, 10, 1)), -17);
    }

    #[test]
    fn events_stay_sorted_and_next_event_is_found() {
        let mut s = SharedData::default();
        let late = s.add_event(d(2026, 12, 25), "Natal").unwrap();
        let soon = s.add_event(d(2026, 10, 20), "  Reuni\u{e3}o  ").unwrap();
        assert_eq!(s.events()[0].title, "Reuni\u{e3}o", "título sem espaços nas pontas");
        assert_eq!(s.next_event(d(2026, 10, 18)).map(|e| e.id), Some(soon));
        assert_eq!(s.next_event(d(2026, 10, 20)).map(|e| e.id), Some(soon), "hoje conta");
        assert_eq!(s.next_event(d(2026, 10, 21)).map(|e| e.id), Some(late));
        assert_eq!(s.next_event(d(2027, 1, 1)), None);
        assert_eq!(s.events_on(d(2026, 12, 25)).count(), 1);
        assert!(s.remove_event(soon) && !s.remove_event(soon));
    }

    #[test]
    fn titles_are_validated() {
        let mut s = SharedData::default();
        assert_eq!(s.add_task("   "), Err(SharedError::EmptyTitle));
        assert_eq!(s.add_task("a\nb"), Err(SharedError::BadChars));
        assert_eq!(s.add_task(&"x".repeat(MAX_TITLE_CHARS + 1)), Err(SharedError::TitleTooLong));
        assert!(s.add_task(&"\u{e7}".repeat(MAX_TITLE_CHARS)).is_ok(), "o limite conta caracteres, não bytes");
        assert!(s.tasks().len() == 1);
    }

    #[test]
    fn tasks_toggle_and_count() {
        let mut s = SharedData::default();
        let a = s.add_task("A").unwrap();
        let b = s.add_task("B").unwrap();
        assert_eq!(s.pending_tasks(), 2);
        assert!(s.apply(SharedOp::ToggleTask(a)));
        assert_eq!(s.pending_tasks(), 1);
        assert!(s.apply(SharedOp::ToggleTask(a)), "alternar de novo desfaz");
        assert!(!s.apply(SharedOp::ToggleTask(999)));
        assert!(s.remove_task(b) && !s.remove_task(b));
    }

    #[test]
    fn limits_are_enforced_and_ids_are_never_reused() {
        let mut s = SharedData::default();
        for i in 0..MAX_TASKS {
            s.add_task(&format!("t{i}")).unwrap();
        }
        assert_eq!(s.add_task("a mais"), Err(SharedError::Full));
        let first = s.tasks()[0].id;
        s.remove_task(first);
        let again = s.add_task("novo").unwrap();
        assert!(again > first && again > s.tasks()[1].id, "id novo nunca repete um antigo");
    }

    #[test]
    fn urls_are_normalized_and_dangerous_ones_rejected() {
        assert_eq!(clean_url("https://exemplo.com.br/a?b=1#c").as_deref(), Ok("https://exemplo.com.br/a?b=1#c"));
        assert_eq!(clean_url("  exemplo.com  ").as_deref(), Ok("https://exemplo.com"), "sem esquema vira https");
        assert_eq!(clean_url("HTTP://Exemplo.com:8080/x").as_deref(), Ok("HTTP://Exemplo.com:8080/x"));
        for bad in [
            "",
            "   ",
            "javascript:alert(1)",
            "file:///C:/Windows/system32/calc.exe",
            "ftp://exemplo.com",
            "https://usuario:senha@exemplo.com",
            "https://usuario@exemplo.com",
            "https://",
            "https:///caminho",
            "https://localhost",
            "exemplo",
            "https://exemplo .com",
            "https://-exemplo.com",
            "https://exemplo.com/a\nb",
            "mailto:a@b.com",
            "C:\\Windows\\calc.exe",
            "calc.exe\" --flag",
        ] {
            assert_eq!(clean_url(bad), Err(SharedError::BadUrl), "{bad:?}");
        }
        assert_eq!(clean_url(&format!("https://exemplo.com/{}", "a".repeat(MAX_URL_CHARS))), Err(SharedError::BadUrl));
    }

    #[test]
    fn codes_are_normalized() {
        assert_eq!(clean_code(" aa 123 456 789 br ").as_deref(), Ok("AA123456789BR"));
        for bad in ["", "ABC123", "AB-123456789", &"A".repeat(31), "AA12345\u{e7}89"] {
            assert_eq!(clean_code(bad), Err(SharedError::BadCode), "{bad:?}");
        }
    }

    #[test]
    fn links_and_parcels_follow_the_same_rules_as_tasks() {
        let mut s = SharedData::default();
        let l = s.add_link("Meu site", "exemplo.com").unwrap();
        assert_eq!(s.link(l).map(|x| x.url.as_str()), Some("https://exemplo.com"));
        assert_eq!(s.add_link("", "exemplo.com"), Err(SharedError::EmptyTitle));
        assert_eq!(s.add_link("x", "javascript:1"), Err(SharedError::BadUrl));
        let p = s.add_parcel("Fone novo", "aa123456789br").unwrap();
        assert_eq!(s.parcels()[0].code, "AA123456789BR");
        assert_eq!(s.pending_parcels(), 1);
        assert!(s.apply(SharedOp::ToggleParcel(p)));
        assert_eq!(s.pending_parcels(), 0);
        assert!(!s.apply(SharedOp::ToggleParcel(999)));
        assert!(s.remove_link(l) && !s.remove_link(l));
        assert!(s.remove_parcel(p) && !s.remove_parcel(p));
        for i in 0..MAX_LINKS {
            s.add_link("x", &format!("site{i}.com")).unwrap();
        }
        assert_eq!(s.add_link("x", "mais.com"), Err(SharedError::Full));
        // O roundtrip preserva tudo e arquivos antigos (sem os campos novos) continuam válidos.
        assert_eq!(SharedData::from_json(&s.to_json()).unwrap(), s);
        assert!(SharedData::from_json(&serde_json::json!({"next_id": 0, "events": [], "tasks": []})).is_ok());
        let bad_link = serde_json::json!({"next_id": 1, "events": [], "tasks": [], "links": [{"id": 1, "title": "x", "url": "javascript:1"}]});
        assert!(SharedData::from_json(&bad_link).is_err(), "link inválido no arquivo é recusado");
        let bad_code = serde_json::json!({"next_id": 1, "events": [], "tasks": [], "parcels": [{"id": 1, "label": "x", "code": "abc", "delivered": false}]});
        assert!(SharedData::from_json(&bad_code).is_err());
    }

    #[test]
    fn json_roundtrip_and_validation() {
        let mut s = SharedData::default();
        s.add_event(d(2026, 10, 20), "Reuni\u{e3}o").unwrap();
        let t = s.add_task("Comprar p\u{e3}o").unwrap();
        s.toggle_task(t);
        let back = SharedData::from_json(&s.to_json()).unwrap();
        assert_eq!(back, s);
        let bad = [
            serde_json::json!({"next_id": 1, "events": [], "tasks": [{"id": 5, "title": "x", "done": false}]}),
            serde_json::json!({"next_id": 2, "events": [], "tasks": [{"id": 1, "title": "x", "done": false}, {"id": 1, "title": "y", "done": true}]}),
            serde_json::json!({"next_id": 1, "events": [{"id": 1, "date": {"year": 2026, "month": 2, "day": 31}, "title": "x"}], "tasks": []}),
            serde_json::json!({"next_id": 1, "events": [], "tasks": [{"id": 1, "title": "", "done": false}]}),
            serde_json::json!("lixo"),
        ];
        for b in bad {
            assert!(SharedData::from_json(&b).is_err(), "{b}");
        }
        assert_eq!(SharedData::from_json(&serde_json::json!({"next_id": 0, "events": [], "tasks": []})).unwrap(), SharedData::default());
    }
}
