//! Motor de layout puro (Stack/Fill) sobre `view::View`. Sem Win32: o texto é medido por um
//! `TextMeasure` injetado (DirectWrite no app; medidor determinístico nos testes).
//!
//! Saída: um `Laid` por nó, em pré-ordem (o índice é o da travessia da árvore). Conteúdo que não
//! cabe não encolhe: transborda e é recortado pelo host.

use crate::view::{Align, Axis, Kind, Len, Node, TextStyle, View};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    fn deflate(&self, pad: f32) -> Rect {
        Rect::new(self.x + pad, self.y + pad, (self.w - 2.0 * pad).max(0.0), (self.h - 2.0 * pad).max(0.0))
    }
    pub fn contains(&self, o: &Rect) -> bool {
        const E: f32 = 0.01;
        o.x >= self.x - E && o.y >= self.y - E && o.right() <= self.right() + E && o.bottom() <= self.bottom() + E
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Laid {
    pub index: usize,
    pub rect: Rect,
}

pub trait TextMeasure {
    /// Tamanho do texto quebrado em `max_w` (se houver), limitado a `max_lines` linhas.
    fn measure(&self, text: &str, style: TextStyle, max_w: Option<f32>, max_lines: u8) -> (f32, f32);
}

const BUTTON_PAD_X: f32 = 12.0;
const BUTTON_H: f32 = 28.0;
const FIELD_W: f32 = 120.0;
const FIELD_H: f32 = 28.0;
/// Caixa (16) mais espaço até o rótulo.
pub const CHECK_BOX_W: f32 = 24.0;
pub const CHECK_H: f32 = 22.0;
pub const ITEM_H: f32 = 24.0;

pub fn layout(view: &View, bounds: Rect, m: &dyn TextMeasure) -> Vec<Laid> {
    let mut out = Vec::with_capacity(view.node_count());
    place(&view.root, bounds, m, &mut out);
    out
}

fn len_or(l: Len, natural: f32) -> f32 {
    match l {
        Len::Fixed(v) => v,
        _ => natural,
    }
}

/// Tamanho natural (Fill conta como Auto) com largura máxima disponível `max_w`.
fn natural(n: &Node, max_w: f32, m: &dyn TextMeasure) -> (f32, f32) {
    let (cw, ch) = match &n.kind {
        Kind::Stack { axis, gap, pad, children, .. } => {
            let inner_w = (max_w - 2.0 * pad).max(0.0);
            let gaps = gap * children.len().saturating_sub(1) as f32;
            let sizes: Vec<(f32, f32)> = children.iter().map(|c| natural(c, inner_w, m)).collect();
            let (w, h) = match axis {
                Axis::Column => (
                    sizes.iter().map(|s| s.0).fold(0.0, f32::max),
                    sizes.iter().map(|s| s.1).sum::<f32>() + gaps,
                ),
                Axis::Row => (
                    sizes.iter().map(|s| s.0).sum::<f32>() + gaps,
                    sizes.iter().map(|s| s.1).fold(0.0, f32::max),
                ),
            };
            (w + 2.0 * pad, h + 2.0 * pad)
        }
        Kind::Spacer => (0.0, 0.0),
        Kind::Marquee { style, .. } => (0.0, style.line_dip()),
        Kind::Layers { children } => children.iter().map(|c| natural(c, max_w, m)).fold((0.0, 0.0), |a, b| (f32::max(a.0, b.0), f32::max(a.1, b.1))),
        Kind::Plate { pad, child, .. } => {
            let (w, h) = natural(child, (max_w - 2.0 * pad).max(0.0), m);
            (w + 2.0 * pad, h + 2.0 * pad)
        }
        Kind::Text { text, style, max_lines, .. } => m.measure(text, *style, Some(max_w), *max_lines),
        Kind::Icon { size, .. } | Kind::Visual { size, .. } => (*size, *size),
        Kind::Ring { size, .. } => (*size, *size),
        Kind::Bar { thickness, .. } | Kind::Segments { thickness, .. } => (0.0, *thickness),
        Kind::Rings { size, .. } => (*size, *size),
        Kind::Divider => (0.0, 1.0),
        Kind::Button { label, .. } => {
            let (w, _) = m.measure(label, TextStyle::Body, None, 1);
            (w + 2.0 * BUTTON_PAD_X, BUTTON_H)
        }
        Kind::TextField { .. } => (FIELD_W, FIELD_H),
        Kind::Chart { .. } => (0.0, 40.0),
        Kind::Image { .. } => (0.0, 0.0),
        Kind::Item { label, detail, .. } => {
            let (lw, _) = m.measure(label, TextStyle::Body, Some(max_w), 1);
            let (dw, _) = m.measure(detail, TextStyle::Caption, Some(max_w), 1);
            (lw + dw + 8.0, ITEM_H)
        }
        Kind::Check { label, .. } => {
            let (w, _) = m.measure(label, TextStyle::Body, Some((max_w - CHECK_BOX_W).max(0.0)), 1);
            (w + CHECK_BOX_W, CHECK_H)
        }
    };
    (len_or(n.w, cw), len_or(n.h, ch))
}

fn offset(align: Align, free: f32) -> f32 {
    match align {
        Align::Start => 0.0,
        Align::Center => free.max(0.0) / 2.0,
        Align::End => free.max(0.0),
    }
}

fn place(n: &Node, rect: Rect, m: &dyn TextMeasure, out: &mut Vec<Laid>) {
    out.push(Laid { index: out.len(), rect });
    match &n.kind {
        Kind::Stack { axis, gap, pad, main, cross, children } => {
            let inner = rect.deflate(*pad);
            place_stack(*axis, *gap, *main, *cross, children, inner, m, out);
        }
        Kind::Layers { children } => {
            for c in children {
                let (cw, ch) = natural(c, rect.w, m);
                let w = if matches!(c.w, Len::Fill(_)) { rect.w } else { cw.min(rect.w) };
                let h = if matches!(c.h, Len::Fill(_)) { rect.h } else { ch };
                let r = Rect::new(rect.x + (rect.w - w) / 2.0, rect.y + (rect.h - h) / 2.0, w, h);
                place(c, r, m, out);
            }
        }
        Kind::Plate { pad, child, .. } => {
            let inner = rect.deflate(*pad);
            let (cw, ch) = natural(child, inner.w, m);
            let w = match child.w {
                Len::Fill(_) => inner.w,
                _ => cw.min(inner.w),
            };
            let h = match child.h {
                Len::Fill(_) => inner.h,
                _ => ch,
            };
            place(child, Rect::new(inner.x, inner.y, w, h), m, out);
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn place_stack(
    axis: Axis,
    gap: f32,
    main: Align,
    cross: Align,
    children: &[Node],
    inner: Rect,
    m: &dyn TextMeasure,
    out: &mut Vec<Laid>,
) {
    let (main_total, cross_total) = match axis {
        Axis::Column => (inner.h, inner.w),
        Axis::Row => (inner.w, inner.h),
    };
    let main_len = |n: &Node| if axis == Axis::Column { n.h } else { n.w };
    let cross_len = |n: &Node| if axis == Axis::Column { n.w } else { n.h };

    // 1. Tamanho principal dos filhos que não são Fill.
    let mut mains: Vec<f32> = Vec::with_capacity(children.len());
    let mut weights = 0u32;
    let mut used = gap * children.len().saturating_sub(1) as f32;
    for c in children {
        match main_len(c) {
            Len::Fill(w) => {
                weights += w as u32;
                mains.push(0.0);
            }
            Len::Fixed(v) => {
                used += v;
                mains.push(v);
            }
            Len::Auto => {
                let (nw, nh) = natural(c, inner.w, m);
                let v = if axis == Axis::Column { nh } else { nw };
                used += v;
                mains.push(v);
            }
        }
    }
    // 2. Reparte o que sobra entre os Fill, por peso.
    let free = (main_total - used).max(0.0);
    if weights > 0 {
        for (c, slot) in children.iter().zip(mains.iter_mut()) {
            if let Len::Fill(w) = main_len(c) {
                *slot = free * w as f32 / weights as f32;
            }
        }
    }
    // 3. Posiciona. Sem Fill, o espaço livre é distribuído pelo alinhamento principal.
    let mut pos = if weights > 0 { 0.0 } else { offset(main, free) };
    for (c, &mlen) in children.iter().zip(&mains) {
        let clen = match cross_len(c) {
            Len::Fixed(v) => v,
            Len::Fill(_) => cross_total,
            Len::Auto => {
                // Largura conhecida (coluna: a cruzada; linha: a principal) antes de medir a altura.
                let (nw, nh) = match axis {
                    Axis::Column => natural(c, inner.w, m),
                    Axis::Row => natural(&Node { w: Len::Fixed(mlen), ..c.clone() }, mlen, m),
                };
                let v = if axis == Axis::Column { nw } else { nh };
                v.min(cross_total.max(0.0)).max(0.0)
            }
        };
        let coff = offset(cross, cross_total - clen);
        let r = match axis {
            Axis::Column => Rect::new(inner.x + coff, inner.y + pos, clen, mlen),
            Axis::Row => Rect::new(inner.x + pos, inner.y + coff, mlen, clen),
        };
        place(c, r, m, out);
        pos += mlen + gap;
    }
}

/// Botões da view com seus retângulos (para hit test e ação principal). Ordem da árvore.
pub fn button_rects(view: &View, laid: &[Laid]) -> Vec<(crate::widget::LocalId, Rect)> {
    fn walk(n: &Node, laid: &[Laid], idx: &mut usize, out: &mut Vec<(crate::widget::LocalId, Rect)>) {
        let Some(l) = laid.get(*idx) else { return };
        *idx += 1;
        if let Kind::Button { id, .. } | Kind::Check { id, .. } | Kind::Item { id, .. } = &n.kind {
            out.push((*id, l.rect));
        }
        for c in n.children() {
            walk(c, laid, idx, out);
        }
    }
    let mut out = Vec::new();
    walk(&view.root, laid, &mut 0, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::{Fill, Role, View};

    /// 6 DIP por caractere para Body (proporcional ao tamanho); quebra por caractere.
    struct Fake;
    impl TextMeasure for Fake {
        fn measure(&self, text: &str, style: TextStyle, max_w: Option<f32>, max_lines: u8) -> (f32, f32) {
            let cw = style.size_dip() * 6.0 / 13.0;
            let full = text.chars().count() as f32 * cw;
            let lines = match max_w {
                Some(w) if w >= cw && full > w => (full / (w / cw).floor().max(1.0) / cw).ceil().max(1.0),
                _ => 1.0,
            };
            let lines = lines.min(max_lines.max(1) as f32);
            (full.min(max_w.unwrap_or(full)), lines * style.line_dip())
        }
    }

    fn txt(s: &str) -> Node {
        Node::text(s, TextStyle::Body, Role::Primary)
    }
    fn lay(root: Node, w: f32, h: f32) -> Vec<Laid> {
        let v = View::new(root);
        assert_eq!(v.validate(), Ok(()));
        layout(&v, Rect::new(0.0, 0.0, w, h), &Fake)
    }

    #[test]
    fn column_stacks_auto_children_with_gap_and_padding() {
        let l = lay(Node::column(4.0, 10.0, vec![txt("abc"), txt("de")]), 100.0, 100.0);
        assert_eq!(l.len(), 3);
        assert_eq!(l[1].rect, Rect::new(10.0, 10.0, 18.0, 17.0));
        assert_eq!(l[2].rect, Rect::new(10.0, 31.0, 12.0, 17.0));
    }

    #[test]
    fn fill_children_split_the_remaining_space_by_weight() {
        let root = Node::row(
            10.0,
            0.0,
            vec![
                txt("ab").size(Len::Fixed(20.0), Len::Fill(1)),
                Node::spacer().size(Len::Fill(1), Len::Fill(1)),
                Node::spacer().size(Len::Fill(3), Len::Fill(1)),
            ],
        );
        let l = lay(root, 200.0, 40.0);
        // 200 - 20 - 2*10 = 160 livres: 40 e 120.
        assert_eq!(l[1].rect.w, 20.0);
        assert_eq!(l[2].rect, Rect::new(30.0, 0.0, 40.0, 40.0));
        assert_eq!(l[3].rect, Rect::new(80.0, 0.0, 120.0, 40.0));
    }

    #[test]
    fn cross_axis_alignment_and_centering() {
        let root = Node::column(0.0, 0.0, vec![txt("abcd")]).centered().size(Len::Fill(1), Len::Fill(1));
        let l = lay(root, 100.0, 60.0);
        // Texto 24x17 centrado em 100x60.
        assert_eq!(l[1].rect, Rect::new(38.0, 21.5, 24.0, 17.0));
    }

    #[test]
    fn main_axis_alignment_without_fill() {
        let mut root = Node::row(0.0, 0.0, vec![txt("ab")]);
        if let Kind::Stack { main, .. } = &mut root.kind {
            *main = Align::End;
        }
        let l = lay(root, 100.0, 20.0);
        assert_eq!(l[1].rect.x, 88.0);
    }

    #[test]
    fn long_text_wraps_inside_a_column_and_grows_taller() {
        let t = Node::new(Kind::Text {
            text: "abcdefghijklmnop".into(),
            style: TextStyle::Body,
            role: Role::Primary,
            align: Align::Start,
            max_lines: 3,
        })
        .size(Len::Fill(1), Len::Auto);
        let l = lay(Node::column(0.0, 0.0, vec![t]), 48.0, 200.0);
        // 16 chars * 6 = 96 em 48 de largura: 2 linhas.
        assert_eq!(l[1].rect.w, 48.0);
        assert_eq!(l[1].rect.h, 34.0);
    }

    #[test]
    fn plate_insets_its_child() {
        let l = lay(Node::plate(Fill::Plain, 8.0, 6.0, Node::bar(0.5, 4.0, Role::Accent)), 100.0, 30.0);
        assert_eq!(l[1].rect, Rect::new(6.0, 6.0, 88.0, 4.0));
    }

    #[test]
    fn overflow_is_not_shrunk() {
        let l = lay(Node::column(0.0, 0.0, vec![txt("a").size(Len::Fixed(80.0), Len::Fixed(80.0)); 2]), 50.0, 100.0);
        assert_eq!(l[2].rect.bottom(), 160.0);
    }

    /// Todo filho fica dentro do pai quando o conteúdo cabe, em todos os formatos de grade.
    #[test]
    fn children_stay_inside_parents_in_every_shape() {
        use crate::shape::Shape;
        for shape in Shape::ALL {
            let s = shape.size_dip();
            let root = Node::plate(
                Fill::Plain,
                14.0,
                8.0,
                Node::column(
                    4.0,
                    0.0,
                    vec![
                        Node::row(6.0, 0.0, vec![Node::icon('x', 14.0, Role::Accent), txt("Titulo")]),
                        Node::spacer(),
                        Node::bar(0.3, 4.0, Role::Accent),
                    ],
                ),
            )
            .fill();
            let v = View::new(root);
            let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
            assert_eq!(laid.len(), v.node_count());
            fn walk(n: &Node, laid: &[Laid], idx: &mut usize, parent: Option<Rect>) {
                let me = laid[*idx].rect;
                if let Some(p) = parent {
                    assert!(p.contains(&me), "{me:?} fora de {p:?}");
                }
                *idx += 1;
                for c in n.children() {
                    walk(c, laid, idx, Some(me));
                }
            }
            let mut i = 0;
            walk(&v.root, &laid, &mut i, None);
            assert_eq!(i, laid.len());
        }
    }

    #[test]
    fn layers_center_every_child_in_the_same_box() {
        let root = Node::layers(vec![Node::ring(0.5, 40.0, 4.0, Role::Accent), txt("ab")]);
        let l = lay(root, 60.0, 60.0);
        assert_eq!(l[1].rect, Rect::new(10.0, 10.0, 40.0, 40.0));
        assert_eq!(l[2].rect, Rect::new(24.0, 21.5, 12.0, 17.0));
    }

    #[test]
    fn button_rects_follow_the_tree_order() {
        let b = |id: u32, l: &str| Node::new(Kind::Button { id: crate::widget::LocalId(id), label: l.into() });
        let v = View::new(Node::column(4.0, 0.0, vec![txt("x"), Node::row(4.0, 0.0, vec![b(7, "a"), b(3, "bb")])]));
        let laid = layout(&v, Rect::new(0.0, 0.0, 200.0, 100.0), &Fake);
        let rects = button_rects(&v, &laid);
        assert_eq!(rects.iter().map(|(i, _)| i.0).collect::<Vec<_>>(), vec![7, 3]);
        assert!(rects[0].1.right() <= rects[1].1.x, "lado a lado, sem sobrepor");
    }

    #[test]
    fn a_checkbox_reserves_room_for_its_box_and_is_hit_testable() {
        let c = Node::check(crate::widget::LocalId(3), "abcd", false);
        let v = View::new(Node::column(0.0, 0.0, vec![c]));
        let laid = layout(&v, Rect::new(0.0, 0.0, 200.0, 100.0), &Fake);
        assert_eq!(laid[1].rect.h, CHECK_H);
        assert_eq!(button_rects(&v, &laid).len(), 1);
        // Largura natural: caixa + texto (24 + 4*6) quando o pai não a estica.
        let narrow = View::new(Node::row(0.0, 0.0, vec![Node::new(Kind::Check { id: crate::widget::LocalId(3), label: "abcd".into(), checked: true })]));
        assert_eq!(layout(&narrow, Rect::new(0.0, 0.0, 200.0, 100.0), &Fake)[1].rect.w, CHECK_BOX_W + 24.0);
    }

    #[test]
    fn layout_is_deterministic() {
        let root = Node::column(3.0, 5.0, vec![txt("abc"), Node::spacer(), txt("z")]);
        assert_eq!(lay(root.clone(), 90.0, 90.0), lay(root, 90.0, 90.0));
    }
}
