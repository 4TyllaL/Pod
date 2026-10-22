//! UI mínima: layout vertical da view do widget, em DIPs, e hit testing. Puro (sem Win32).
//!
//! Só os elementos do contrato existem: superfície, rótulo, botão e campo multilinha.

use pod_model::widget::{Element, LocalId};

pub mod anim;
pub mod keymode;
pub mod note;
pub mod theme;

const GAP: f32 = 6.0;
const LABEL_H: f32 = 22.0;
const BUTTON_H: f32 = 32.0;
const BUTTON_W: f32 = 96.0;
#[cfg(test)]
const FIELD_MIN_H: f32 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectDip {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectDip {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlacedKind {
    Label(String),
    Button { id: LocalId, label: String },
    TextField { id: LocalId },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub rect: RectDip,
    pub kind: PlacedKind,
}

/// Empilha os elementos verticalmente dentro de `area`. O campo de texto ocupa o espaço restante.
pub fn layout(root: &Element, area: RectDip) -> Vec<Placed> {
    let mut flat = Vec::new();
    flatten(root, &mut flat);
    let fixed: f32 = flat
        .iter()
        .map(|e| match e {
            Element::Label(_) => LABEL_H,
            Element::Button { .. } => BUTTON_H,
            _ => 0.0,
        })
        .sum::<f32>() + flat.len().saturating_sub(1) as f32 * GAP;
    let fields = flat.iter().filter(|e| matches!(e, Element::TextField { .. })).count().max(1) as f32;
    let field_h = (area.h - fixed).max(0.0) / fields;

    let mut y = area.y;
    let mut out = Vec::new();
    for e in flat {
        let (h, kind, w) = match e {
            Element::Label(t) => (LABEL_H, PlacedKind::Label(t.clone()), area.w),
            Element::Button { id, label } => {
                (BUTTON_H, PlacedKind::Button { id: *id, label: label.clone() }, BUTTON_W.min(area.w))
            }
            Element::TextField { id, .. } => (field_h, PlacedKind::TextField { id: *id }, area.w),
            Element::Surface(_) => continue,
        };
        out.push(Placed { rect: RectDip { x: area.x, y, w, h }, kind });
        y += h + GAP;
    }
    out
}

fn flatten<'a>(e: &'a Element, out: &mut Vec<&'a Element>) {
    match e {
        Element::Surface(children) => children.iter().for_each(|c| flatten(c, out)),
        other => out.push(other),
    }
}

/// Botão sob o ponto (em DIPs), se houver.
pub fn button_at(placed: &[Placed], x: f32, y: f32) -> Option<LocalId> {
    placed.iter().find_map(|p| match &p.kind {
        PlacedKind::Button { id, .. } if p.rect.contains(x, y) => Some(*id),
        _ => None,
    })
}

/// Retângulo do campo de texto `id`, se existir.
pub fn field_rect(placed: &[Placed], id: LocalId) -> Option<RectDip> {
    placed.iter().find_map(|p| match &p.kind {
        PlacedKind::TextField { id: f } if *f == id => Some(p.rect),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: RectDip = RectDip { x: 12.0, y: 36.0, w: 296.0, h: 192.0 };

    #[test]
    fn stacks_without_overlap_inside_the_area() {
        let view = Element::Surface(vec![
            Element::Label("a".into()),
            Element::Label("b".into()),
            Element::Button { id: LocalId(1), label: "+1".into() },
        ]);
        let placed = layout(&view, AREA);
        assert_eq!(placed.len(), 3);
        for pair in placed.windows(2) {
            assert!(pair[0].rect.y + pair[0].rect.h <= pair[1].rect.y, "sobreposição");
        }
        assert!(placed.iter().all(|p| p.rect.x >= AREA.x && p.rect.x + p.rect.w <= AREA.x + AREA.w));
    }

    #[test]
    fn text_field_takes_the_remaining_height() {
        let view = Element::Surface(vec![
            Element::Label("título".into()),
            Element::TextField { id: LocalId(7), text: String::new() },
        ]);
        let placed = layout(&view, AREA);
        let f = field_rect(&placed, LocalId(7)).unwrap();
        assert!((f.y + f.h) <= AREA.y + AREA.h + 0.01);
        assert!(f.h > FIELD_MIN_H, "deveria crescer além do mínimo");
    }

    #[test]
    fn hit_testing_finds_only_the_button() {
        let view = Element::Surface(vec![
            Element::Label("x".into()),
            Element::Button { id: LocalId(1), label: "+1".into() },
        ]);
        let placed = layout(&view, AREA);
        let b = placed.iter().find(|p| matches!(p.kind, PlacedKind::Button { .. })).unwrap().rect;
        assert_eq!(button_at(&placed, b.x + 1.0, b.y + 1.0), Some(LocalId(1)));
        assert_eq!(button_at(&placed, AREA.x + 1.0, AREA.y + 1.0), None, "rótulo não é botão");
        assert_eq!(button_at(&placed, b.x + b.w + 5.0, b.y + 1.0), None);
    }

    #[test]
    fn tiny_area_never_produces_negative_sizes() {
        let view = Element::Surface(vec![Element::TextField { id: LocalId(1), text: String::new() }]);
        let placed = layout(&view, RectDip { x: 0.0, y: 0.0, w: 10.0, h: 5.0 });
        assert!(placed.iter().all(|p| p.rect.w >= 0.0 && p.rect.h >= 0.0));
    }
}
