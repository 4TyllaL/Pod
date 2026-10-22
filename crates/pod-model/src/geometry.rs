//! Geometria e conversões DIP <-> pixel físico.

/// Converte DIPs para pixels físicos (arredondando só na fronteira).
pub fn dip_to_px(dip: f64, dpi: u32) -> i32 {
    (dip * dpi as f64 / 96.0).round() as i32
}

/// Converte pixels físicos para DIPs.
pub fn px_to_dip(px: i32, dpi: u32) -> f64 {
    px as f64 * 96.0 / dpi as f64
}

/// Retângulo em pixels físicos do desktop virtual (origem pode ser negativa).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RectPx {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl RectPx {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_conversion() {
        assert_eq!(dip_to_px(320.0, 96), 320);
        assert_eq!(dip_to_px(320.0, 192), 640);
        assert_eq!(dip_to_px(240.0, 144), 360);
    }

    #[test]
    fn roundtrip_does_not_drift() {
        for dpi in [96, 120, 144, 192] {
            let mut px = dip_to_px(251.0, dpi);
            for _ in 0..100 {
                px = dip_to_px(px_to_dip(px, dpi), dpi);
            }
            assert_eq!(px, dip_to_px(251.0, dpi));
        }
    }
}
