//! Mover e redimensionar pelo teclado (Alt+F7 / Alt+F8 ou pelo menu da nota). A janela sem moldura
//! não tem menu de sistema, então estes comandos são a alternativa ao arrasto. Lógica pura.
//!
//! Setas movem/redimensionam em passos de 8 DIPs (32 com Shift), Enter confirma e Esc cancela.

use pod_model::geometry::{dip_to_px, RectPx};

pub const STEP_DIP: f64 = 8.0;
pub const BIG_STEP_DIP: f64 = 32.0;
/// Quanto da nota precisa continuar sobre algum monitor (px) para ela não se perder fora da tela.
const MIN_VISIBLE_W: i32 = 64;
const MIN_VISIBLE_H: i32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMode {
    Move,
    Resize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    Left,
    Up,
    Right,
    Down,
}

impl Arrow {
    pub fn from_vk(vk: usize) -> Option<Self> {
        match vk {
            0x25 => Some(Arrow::Left),
            0x26 => Some(Arrow::Up),
            0x27 => Some(Arrow::Right),
            0x28 => Some(Arrow::Down),
            _ => None,
        }
    }
}

/// Aplica um passo de seta. Mover desloca a janela; redimensionar mexe na borda direita/inferior
/// (esquerda/cima diminuem, direita/baixo aumentam), respeitando o tamanho mínimo em pixels.
pub fn step(rect: RectPx, mode: KeyMode, arrow: Arrow, big: bool, dpi: u32, min_w_px: i32, min_h_px: i32) -> RectPx {
    let d = dip_to_px(if big { BIG_STEP_DIP } else { STEP_DIP }, dpi);
    let (dx, dy) = match arrow {
        Arrow::Left => (-d, 0),
        Arrow::Right => (d, 0),
        Arrow::Up => (0, -d),
        Arrow::Down => (0, d),
    };
    match mode {
        KeyMode::Move => RectPx { left: rect.left + dx, top: rect.top + dy, right: rect.right + dx, bottom: rect.bottom + dy },
        KeyMode::Resize => RectPx {
            left: rect.left,
            top: rect.top,
            right: (rect.right + dx).max(rect.left + min_w_px),
            bottom: (rect.bottom + dy).max(rect.top + min_h_px),
        },
    }
}

/// A janela continua alcançável? Precisa de uma parte mínima sobre algum monitor (a faixa de
/// movimentação fica ao alcance do mouse e da bandeja).
pub fn reachable(rect: RectPx, monitors: &[RectPx]) -> bool {
    monitors.iter().any(|m| {
        let w = rect.right.min(m.right) - rect.left.max(m.left);
        let h = rect.bottom.min(m.bottom) - rect.top.max(m.top);
        w >= MIN_VISIBLE_W.min(rect.width()) && h >= MIN_VISIBLE_H.min(rect.height())
    })
}

pub fn status_text(mode: KeyMode) -> &'static str {
    match mode {
        KeyMode::Move => "Mover: setas \u{b7} Enter confirma \u{b7} Esc cancela",
        KeyMode::Resize => "Redimensionar: setas \u{b7} Enter \u{b7} Esc",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: RectPx = RectPx { left: 100, top: 100, right: 420, bottom: 340 };

    #[test]
    fn move_steps_scale_with_dpi_and_shift_makes_them_bigger() {
        let m = |dpi, big| step(R, KeyMode::Move, Arrow::Right, big, dpi, 160, 120);
        assert_eq!(m(96, false), RectPx { left: 108, top: 100, right: 428, bottom: 340 });
        assert_eq!(m(192, false).left, 116, "8 DIP a 200% = 16 px");
        assert_eq!(m(96, true).left, 132, "Shift: 32 DIP");
        assert_eq!(step(R, KeyMode::Move, Arrow::Up, false, 96, 160, 120).top, 92);
        assert_eq!(step(R, KeyMode::Move, Arrow::Left, false, 96, 160, 120).right, 412);
    }

    #[test]
    fn moving_keeps_the_size() {
        for a in [Arrow::Left, Arrow::Up, Arrow::Right, Arrow::Down] {
            let r = step(R, KeyMode::Move, a, true, 144, 160, 120);
            assert_eq!((r.width(), r.height()), (R.width(), R.height()));
        }
    }

    #[test]
    fn resize_moves_the_bottom_right_corner_only() {
        let g = step(R, KeyMode::Resize, Arrow::Right, false, 96, 160, 120);
        assert_eq!((g.left, g.top, g.right, g.bottom), (100, 100, 428, 340));
        let s = step(R, KeyMode::Resize, Arrow::Up, false, 96, 160, 120);
        assert_eq!((s.left, s.top, s.right, s.bottom), (100, 100, 420, 332));
    }

    #[test]
    fn resize_never_goes_below_the_minimum() {
        let mut r = R;
        for _ in 0..100 {
            r = step(r, KeyMode::Resize, Arrow::Left, true, 96, 160, 120);
            r = step(r, KeyMode::Resize, Arrow::Up, true, 96, 160, 120);
        }
        assert_eq!((r.width(), r.height()), (160, 120));
        assert_eq!((r.left, r.top), (100, 100), "o canto superior esquerdo não se move");
    }

    #[test]
    fn a_note_cannot_be_pushed_off_every_monitor() {
        let mons = [RectPx { left: 0, top: 0, right: 1920, bottom: 1080 }];
        assert!(reachable(R, &mons));
        assert!(reachable(RectPx { left: 1880, top: 500, right: 2200, bottom: 740 }, &mons) == false, "só 40 px visíveis");
        assert!(reachable(RectPx { left: 1800, top: 500, right: 2120, bottom: 740 }, &mons), "120 px visíveis bastam");
        assert!(!reachable(RectPx { left: 100, top: 1070, right: 420, bottom: 1310 }, &mons), "só 10 px de altura");
        assert!(!reachable(R, &[]), "sem monitores, nada é alcançável");
        // Vários monitores com origem negativa.
        let two = [RectPx { left: -1920, top: 0, right: 0, bottom: 1080 }, mons[0]];
        assert!(reachable(RectPx { left: -300, top: 100, right: 20, bottom: 340 }, &two));
    }

    #[test]
    fn small_windows_need_only_their_own_size_visible() {
        let mons = [RectPx { left: 0, top: 0, right: 1000, bottom: 1000 }];
        assert!(reachable(RectPx { left: 990, top: 10, right: 1030, bottom: 60 }, &mons) == false);
        assert!(reachable(RectPx { left: 10, top: 10, right: 50, bottom: 50 }, &mons));
    }

    #[test]
    fn arrow_keys_map_from_virtual_keys() {
        assert_eq!(Arrow::from_vk(0x25), Some(Arrow::Left));
        assert_eq!(Arrow::from_vk(0x28), Some(Arrow::Down));
        assert_eq!(Arrow::from_vk(0x41), None);
    }
}
