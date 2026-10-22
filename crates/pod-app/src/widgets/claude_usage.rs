//! Widget Claude usage: quanto o Claude Code consumiu hoje e nos últimos 7 dias, lido **só dos
//! arquivos locais** `~/.claude/projects` (sem rede, sem chave). Formatos 1×1, 2×1 e 2×2 com gráfico
//! por dia. Assina `ClaudeUsage` só enquanto visível; não guarda nada em disco.

use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::usage::{format_tokens, UsageSnapshot};
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, Outcome, SizeDip, Snapshot, StopReason, ViewCtx, Widget,
    WidgetError,
};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.claude-usage");
pub const STATE_VERSION: u32 = 1;

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

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(ClaudeUsage { usage: None, stopped: false }))
}

struct ClaudeUsage {
    /// `None` = ainda lendo.
    usage: Option<UsageSnapshot>,
    stopped: bool,
}

fn text(s: &str, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

fn replies_label(n: u32) -> String {
    match n {
        0 => "nenhuma resposta".to_string(),
        1 => "1 resposta".to_string(),
        n => format!("{n} respostas"),
    }
}

fn chart(u: &UsageSnapshot) -> Node {
    let label = format!("Tokens por dia nos \u{fa}ltimos 7 dias; hoje {}", format_tokens(u.today().billed()));
    Node::chart(u.chart(), 0.0, Role::Accent, label)
}

fn message(shape: Shape, main: &str, hint: &str) -> Node {
    let mut col = vec![text("\u{2728}", TextStyle::Headline, Role::Accent, Align::Center), text(main, TextStyle::Body, Role::Secondary, Align::Center)];
    if shape == Shape::S2X2 && !hint.is_empty() {
        col.push(Node::new(Kind::Text { text: hint.to_string(), style: TextStyle::Caption, role: Role::Tertiary, align: Align::Center, max_lines: 3 }).size(Len::Fill(1), Len::Auto));
    }
    Node::column(4.0, 14.0, col).centered()
}

fn view_for(shape: Shape, usage: &Option<UsageSnapshot>) -> View {
    let root = match usage {
        None => message(shape, "lendo\u{2026}", ""),
        Some(u) if u.files == 0 => {
            message(shape, "Sem dados", "N\u{e3}o encontrei transcritos do Claude Code em ~/.claude/projects.")
        }
        Some(u) => {
            let today = format_tokens(u.today().billed());
            match shape {
                sh if sh == Shape::S1X1 => Node::column(2.0, 6.0, vec![
                    text(&today, TextStyle::Headline, Role::Accent, Align::Center),
                    text("CLAUDE HOJE", TextStyle::Caption, Role::Secondary, Align::Center),
                ])
                .centered(),
                sh if sh == Shape::S2X2 => {
                    let week = u.week();
                    Node::column(6.0, 14.0, vec![
                        super::tkit::chip_header('\u{2728}', "Claude Code", false),
                        Node::row(8.0, 0.0, vec![
                            text(&today, TextStyle::Display, Role::Accent, Align::Start).size(Len::Auto, Len::Auto),
                            text("tokens hoje", TextStyle::Caption, Role::Secondary, Align::Start),
                        ])
                        .size(Len::Fill(1), Len::Auto),
                        chart(u),
                        text(&format!("7 dias: {} \u{b7} cache lido {}", format_tokens(week.billed()), format_tokens(week.cache_read)), TextStyle::Caption, Role::Secondary, Align::Start),
                        text(&replies_label(u.messages_today), TextStyle::Caption, Role::Tertiary, Align::Start),
                    ])
                }
                _ => Node::row(10.0, 12.0, vec![
                    Node::column(2.0, 0.0, vec![
                        text("Claude hoje", TextStyle::Caption, Role::Secondary, Align::Start),
                        text(&today, TextStyle::Headline, Role::Accent, Align::Start),
                        text(&replies_label(u.messages_today), TextStyle::Caption, Role::Tertiary, Align::Start),
                    ])
                    .size(Len::Fixed(80.0), Len::Auto),
                    chart(u),
                ]),
            }
        }
    };
    View::new(root.fill())
}

impl Widget for ClaudeUsage {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::ClaudeUsage(true)] },
            Event::ClaudeUsage(u) => {
                if self.usage.as_ref() == Some(u) {
                    return Outcome::none();
                }
                self.usage = Some(u.clone());
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
        Some(view_for(ctx.shape, &self.usage))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: String::new() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        self.usage = None;
        vec![Effect::ClaudeUsage(false)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::layout::{layout, Rect, TextMeasure};
    use pod_model::usage::{Tokens, DAYS};

    struct Fake;
    impl TextMeasure for Fake {
        fn measure(&self, text: &str, style: TextStyle, max_w: Option<f32>, _l: u8) -> (f32, f32) {
            let w = text.chars().count() as f32 * style.size_dip() * 0.55;
            (w.min(max_w.unwrap_or(w)), style.line_dip())
        }
    }

    pub(crate) fn demo() -> UsageSnapshot {
        let mut u = UsageSnapshot::empty();
        let per_day = [180_000u64, 420_000, 90_000, 760_000, 1_300_000, 640_000, 1_050_000];
        for (i, out) in per_day.iter().enumerate() {
            u.days[i] = Tokens { input: out / 20, output: *out, cache_read: out * 40, cache_create: out / 4 };
        }
        u.messages_today = 38;
        u.files = 12;
        u
    }

    #[test]
    fn subscribes_stops_and_forgets() {
        let mut w = ClaudeUsage { usage: None, stopped: false };
        assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::ClaudeUsage(true)]);
        w.handle(&Event::ClaudeUsage(demo()));
        assert!(w.usage.is_some());
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::ClaudeUsage(false)]);
        assert!(w.usage.is_none());
        assert!(w.stop(StopReason::Removed).is_empty());
        assert_eq!(w.handle(&Event::ClaudeUsage(demo())), Outcome::none());
    }

    #[test]
    fn only_a_changed_summary_invalidates() {
        let mut w = ClaudeUsage { usage: None, stopped: false };
        assert_eq!(w.handle(&Event::ClaudeUsage(demo())).invalidate, Invalidation::CONTENT);
        assert_eq!(w.handle(&Event::ClaudeUsage(demo())), Outcome::none());
        let mut more = demo();
        more.messages_today += 1;
        assert_eq!(w.handle(&Event::ClaudeUsage(more)).invalidate, Invalidation::CONTENT);
    }

    #[test]
    fn texts_and_chart_label_in_portuguese() {
        assert_eq!(replies_label(0), "nenhuma resposta");
        assert_eq!(replies_label(1), "1 resposta");
        assert_eq!(replies_label(38), "38 respostas");
        let c = chart(&demo());
        match c.kind {
            Kind::Chart { values, label, .. } => {
                assert_eq!(values.len(), DAYS);
                assert!(label.contains("7 dias") && label.contains("hoje"), "{label}");
            }
            _ => panic!("nao e grafico"),
        }
    }

    #[test]
    fn missing_folder_says_so_in_the_big_format() {
        let v = view_for(Shape::S2X2, &Some(UsageSnapshot::empty()));
        fn walk(n: &Node, out: &mut Vec<String>) {
            if let Kind::Text { text, .. } = &n.kind {
                out.push(text.clone());
            }
            n.children().iter().for_each(|c| walk(c, out));
        }
        let mut t = Vec::new();
        walk(&v.root, &mut t);
        assert!(t.iter().any(|s| s == "Sem dados") && t.iter().any(|s| s.contains(".claude/projects")), "{t:?}");
    }

    #[test]
    fn every_state_and_shape_fits_the_tile() {
        let mut zero = UsageSnapshot::empty();
        zero.files = 3;
        let mut huge = demo();
        huge.days[DAYS - 1].output = 98_765_432_100;
        for u in [None, Some(UsageSnapshot::empty()), Some(zero), Some(demo()), Some(huge)] {
            for shape in descriptor().shapes {
                let v = view_for(shape, &u);
                assert_eq!(v.validate(), Ok(()), "{shape}");
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{shape}: {:?}", l.rect);
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
