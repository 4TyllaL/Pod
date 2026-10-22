//! Cartão de medidor (anel com percentual) compartilhado por Time progress e pelos widgets de
//! métricas: um só desenho para 1×1, 2×1 e 2×2.

use pod_model::shape::Shape;
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};

use super::tkit::chip_header;

pub struct Gauge {
    pub title: String,
    /// Rótulo curto do 1×1 (maiúsculas); vazio = usa o título.
    pub short: String,
    /// `None` = ainda sem leitura: anel vazio e "—".
    pub value: Option<f32>,
    /// Texto no centro do anel (normalmente o percentual).
    pub center: String,
    pub detail: String,
    pub role: Role,
    /// Ícone vetorial do chip do cabeçalho (`'\0'` = só texto).
    pub icon: char,
}

fn text(s: String, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s, style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

fn ring_with(v: f32, size: f32, thick: f32, role: Role, label: Node) -> Node {
    Node::layers(vec![Node::ring(v, size, thick, role), label])
}

fn head(g: &Gauge, centered: bool) -> Node {
    if g.icon == '\0' {
        text(g.title.clone(), TextStyle::Title, Role::Primary, if centered { Align::Center } else { Align::Start })
    } else {
        chip_header(g.icon, &g.title, centered)
    }
}

pub fn gauge_view(shape: Shape, g: &Gauge) -> View {
    let v = g.value.unwrap_or(0.0);
    let center = if g.value.is_some() { g.center.clone() } else { "\u{2014}".to_string() };
    let root = match shape {
        s if s == Shape::S1X1 => Node::column(5.0, 6.0, vec![
            ring_with(v, 54.0, 6.0, g.role, text(center, TextStyle::Body, Role::Primary, Align::Center)),
            text(if g.short.is_empty() { g.title.to_uppercase() } else { g.short.to_uppercase() }, TextStyle::Caption, Role::Secondary, Align::Center),
        ])
        .centered(),
        s if s == Shape::S2X2 => Node::column(8.0, 14.0, vec![
            head(g, true),
            ring_with(v, 112.0, 10.0, g.role, text(center, TextStyle::Display, Role::Primary, Align::Center)),
            text(g.detail.clone(), TextStyle::Body, Role::Secondary, Align::Center),
        ])
        .centered(),
        _ => Node::row(14.0, 12.0, vec![
            ring_with(v, 64.0, 7.0, g.role, text(center, TextStyle::Body, Role::Primary, Align::Center)),
            Node::column(4.0, 0.0, vec![
                head(g, false),
                text(g.detail.clone(), TextStyle::Caption, Role::Secondary, Align::Start),
                // Sem leitura não há progresso a mostrar (nem a anunciar): sem barra.
                if g.value.is_some() { Node::bar(v, 6.0, g.role) } else { Node::spacer().size(Len::Fill(1), Len::Fixed(6.0)) },
            ])
            .size(Len::Fill(1), Len::Auto),
        ]),
    };
    View::new(root.fill())
}
