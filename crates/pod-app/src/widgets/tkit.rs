//! Peças comuns dos widgets de cronometragem: botões, texto de tempo e itens de menu.

use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle};
use pod_model::widget::{LocalId, MenuItem};

pub fn button(id: u32, label: &str) -> Node {
    Node::new(Kind::Button { id: LocalId(id), label: label.to_string() })
}

/// Texto de tempo: Display quando cabe (até "00:00"), Headline para "1:05:00".
pub fn time_text(s: String, role: Role) -> Node {
    let style = if s.chars().count() <= 5 { TextStyle::Display } else { TextStyle::Headline };
    Node::new(Kind::Text { text: s, style, role, align: Align::Center, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

pub fn label(s: String, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s, style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

pub fn item(id: u32, label: &str, checked: bool, group: Option<&'static str>) -> MenuItem {
    MenuItem { id, label: label.to_string(), checked, group }
}

/// Cor do símbolo do cabeçalho para cada ícone (paleta de sistema).
fn symbol_role(glyph: char) -> Role {
    match glyph {
        '\u{25a3}' => Role::Blue,
        '\u{21c5}' => Role::Indigo,
        '\u{2728}' => Role::Clay,
        '\u{2713}' => Role::Blue,
        '\u{1f4a7}' => Role::Cyan,
        '\u{23f1}' => Role::Orange,
        '\u{1f50b}' => Role::Positive,
        _ => Role::Secondary,
    }
}

/// Cabeçalho do cartão (estilo macOS): símbolo colorido e título pequeno em cinza ao lado.
/// `centered` centraliza o conjunto (formatos em coluna); senão alinha à esquerda.
pub fn chip_header(glyph: char, title: &str, centered: bool) -> Node {
    let icon = Node::new(Kind::Text { text: glyph.to_string(), style: TextStyle::Title, role: symbol_role(glyph), align: Align::Center, max_lines: 1 })
        .size(Len::Fixed(18.0), Len::Fixed(18.0));
    let title = Node::new(Kind::Text { text: title.to_string(), style: TextStyle::Label, role: Role::Secondary, align: Align::Start, max_lines: 1 })
        .size(if centered { Len::Auto } else { Len::Fill(1) }, Len::Auto);
    let row = Node::row(6.0, 0.0, vec![icon, title]);
    if centered { row.centered().size(Len::Fill(1), Len::Auto) } else { row.size(Len::Fill(1), Len::Auto) }
}
