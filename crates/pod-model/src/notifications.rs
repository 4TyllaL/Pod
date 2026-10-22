//! Serviço `Notifications` (ADR-0001, W4): as notificações do Windows (toasts do Centro de
//! Notificações) que o widget Notifications mostra. O acesso exige a permissão do sistema
//! ("Acesso a notificações"); aqui ficam os tipos e as regras puras, e o app lê o Windows numa
//! thread própria.
//!
//! Privacidade: só o aplicativo e o título viram texto do cartão; o corpo da mensagem não é entregue ao
//! widget. Nada é gravado em disco.

/// Estado da permissão de acesso às notificações.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifAccess {
    /// Ainda não perguntou ao Windows.
    Unknown,
    Allowed,
    /// O usuário recusou (ou o Windows negou): só ele muda isso, em Configurações.
    Denied,
    /// O Windows não oferece o acesso a este processo (por exemplo, app sem identidade de pacote).
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifItem {
    /// Nome do aplicativo de origem ("Slack", "Mail").
    pub app: String,
    /// Primeira linha do toast (título), já limitada.
    pub title: String,
    /// Segundos desde que chegou.
    pub age_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifSnapshot {
    pub access: NotifAccess,
    /// Do mais novo para o mais antigo, no máximo `MAX_ITEMS`.
    pub items: Vec<NotifItem>,
    /// Total de notificações no Centro de Notificações (pode ser maior que `items`).
    pub total: usize,
}

pub const MAX_ITEMS: usize = 5;
/// Tamanho máximo, em caracteres, do título entregue ao widget.
pub const MAX_TITLE_CHARS: usize = 80;

impl NotifSnapshot {
    pub fn without_access(access: NotifAccess) -> Self {
        Self { access, items: Vec::new(), total: 0 }
    }

    /// Monta o resumo a partir de `(app, título, segundos desde a chegada)` em qualquer ordem.
    pub fn from_raw(mut raw: Vec<(String, String, u64)>) -> Self {
        let total = raw.len();
        raw.sort_by(|a, b| a.2.cmp(&b.2));
        let items = raw
            .into_iter()
            .take(MAX_ITEMS)
            .map(|(app, title, age_secs)| NotifItem { app: clean(&app, 40), title: clean(&title, MAX_TITLE_CHARS), age_secs })
            .collect();
        Self { access: NotifAccess::Allowed, items, total }
    }
}

/// Uma linha só, sem espaços repetidos, cortada por caracteres (com reticências quando corta).
pub fn clean(s: &str, max: usize) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= max {
        return one;
    }
    let mut out: String = one.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

/// Mensagem do cartão para cada estado sem lista: (linha principal, dica).
pub fn access_message(a: NotifAccess) -> (&'static str, &'static str) {
    match a {
        NotifAccess::Unknown => ("Lendo\u{2026}", ""),
        NotifAccess::Allowed => ("Sem notifica\u{e7}\u{f5}es", "Tudo em dia"),
        NotifAccess::Denied => ("Acesso negado", "Permita em Configura\u{e7}\u{f5}es > Privacidade > Notifica\u{e7}\u{f5}es"),
        NotifAccess::Unavailable => ("Indispon\u{ed}vel", "O Windows n\u{e3}o libera notifica\u{e7}\u{f5}es a este modo de execu\u{e7}\u{e3}o"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(app: &str, title: &str, age: u64) -> (String, String, u64) {
        (app.into(), title.into(), age)
    }

    #[test]
    fn newest_first_capped_and_total_counted() {
        let list: Vec<_> = (0..9).map(|i| raw("App", &format!("t{i}"), (9 - i) * 60)).collect();
        let s = NotifSnapshot::from_raw(list);
        assert_eq!(s.total, 9);
        assert_eq!(s.items.len(), MAX_ITEMS);
        assert_eq!(s.items[0].title, "t8", "o mais novo primeiro");
        assert!(s.items.windows(2).all(|w| w[0].age_secs <= w[1].age_secs));
        assert_eq!(s.access, NotifAccess::Allowed);
    }

    #[test]
    fn text_is_one_line_and_limited_by_characters() {
        assert_eq!(clean("  Ol\u{e1}\n\n  mundo \t!  ", 40), "Ol\u{e1} mundo !");
        let long = "\u{e7}".repeat(200);
        let c = clean(&long, MAX_TITLE_CHARS);
        assert_eq!(c.chars().count(), MAX_TITLE_CHARS);
        assert!(c.ends_with('\u{2026}'));
        assert_eq!(clean("", 10), "");
    }

    #[test]
    fn states_without_a_list_have_a_message() {
        for a in [NotifAccess::Unknown, NotifAccess::Allowed, NotifAccess::Denied, NotifAccess::Unavailable] {
            assert!(!access_message(a).0.is_empty());
        }
        assert!(access_message(NotifAccess::Denied).1.contains("Configura"));
        let s = NotifSnapshot::without_access(NotifAccess::Denied);
        assert!(s.items.is_empty() && s.total == 0);
    }
}
