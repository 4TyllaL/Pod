//! Formatos em grade dos widgets (ADR-0001, D2): uma célula de 88 DIP e 12 DIP entre células.
//! O tamanho de uma janela de widget de formato fixo é exatamente `size_dip()`.

use crate::widget::SizeDip;

pub const CELL_DIP: f32 = 88.0;
pub const GAP_DIP: f32 = 12.0;

/// Colunas × linhas de células.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Shape {
    pub cols: u8,
    pub rows: u8,
}

impl Shape {
    pub const S1X1: Shape = Shape { cols: 1, rows: 1 };
    pub const S2X1: Shape = Shape { cols: 2, rows: 1 };
    pub const S3X1: Shape = Shape { cols: 3, rows: 1 };
    pub const S4X1: Shape = Shape { cols: 4, rows: 1 };
    pub const S2X2: Shape = Shape { cols: 2, rows: 2 };
    /// Colunas altas para widgets de lista (tarefas, notícias, calendário com agenda).
    pub const S2X3: Shape = Shape { cols: 2, rows: 3 };
    pub const S2X4: Shape = Shape { cols: 2, rows: 4 };
    pub const S4X2: Shape = Shape { cols: 4, rows: 2 };
    pub const S4X4: Shape = Shape { cols: 4, rows: 4 };

    /// Os formatos permitidos. As quatro larguras de uma faixa viram 1×1…4×1; 2×2 e 4×2 atendem
    /// cartões ricos.
    pub const ALL: [Shape; 9] = [
        Self::S1X1,
        Self::S2X1,
        Self::S3X1,
        Self::S4X1,
        Self::S2X2,
        Self::S2X3,
        Self::S2X4,
        Self::S4X2,
        Self::S4X4,
    ];

    /// Formato de desenho: os largos (3 ou 4 colunas) usam o layout de 2 colunas com a mesma altura,
    /// esticado na largura real. Assim todo widget ganha expansão horizontal sem uma view nova.
    pub fn design(self) -> Shape {
        Shape { cols: self.cols.min(2), rows: self.rows }
    }

    pub fn is_allowed(self) -> bool {
        Self::ALL.contains(&self)
    }

    pub fn size_dip(self) -> SizeDip {
        let span = |n: u8| n as f32 * CELL_DIP + (n.saturating_sub(1)) as f32 * GAP_DIP;
        SizeDip { w: span(self.cols), h: span(self.rows) }
    }

    /// Nome estilo macOS de `self` entre os formatos `all` de um widget, por área crescente:
    /// dois formatos = Pequeno e Grande; três = Pequeno, Médio e Grande; quatro ou mais ganham
    /// "Extra grande" no maior. Um formato só não tem nome (sem escolha a fazer).
    pub fn size_name(self, all: &[Shape]) -> Option<&'static str> {
        let mut sorted: Vec<Shape> = all.to_vec();
        sorted.sort_by_key(|s| (s.cols as u32 * s.rows as u32, s.cols));
        sorted.dedup();
        let rank = sorted.iter().position(|s| *s == self)?;
        let names: &[&'static str] = match sorted.len() {
            0 | 1 => return None,
            2 => &["Pequeno", "Grande"],
            3 => &["Pequeno", "M\u{e9}dio", "Grande"],
            _ => &["Pequeno", "M\u{e9}dio", "Grande", "Extra grande"],
        };
        // Com mais de 4 formatos os intermediários repetem "Grande" até o último ser "Extra grande".
        let i = if sorted.len() > 4 { rank.min(3) } else { rank };
        names.get(i).copied()
    }

    /// "2x1" (forma gravada no arquivo e usada em menus).
    pub fn key(self) -> String {
        format!("{}x{}", self.cols, self.rows)
    }

    /// Só aceita formatos permitidos; qualquer outra coisa é `None` (nunca um tamanho inventado).
    pub fn parse(s: &str) -> Option<Shape> {
        let (c, r) = s.trim().split_once('x')?;
        let shape = Shape { cols: c.parse().ok()?, rows: r.parse().ok()? };
        shape.is_allowed().then_some(shape)
    }
}

impl std::fmt::Display for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\u{d7}{}", self.cols, self.rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_names_follow_area_order() {
        let three = [Shape::S2X2, Shape::S1X1, Shape::S2X1];
        assert_eq!(Shape::S1X1.size_name(&three), Some("Pequeno"));
        assert_eq!(Shape::S2X1.size_name(&three), Some("M\u{e9}dio"));
        assert_eq!(Shape::S2X2.size_name(&three), Some("Grande"));
        let two = [Shape::S1X1, Shape::S2X2];
        assert_eq!(Shape::S2X2.size_name(&two), Some("Grande"));
        assert_eq!(Shape::S1X1.size_name(&[Shape::S1X1]), None, "um formato s\u{f3}: sem nome");
        assert_eq!(Shape::S4X4.size_name(&[Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S4X4]), Some("Extra grande"));
        assert_eq!(Shape::S4X1.size_name(&three), None, "formato que o widget n\u{e3}o tem");
    }

    #[test]
    fn sizes_follow_the_grid_formula() {
        assert_eq!(Shape::S1X1.size_dip(), SizeDip { w: 88.0, h: 88.0 });
        assert_eq!(Shape::S2X1.size_dip(), SizeDip { w: 188.0, h: 88.0 });
        assert_eq!(Shape::S2X2.size_dip(), SizeDip { w: 188.0, h: 188.0 });
        assert_eq!(Shape::S4X2.size_dip(), SizeDip { w: 388.0, h: 188.0 });
        assert_eq!(Shape::S4X4.size_dip(), SizeDip { w: 388.0, h: 388.0 });
    }

    #[test]
    fn adjacent_shapes_line_up_on_the_same_grid() {
        // Dois 2×1 lado a lado com o espaço entre células ocupam exatamente um 4×1.
        let two = Shape::S2X1.size_dip().w;
        assert_eq!(two + GAP_DIP + two, Shape::S4X1.size_dip().w);
        // Dois 2×2 empilhados ocupam um 2×4... e um 4×2 mais um 4×2 ocupam um 4×4.
        assert_eq!(Shape::S4X2.size_dip().h * 2.0 + GAP_DIP, Shape::S4X4.size_dip().h);
    }

    #[test]
    fn keys_roundtrip_and_reject_anything_else() {
        for s in Shape::ALL {
            assert_eq!(Shape::parse(&s.key()), Some(s));
        }
        assert_eq!(Shape::parse(" 2x1 "), Some(Shape::S2X1));
        for bad in ["", "x", "2x", "x2", "5x5", "0x0", "3x3", "2X1", "2 x 1", "-1x1", "300x1", "2x1x1"] {
            assert_eq!(Shape::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn display_uses_the_multiplication_sign() {
        assert_eq!(Shape::S2X1.to_string(), "2\u{d7}1");
    }
}
