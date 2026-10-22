//! Composição Editorial das notas. Uma só geometria para desenho, input, editor e UIA.
use super::RectDip;

#[derive(Clone, Copy, Debug)]
pub struct NoteLayout {
    pub radius: f32,
    pub caption_bottom: f32,
    pub menu: RectDip,
    pub label: RectDip,
    pub content: RectDip,
    pub status: RectDip,
    pub font_dip: f32,
}

impl NoteLayout {
    pub fn new(width: f32, height: f32) -> Self {
        // Formatos distintos, nunca o volume de texto. A nota padrão (320×240) tem escala
        // intermediária; a composição 4×4 usa os 28 DIP da direção Editorial.
        let (pad, top, gap, font_dip) = if width >= 360.0 && height >= 320.0 {
            (28.0, 20.0, 26.0, 28.0)
        } else if width >= 288.0 && height >= 200.0 {
            (24.0, 12.0, 16.0, 22.0)
        } else {
            (16.0, 6.0, 6.0, 16.0)
        };
        let menu_size = 28.0;
        let caption_bottom = top + 32.0;
        let content_top = caption_bottom + gap;
        // Rodapé estável: o ack/erro de salvamento não desloca o texto ou o caret.
        let status_top = (height - 28.0).max(content_top);
        Self {
            radius: if width.min(height) < 188.0 {
                14.0
            } else {
                20.0
            },
            caption_bottom,
            menu: RectDip {
                x: (width - pad - menu_size).max(0.0),
                y: top + 2.0,
                w: menu_size,
                h: menu_size,
            },
            label: RectDip {
                x: pad,
                y: top + 8.0,
                w: (width - 2.0 * pad - menu_size - 12.0).max(0.0),
                h: 20.0,
            },
            content: RectDip {
                x: pad,
                y: content_top,
                w: (width - 2.0 * pad).max(1.0),
                h: (status_top - 8.0 - content_top).max(1.0),
            },
            status: RectDip {
                x: 12.0,
                y: status_top,
                w: (width - 24.0).max(1.0),
                h: 20.0,
            },
            font_dip,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{field_rect, layout};
    use pod_model::widget::{Element, LocalId};

    #[test]
    fn input_content_and_status_remain_separate_at_supported_sizes() {
        for (w, h) in [
            (160.0, 120.0),
            (188.0, 188.0),
            (320.0, 240.0),
            (388.0, 388.0),
            (800.0, 600.0),
        ] {
            let n = NoteLayout::new(w, h);
            assert!(n.menu.y + n.menu.h <= n.caption_bottom);
            assert!(n.menu.x + n.menu.w <= w);
            assert!(n.content.y > n.caption_bottom);
            let view = Element::Surface(vec![
                Element::TextField {
                    id: LocalId(1),
                    text: String::new(),
                },
                Element::Label("Limite de texto".into()),
            ]);
            let placed = layout(&view, n.content);
            let field = field_rect(&placed, LocalId(1)).unwrap();
            assert!(field.h > 0.0);
            assert!(placed.iter().all(|p| p.rect.y + p.rect.h <= n.status.y));
            assert!(n.status.y + n.status.h <= h);
        }
    }
}
