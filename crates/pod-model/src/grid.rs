//! Grade de encaixe dos cartões (ADR-0003, P4): passo de uma célula mais o espaço entre células,
//! contado a partir do canto da área útil do monitor com uma margem de um espaço. Puro, em pixels.

use crate::shape::{CELL_DIP, GAP_DIP};

/// Passo da grade em pixels físicos no DPI dado (célula + espaço).
pub fn pitch_px(dpi: u32) -> f64 {
    (CELL_DIP + GAP_DIP) as f64 * dpi as f64 / 96.0
}

/// Margem da grade em relação à borda da área útil, em pixels físicos.
pub fn margin_px(dpi: u32) -> f64 {
    GAP_DIP as f64 * dpi as f64 / 96.0
}

/// Ponto da grade mais próximo de `v` em um eixo cuja área útil começa em `work_start`.
pub fn snap_axis(v: i32, work_start: i32, dpi: u32) -> i32 {
    let pitch = pitch_px(dpi);
    let origin = work_start as f64 + margin_px(dpi);
    let k = ((v as f64 - origin) / pitch).round().max(0.0);
    (origin + k * pitch).round() as i32
}

/// `v` já está na grade (tolerância de 1 px por arredondamento)?
pub fn is_snapped(v: i32, work_start: i32, dpi: u32) -> bool {
    (snap_axis(v, work_start, dpi) - v).abs() <= 1
}

/// Retângulo em pixels: (esquerda, topo, direita, base).
pub type Box4 = (i32, i32, i32, i32);

fn overlaps(a: Box4, b: Box4) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

/// Primeira posição da grade (linha a linha, da esquerda para a direita, a partir do canto da área
/// útil) onde um cartão de `size` (largura, altura em pixels) cabe inteiro na área útil `work` sem
/// cobrir nenhum retângulo de `occupied`. `None` se não há lugar.
pub fn first_free_slot(work: Box4, size: (i32, i32), dpi: u32, occupied: &[Box4]) -> Option<(i32, i32)> {
    let pitch = pitch_px(dpi);
    let margin = margin_px(dpi);
    let mut row = 0.0;
    loop {
        let top = (work.1 as f64 + margin + row * pitch).round() as i32;
        if top + size.1 > work.3 {
            return None;
        }
        let mut col = 0.0;
        loop {
            let left = (work.0 as f64 + margin + col * pitch).round() as i32;
            if left + size.0 > work.2 {
                break;
            }
            let cand = (left, top, left + size.0, top + size.1);
            if !occupied.iter().any(|o| overlaps(cand, *o)) {
                return Some((left, top));
            }
            col += 1.0;
        }
        row += 1.0;
    }
}

/// Até `max` posições livres da grade para um cartão de `size` pixels, em ordem de linha, sem que as
/// escolhidas se sobreponham entre si nem a `occupied`. Usado para os espaços "+" do modo de edição.
pub fn free_slots(work: Box4, size: (i32, i32), dpi: u32, occupied: &[Box4], max: usize) -> Vec<(i32, i32)> {
    let mut taken: Vec<Box4> = occupied.to_vec();
    let mut out = Vec::new();
    while out.len() < max {
        let Some((l, t)) = first_free_slot(work, size, dpi, &taken) else { break };
        let b = (l, t, l + size.0, t + size.1);
        taken.push(b);
        out.push((l, t));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_and_margin_scale_with_dpi() {
        assert_eq!(pitch_px(96), 100.0);
        assert_eq!(pitch_px(144), 150.0);
        assert_eq!(margin_px(96), 12.0);
        assert_eq!(margin_px(192), 24.0);
    }

    #[test]
    fn snaps_to_the_nearest_grid_point() {
        // 100% DPI, área útil começando em 0: pontos em 12, 112, 212...
        assert_eq!(snap_axis(12, 0, 96), 12);
        assert_eq!(snap_axis(50, 0, 96), 12, "mais perto de 12 do que de 112");
        assert_eq!(snap_axis(63, 0, 96), 112);
        assert_eq!(snap_axis(300, 0, 96), 312);
    }

    #[test]
    fn never_snaps_before_the_first_cell_and_follows_the_work_area() {
        assert_eq!(snap_axis(-500, 0, 96), 12, "antes da margem: primeira célula");
        // Monitor à esquerda com origem negativa.
        assert_eq!(snap_axis(-1900, -1920, 96), -1908);
        assert_eq!(snap_axis(-1920 + 12 + 100, -1920, 96), -1808);
    }

    #[test]
    fn high_dpi_uses_physical_pixels() {
        // 150%: margem 18 px e passo 150 px.
        assert_eq!(snap_axis(100, 0, 144), 168);
        assert_eq!(snap_axis(20, 0, 144), 18);
        assert!(is_snapped(168, 0, 144));
        assert!(!is_snapped(100, 0, 144));
    }

    #[test]
    fn snapping_is_idempotent() {
        for v in [-3000, -17, 0, 5, 99, 1234, 4000] {
            let once = snap_axis(v, 0, 120);
            assert_eq!(snap_axis(once, 0, 120), once);
        }
    }

    #[test]
    fn first_free_slot_fills_row_by_row_and_skips_occupied_cells() {
        let work = (0, 0, 800, 600);
        let size = (188, 88);
        assert_eq!(first_free_slot(work, size, 96, &[]), Some((12, 12)));
        let first = (12, 12, 200, 100);
        assert_eq!(
            first_free_slot(work, size, 96, &[first]),
            Some((212, 12)),
            "o 2x1 ocupa duas células: o próximo ponto livre da grade é o de 212"
        );
        // Linha cheia (largura 800): desce para a próxima linha.
        let full: Vec<Box4> = (0..4).map(|c| (12 + c * 200, 12, 200 + c * 200, 100)).collect();
        assert_eq!(first_free_slot(work, size, 96, &full), Some((12, 112)));
    }

    #[test]
    fn first_free_slot_returns_none_when_nothing_fits() {
        assert_eq!(first_free_slot((0, 0, 100, 100), (188, 88), 96, &[]), None);
        let all = [(0, 0, 800, 600)];
        assert_eq!(first_free_slot((0, 0, 800, 600), (88, 88), 96, &all), None);
    }

    #[test]
    fn first_free_slot_positions_are_on_the_grid() {
        let (x, y) = first_free_slot((-1920, 0, 0, 1000), (188, 88), 96, &[(-1908, 12, -1720, 100)]).unwrap();
        assert!(is_snapped(x, -1920, 96) && is_snapped(y, 0, 96), "({x},{y})");
    }

    #[test]
    fn free_slots_are_distinct_free_and_bounded() {
        let work = (0, 0, 400, 300);
        let occupied = [(12, 12, 100, 100)];
        let slots = free_slots(work, (88, 88), 96, &occupied, 3);
        assert_eq!(slots.len(), 3);
        for (i, a) in slots.iter().enumerate() {
            let ab = (a.0, a.1, a.0 + 88, a.1 + 88);
            assert!(!overlaps(ab, occupied[0]), "{a:?} cobre o ocupado");
            for b in slots.iter().skip(i + 1) {
                assert!(!overlaps(ab, (b.0, b.1, b.0 + 88, b.1 + 88)));
            }
        }
        assert!(free_slots(work, (88, 88), 96, &[(0, 0, 400, 300)], 3).is_empty());
        assert_eq!(free_slots(work, (88, 88), 96, &[], 0).len(), 0);
    }
}
