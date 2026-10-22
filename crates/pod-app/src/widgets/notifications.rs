//! Widget Notifications: as notificações recentes do Centro de Notificações do Windows (aplicativo,
//! título e há quanto tempo), em 1×1, 2×1 e 2×2. Só leitura. Exige a permissão do Windows para
//! acessar notificações; sem ela o cartão explica o que fazer. O corpo das mensagens não é mostrado
//! e nada é guardado em disco: ao remover o widget a lista é esquecida.

use pod_model::ids::WidgetTypeId;
use pod_model::local::format_age;
use pod_model::notifications::{access_message, NotifAccess, NotifSnapshot};
use pod_model::shape::Shape;
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, Outcome, SizeDip, Snapshot, StopReason, ViewCtx, Widget,
    WidgetError,
};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.notifications");
pub const STATE_VERSION: u32 = 1;

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X2.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S4X1, Shape::S4X2],
        default_shape: Some(Shape::S2X2),
    }
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(Notifications { snap: NotifSnapshot::without_access(NotifAccess::Unknown), stopped: false }))
}

struct Notifications {
    snap: NotifSnapshot,
    stopped: bool,
}

fn text(s: &str, style: TextStyle, role: Role, align: Align, lines: u8) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align, max_lines: lines }).size(Len::Fill(1), Len::Auto)
}

fn count_text(s: &NotifSnapshot) -> String {
    if s.access == NotifAccess::Allowed {
        s.total.to_string()
    } else {
        "\u{2014}".to_string()
    }
}

fn count_label(total: usize) -> String {
    match total {
        0 => "Sem notifica\u{e7}\u{f5}es".to_string(),
        1 => "1 notifica\u{e7}\u{e3}o".to_string(),
        n => format!("{n} notifica\u{e7}\u{f5}es"),
    }
}

fn message_node(s: &NotifSnapshot, with_hint: bool) -> Node {
    let (main, hint) = access_message(s.access);
    let mut col = vec![text(main, TextStyle::Body, Role::Secondary, Align::Center, 1)];
    if with_hint && !hint.is_empty() {
        col.push(text(hint, TextStyle::Caption, Role::Tertiary, Align::Center, 3));
    }
    Node::column(4.0, 14.0, col).centered()
}

fn item_rows(s: &NotifSnapshot, n: usize, two_lines: bool) -> Vec<Node> {
    s.items
        .iter()
        .take(n)
        .map(|it| {
            let who = if it.app.is_empty() { format_age(it.age_secs) } else { format!("{} \u{b7} {}", it.app, format_age(it.age_secs)) };
            let title = if it.title.is_empty() { "(sem t\u{ed}tulo)" } else { it.title.as_str() };
            if two_lines {
                Node::column(1.0, 0.0, vec![
                    text(&who, TextStyle::Caption, Role::Secondary, Align::Start, 1),
                    text(title, TextStyle::Body, Role::Primary, Align::Start, 1),
                ])
                .size(Len::Fill(1), Len::Auto)
            } else {
                text(&format!("{}: {title}", if it.app.is_empty() { "?" } else { &it.app }), TextStyle::Caption, Role::Primary, Align::Start, 1)
            }
        })
        .collect()
}

fn view_for(shape: Shape, s: &NotifSnapshot) -> View {
    let has_list = s.access == NotifAccess::Allowed && !s.items.is_empty();
    let root = match shape {
        sh if sh == Shape::S1X1 => Node::column(2.0, 6.0, vec![
            text(&count_text(s), TextStyle::Headline, if s.total > 0 { Role::Accent } else { Role::Primary }, Align::Center, 1),
            text("NOTIFICA\u{c7}\u{d5}ES", TextStyle::Caption, Role::Secondary, Align::Center, 1),
        ])
        .centered(),
        sh if sh == Shape::S2X2 => {
            if !has_list {
                message_node(s, true)
            } else {
                let mut col = vec![
                    text("Notifica\u{e7}\u{f5}es", TextStyle::Title, Role::Primary, Align::Start, 1),
                    text(&count_label(s.total), TextStyle::Caption, Role::Secondary, Align::Start, 1),
                ];
                col.push(Node::divider());
                col.extend(item_rows(s, 3, true));
                Node::column(6.0, 14.0, col)
            }
        }
        _ => {
            if !has_list {
                message_node(s, false)
            } else {
                let mut col = vec![text(&count_label(s.total), TextStyle::Body, Role::Primary, Align::Start, 1)];
                col.extend(item_rows(s, 2, false));
                Node::column(4.0, 12.0, col)
            }
        }
    };
    View::new(root.fill())
}

impl Widget for Notifications {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => {
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::Notifications(true)] }
            }
            Event::Notifications(snap) => {
                // A idade ("agora", "há 5 min") muda sozinha, mas só importa quando o texto muda de
                // faixa; comparar tudo basta porque a lista chega a cada poucos segundos.
                if *snap == self.snap {
                    return Outcome::none();
                }
                self.snap = snap.clone();
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![] }
            }
            Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        Some(view_for(ctx.shape, &self.snap))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: String::new() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        self.snap = NotifSnapshot::without_access(NotifAccess::Unknown); // esquece o conteúdo
        vec![Effect::Notifications(false)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::layout::{layout, Rect, TextMeasure};
    use pod_model::notifications::NotifItem;

    struct Fake;
    impl TextMeasure for Fake {
        fn measure(&self, text: &str, style: TextStyle, max_w: Option<f32>, _l: u8) -> (f32, f32) {
            let w = text.chars().count() as f32 * style.size_dip() * 0.55;
            (w.min(max_w.unwrap_or(w)), style.line_dip())
        }
    }

    pub(crate) fn demo() -> NotifSnapshot {
        let it = |app: &str, t: &str, age| NotifItem { app: app.into(), title: t.into(), age_secs: age };
        NotifSnapshot {
            access: NotifAccess::Allowed,
            items: vec![
                it("Mensagens", "Ana: vamos almoçar?", 40),
                it("Calendário", "Reunião em 10 minutos", 600),
                it("Mail", "Fatura de outubro disponível", 7_300),
                it("Atualizações", "Reinicie para concluir", 90_000),
            ],
            total: 7,
        }
    }

    #[test]
    fn subscribes_and_stop_forgets_the_content() {
        let mut w = Notifications { snap: NotifSnapshot::without_access(NotifAccess::Unknown), stopped: false };
        assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::Notifications(true)]);
        w.handle(&Event::Notifications(demo()));
        assert_eq!(w.snap.total, 7);
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::Notifications(false)]);
        assert_eq!(w.snap.total, 0, "nao guarda as notificacoes");
        assert!(w.stop(StopReason::Removed).is_empty());
        assert_eq!(w.handle(&Event::Notifications(demo())), Outcome::none());
    }

    #[test]
    fn only_a_changed_list_invalidates() {
        let mut w = Notifications { snap: NotifSnapshot::without_access(NotifAccess::Unknown), stopped: false };
        assert_eq!(w.handle(&Event::Notifications(demo())).invalidate, Invalidation::CONTENT);
        assert_eq!(w.handle(&Event::Notifications(demo())), Outcome::none());
        let denied = NotifSnapshot::without_access(NotifAccess::Denied);
        assert_eq!(w.handle(&Event::Notifications(denied)).invalidate, Invalidation::CONTENT);
    }

    #[test]
    fn counts_and_texts_in_portuguese() {
        assert_eq!(count_label(0), "Sem notifica\u{e7}\u{f5}es");
        assert_eq!(count_label(1), "1 notifica\u{e7}\u{e3}o");
        assert_eq!(count_label(7), "7 notifica\u{e7}\u{f5}es");
        assert_eq!(count_text(&demo()), "7");
        assert_eq!(count_text(&NotifSnapshot::without_access(NotifAccess::Denied)), "\u{2014}", "sem acesso nao mostra zero");
    }

    #[test]
    fn denied_and_unavailable_explain_what_to_do_in_the_big_format() {
        let hint_in = |a| {
            let v = view_for(Shape::S2X2, &NotifSnapshot::without_access(a));
            fn walk(n: &Node, out: &mut Vec<String>) {
                if let Kind::Text { text, .. } = &n.kind {
                    out.push(text.clone());
                }
                n.children().iter().for_each(|c| walk(c, out));
            }
            let mut t = Vec::new();
            walk(&v.root, &mut t);
            t.join(" | ")
        };
        assert!(hint_in(NotifAccess::Denied).contains("Configura"), "{}", hint_in(NotifAccess::Denied));
        assert!(hint_in(NotifAccess::Unavailable).contains("Indispon"));
    }

    #[test]
    fn every_state_and_shape_fits_the_tile() {
        let mut long = demo();
        long.items[0].title = "Um t\u{ed}tulo muito mas muito longo que n\u{e3}o cabe em linha nenhuma do cart\u{e3}o".into();
        long.items[1].app = "Aplicativo com nome enorme".into();
        let mut empty_titles = demo();
        empty_titles.items.iter_mut().for_each(|i| i.title.clear());
        let snaps = [
            NotifSnapshot::without_access(NotifAccess::Unknown),
            NotifSnapshot::without_access(NotifAccess::Denied),
            NotifSnapshot::without_access(NotifAccess::Unavailable),
            NotifSnapshot { access: NotifAccess::Allowed, items: vec![], total: 0 },
            demo(),
            long,
            empty_titles,
        ];
        for s in snaps {
            for shape in descriptor().shapes {
                let v = view_for(shape, &s);
                assert_eq!(v.validate(), Ok(()), "{shape}");
                let size = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, size.w, size.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{shape} {:?}: {:?}", s.access, l.rect);
                }
            }
        }
    }

    #[test]
    fn future_state_is_refused() {
        assert!(matches!(
            create(Some(&Snapshot { version: 2, data: String::new() })),
            Err(WidgetError::UnsupportedVersion { .. })
        ));
    }
}
