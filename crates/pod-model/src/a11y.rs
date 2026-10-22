//! Árvore de acessibilidade gerada da view (ADR-0001: UI Automation sai da view, não é escrita
//! à mão por widget). Pura: o host converte os itens em provedores UIA.

use crate::layout::{Laid, Rect};
use crate::view::{Kind, Node, View, VisualKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A11yRole {
    Text,
    Progress,
    Image,
    Button,
    /// Caixa de seleção; o nome já diz se está concluída.
    CheckBox,
}

#[derive(Debug, Clone, PartialEq)]
pub struct A11yItem {
    pub role: A11yRole,
    pub name: String,
    /// Em DIPs, relativo à janela.
    pub rect: Rect,
    /// Id do botão (para `Invoke`); `None` nos demais itens.
    pub button: Option<crate::widget::LocalId>,
}

fn percent(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * 100.0).floor() as u32
}

/// Descrição falada de um desenho que não é texto.
pub fn describe_visual(kind: &VisualKind) -> String {
    match kind {
        VisualKind::AnalogClock { secs, .. } => {
            let total = *secs as u32;
            format!("Rel\u{f3}gio anal\u{f3}gico, {:02}:{:02}", total / 3600 % 24, total / 60 % 60)
        }
    }
}

fn texts(n: &Node, out: &mut Vec<String>) {
    if let Kind::Text { text, .. } = &n.kind {
        if !text.is_empty() {
            out.push(text.clone());
        }
    }
    n.children().iter().for_each(|c| texts(c, out));
}

/// Itens em ordem de leitura (pré-ordem). Ícones são decorativos e ficam de fora; um anel com
/// texto sobreposto vira um único item de progresso com esse texto.
pub fn items(view: &View, laid: &[Laid]) -> Vec<A11yItem> {
    fn walk(n: &Node, laid: &[Laid], idx: &mut usize, out: &mut Vec<A11yItem>) {
        let Some(l) = laid.get(*idx) else { return };
        let rect = l.rect;
        *idx += 1;
        match &n.kind {
            Kind::Text { text, .. } | Kind::Marquee { text, .. } if !text.is_empty() => {
                out.push(A11yItem { role: A11yRole::Text, name: text.clone(), rect, button: None })
            }
            Kind::Bar { value, .. } | Kind::Segments { value, .. } => out.push(A11yItem {
                role: A11yRole::Progress,
                name: format!("Progresso: {}%", percent(*value)),
                rect,
                button: None,
            }),
            Kind::Ring { value, .. } => out.push(A11yItem {
                role: A11yRole::Progress,
                name: format!("Progresso: {}%", percent(*value)),
                rect,
                button: None,
            }),
            Kind::Visual { kind, .. } => {
                out.push(A11yItem { role: A11yRole::Image, name: describe_visual(kind), rect, button: None })
            }
            // Imagem com texto alternativo vira um item; sem texto é decorativa (não é anunciada).
            Kind::Image { label, .. } if !label.is_empty() => {
                out.push(A11yItem { role: A11yRole::Image, name: label.clone(), rect, button: None })
            }
            Kind::Image { .. } => {}
            Kind::Rings { label, .. } => {
                out.push(A11yItem { role: A11yRole::Image, name: label.clone(), rect, button: None })
            }
            Kind::Chart { label, .. } => {
                out.push(A11yItem { role: A11yRole::Image, name: label.clone(), rect, button: None })
            }
            Kind::Item { id, label, detail } => out.push(A11yItem {
                role: A11yRole::Button,
                name: if detail.is_empty() { label.clone() } else { format!("{label}, {detail}") },
                rect,
                button: Some(*id),
            }),
            Kind::Check { id, label, checked } => out.push(A11yItem {
                role: A11yRole::CheckBox,
                name: format!("{label}, {}", if *checked { "conclu\u{ed}da" } else { "pendente" }),
                rect,
                button: Some(*id),
            }),
            Kind::Button { id, label } => {
                out.push(A11yItem { role: A11yRole::Button, name: label.clone(), rect, button: Some(*id) })
            }
            Kind::Layers { children } if children.iter().any(|c| matches!(c.kind, Kind::Ring { .. })) => {
                let mut said = Vec::new();
                texts(n, &mut said);
                let name = if said.is_empty() {
                    String::from("Progresso")
                } else {
                    said.join(" ")
                };
                out.push(A11yItem { role: A11yRole::Progress, name, rect, button: None });
                // Consome os filhos sem emitir itens próprios.
                fn skip(n: &Node, idx: &mut usize) {
                    for c in n.children() {
                        *idx += 1;
                        skip(c, idx);
                    }
                }
                skip(n, idx);
                return;
            }
            _ => {}
        }
        for c in n.children() {
            walk(c, laid, idx, out);
        }
    }
    let mut out = Vec::new();
    let mut idx = 0;
    walk(&view.root, laid, &mut idx, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{layout, TextMeasure};
    use crate::view::{Align, Role, TextStyle};

    struct Fake;
    impl TextMeasure for Fake {
        fn measure(&self, text: &str, style: TextStyle, _: Option<f32>, _: u8) -> (f32, f32) {
            (text.chars().count() as f32 * 6.0, style.line_dip())
        }
    }

    fn t(s: &str) -> Node {
        Node::new(Kind::Text { text: s.into(), style: TextStyle::Body, role: Role::Primary, align: Align::Start, max_lines: 1 })
    }

    fn run(root: Node) -> Vec<A11yItem> {
        let v = View::new(root);
        let laid = layout(&v, Rect::new(0.0, 0.0, 200.0, 100.0), &Fake);
        items(&v, &laid)
    }

    #[test]
    fn texts_come_out_in_reading_order_with_their_rects() {
        let it = run(Node::column(2.0, 0.0, vec![t("domingo"), t("18 de outubro")]));
        assert_eq!(it.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["domingo", "18 de outubro"]);
        assert!(it[0].rect.bottom() <= it[1].rect.y + 0.01);
        assert!(it.iter().all(|i| i.role == A11yRole::Text));
    }

    #[test]
    fn a_ring_with_a_label_is_one_progress_item() {
        let it = run(Node::row(4.0, 0.0, vec![
            Node::layers(vec![Node::ring(0.44, 40.0, 4.0, Role::Accent), t("44%")]),
            t("Dia"),
        ]));
        assert_eq!(it.len(), 2, "{it:?}");
        assert_eq!((it[0].role, it[0].name.as_str()), (A11yRole::Progress, "44%"));
        assert_eq!(it[1].name, "Dia");
    }

    #[test]
    fn bars_visuals_and_buttons_are_described_and_icons_are_skipped() {
        let it = run(Node::column(0.0, 0.0, vec![
            Node::bar(0.999, 6.0, Role::Accent),
            Node::visual(VisualKind::AnalogClock { secs: 10.0 * 3600.0 + 42.0 * 60.0, seconds_hand: false }, 80.0),
            Node::icon('x', 14.0, Role::Accent),
            Node::new(Kind::Button { id: crate::widget::LocalId(1), label: "Ok".into() }),
        ]));
        let names: Vec<_> = it.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Progresso: 99%", "Rel\u{f3}gio anal\u{f3}gico, 10:42", "Ok"]);
        assert_eq!(it[2].role, A11yRole::Button);
    }

    #[test]
    fn images_are_announced_only_with_alt_text() {
        use crate::image::ImageData;
        let img = ImageData::gradient(4, 4, [1, 2, 3], [4, 5, 6]);
        let it = run(Node::row(0.0, 0.0, vec![
            Node::image(img.clone(), 40.0, 8.0, "Capa do \u{e1}lbum"),
            Node::image(img, 40.0, 8.0, ""),
        ]));
        assert_eq!(it.len(), 1, "a decorativa nao e anunciada");
        assert_eq!((it[0].role, it[0].name.as_str()), (A11yRole::Image, "Capa do \u{e1}lbum"));
    }

    #[test]
    fn checkboxes_say_their_state_and_can_be_invoked() {
        let it = run(Node::column(2.0, 0.0, vec![
            Node::check(crate::widget::LocalId(7), "Comprar p\u{e3}o", false),
            Node::check(crate::widget::LocalId(8), "Pagar conta", true),
        ]));
        assert_eq!(it[0].name, "Comprar p\u{e3}o, pendente");
        assert_eq!(it[1].name, "Pagar conta, conclu\u{ed}da");
        assert!(it.iter().all(|i| i.role == A11yRole::CheckBox));
        assert_eq!(it.iter().map(|i| i.button.unwrap().0).collect::<Vec<_>>(), vec![7, 8]);
    }

    #[test]
    fn empty_text_is_not_announced() {
        assert!(run(Node::column(0.0, 0.0, vec![t("")])).is_empty());
    }
}
