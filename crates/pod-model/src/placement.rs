//! Placement preferido (durável) vs efetivo (derivado), identificação aproximada de monitores e
//! remapeamento. Puro: sem Win32 nem filesystem (ARCHITECTURE §10).
//!
//! Regras: o preferido nunca é reescrito por uma mudança de topologia; o efetivo é recalculado.
//! `HMONITOR`, índice e `DISPLAY1` não são identidade: usa-se caminho do dispositivo, identidade
//! auxiliar (EDID) e topologia, e uma correspondência ambígua nunca é tratada como definitiva.

use serde::{Deserialize, Serialize};

use crate::geometry::RectPx;

/// Menor tamanho aceitável em situação normal vem do widget; este é o piso de emergência em áreas
/// excepcionalmente pequenas (conteúdo rolável, recuperação pela bandeja).
pub const EMERGENCY_MIN_DIP: (f64, f64) = (80.0, 60.0);
/// Deslocamento entre notas remapeadas para o monitor principal.
pub const CASCADE_STEP_DIP: f64 = 24.0;
const REF_TOLERANCE_DIP: f64 = 0.5;

/// Pistas para reencontrar um monitor. Nenhuma é garantia eterna.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MonitorHint {
    /// `monitorDevicePath` do `DisplayConfigGetDeviceInfo`: pista principal.
    #[serde(default)]
    pub device_path: String,
    /// Fabricante e produto do EDID (por exemplo `DEL:A0B1`).
    #[serde(default)]
    pub edid_id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub connection: Option<String>,
    /// Geometria conhecida no desktop virtual quando foi gravado (px): desempata monitores idênticos.
    pub known_left: i32,
    pub known_top: i32,
    pub known_width: i32,
    pub known_height: i32,
}

/// Posição e tamanho que o usuário escolheu, relativos à área útil de um monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreferredPlacement {
    #[serde(default)]
    pub monitor: Option<MonitorHint>,
    /// Deslocamento a partir do canto da área útil, em DIPs do monitor.
    pub offset_x_dip: f64,
    pub offset_y_dip: f64,
    pub width_dip: f32,
    pub height_dip: f32,
    /// Posição normalizada no espaço livre (0..=1) e dimensões da área útil de referência: usadas
    /// quando a área útil mudou.
    pub ratio_x: f64,
    pub ratio_y: f64,
    pub ref_work_w_dip: f64,
    pub ref_work_h_dip: f64,
    /// Migração do schema 1: posição absoluta no desktop virtual, sem monitor identificado.
    /// Resolvida em tempo de execução e descartada no primeiro placement gravado depois.
    #[serde(default)]
    pub legacy_abs: Option<LegacyAbs>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LegacyAbs {
    pub x: i32,
    pub y: i32,
}

impl PreferredPlacement {
    /// Todos os números finitos e tamanhos positivos (validação de arquivo).
    pub fn is_valid(&self) -> bool {
        let finite = [self.offset_x_dip, self.offset_y_dip, self.ratio_x, self.ratio_y, self.ref_work_w_dip, self.ref_work_h_dip]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.0);
        let sizes = [self.width_dip, self.height_dip].iter().all(|v| v.is_finite() && *v > 0.0 && *v <= 1_000_000.0);
        finite && sizes && self.ref_work_w_dip >= 0.0 && self.ref_work_h_dip >= 0.0
    }

    /// Posição absoluta do schema 1, sem monitor.
    pub fn from_legacy(x: i32, y: i32, width_dip: f32, height_dip: f32) -> Self {
        Self {
            monitor: None,
            offset_x_dip: 0.0,
            offset_y_dip: 0.0,
            width_dip,
            height_dip,
            ratio_x: 0.0,
            ratio_y: 0.0,
            ref_work_w_dip: 0.0,
            ref_work_h_dip: 0.0,
            legacy_abs: Some(LegacyAbs { x, y }),
        }
    }
}

/// Um monitor como o sistema o descreve agora (não é gravado).
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub device_path: String,
    pub edid_id: Option<String>,
    pub name: String,
    pub connection: Option<String>,
    /// Retângulo total e área útil, em pixels físicos do desktop virtual (origem pode ser negativa).
    pub bounds: RectPx,
    pub work: RectPx,
    pub dpi: u32,
    pub primary: bool,
}

impl Monitor {
    pub fn work_dip(&self) -> (f64, f64) {
        let s = 96.0 / self.dpi.max(1) as f64;
        (self.work.width() as f64 * s, self.work.height() as f64 * s)
    }

    pub fn hint(&self) -> MonitorHint {
        MonitorHint {
            device_path: self.device_path.clone(),
            edid_id: self.edid_id.clone(),
            name: self.name.clone(),
            connection: self.connection.clone(),
            known_left: self.bounds.left,
            known_top: self.bounds.top,
            known_width: self.bounds.width(),
            known_height: self.bounds.height(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Mesmo `device_path`.
    Exact,
    /// Mesmo EDID e inequívoco (único, ou desempatado pela geometria conhecida).
    Auxiliary,
    /// Mesma geometria no desktop virtual, sem identidade melhor.
    Topology,
    /// Nada compatível (ou ambíguo): usar o monitor principal sem declarar correspondência.
    Fallback,
}

/// Encontra o monitor que corresponde a `hint`. Ordem: caminho exato, identidade auxiliar
/// inequívoca, topologia compatível, monitor principal. Ambiguidade nunca vira correspondência.
pub fn match_monitor(hint: &MonitorHint, monitors: &[Monitor]) -> (Option<usize>, MatchKind) {
    if !hint.device_path.is_empty() {
        let hits: Vec<usize> = (0..monitors.len()).filter(|&i| monitors[i].device_path == hint.device_path).collect();
        if hits.len() == 1 {
            return (Some(hits[0]), MatchKind::Exact);
        }
    }
    if let Some(edid) = hint.edid_id.as_deref().filter(|e| !e.is_empty()) {
        let same: Vec<usize> = (0..monitors.len()).filter(|&i| monitors[i].edid_id.as_deref() == Some(edid)).collect();
        match same.len() {
            1 => return (Some(same[0]), MatchKind::Auxiliary),
            n if n > 1 => {
                // Monitores idênticos: só a geometria conhecida pode desempatar, e só se for única.
                let at: Vec<usize> = same
                    .iter()
                    .copied()
                    .filter(|&i| monitors[i].bounds.left == hint.known_left && monitors[i].bounds.top == hint.known_top)
                    .collect();
                if at.len() == 1 {
                    return (Some(at[0]), MatchKind::Auxiliary);
                }
                return (None, MatchKind::Fallback); // ambíguo
            }
            _ => {}
        }
    }
    // Topologia: só quando não há identidade nenhuma a comparar.
    if hint.device_path.is_empty() && hint.edid_id.is_none() {
        let at: Vec<usize> = (0..monitors.len())
            .filter(|&i| {
                let b = monitors[i].bounds;
                b.left == hint.known_left
                    && b.top == hint.known_top
                    && b.width() == hint.known_width
                    && b.height() == hint.known_height
            })
            .collect();
        if at.len() == 1 {
            return (Some(at[0]), MatchKind::Topology);
        }
    }
    (None, MatchKind::Fallback)
}

pub fn primary(monitors: &[Monitor]) -> Option<usize> {
    monitors.iter().position(|m| m.primary).or(if monitors.is_empty() { None } else { Some(0) })
}

/// Índice do monitor que contém o ponto, ou o de maior sobreposição com o retângulo.
pub fn monitor_for_rect(rect: RectPx, monitors: &[Monitor]) -> Option<usize> {
    let overlap = |m: &Monitor| {
        let w = (rect.right.min(m.bounds.right) - rect.left.max(m.bounds.left)).max(0) as i64;
        let h = (rect.bottom.min(m.bounds.bottom) - rect.top.max(m.bounds.top)).max(0) as i64;
        w * h
    };
    let best = (0..monitors.len()).max_by_key(|&i| overlap(&monitors[i]))?;
    if overlap(&monitors[best]) > 0 {
        return Some(best);
    }
    // Sem sobreposição: o monitor mais próximo do centro.
    let (cx, cy) = ((rect.left + rect.right) as i64 / 2, (rect.top + rect.bottom) as i64 / 2);
    (0..monitors.len()).min_by_key(|&i| {
        let b = monitors[i].bounds;
        let dx = (cx - cx.clamp(b.left as i64, b.right as i64)).abs();
        let dy = (cy - cy.clamp(b.top as i64, b.bottom as i64)).abs();
        dx * dx + dy * dy
    })
}

/// Constrói o placement preferido a partir de onde a janela está agora (movimento confirmado).
pub fn to_preferred(window: RectPx, window_dpi: u32, monitor: &Monitor) -> PreferredPlacement {
    let to_dip = |px: i32, dpi: u32| px as f64 * 96.0 / dpi.max(1) as f64;
    let (work_w, work_h) = monitor.work_dip();
    let (w, h) = (to_dip(window.width(), window_dpi), to_dip(window.height(), window_dpi));
    let off_x = to_dip(window.left - monitor.work.left, monitor.dpi);
    let off_y = to_dip(window.top - monitor.work.top, monitor.dpi);
    let ratio = |off: f64, free: f64| if free > 0.5 { (off / free).clamp(0.0, 1.0) } else { 0.0 };
    PreferredPlacement {
        monitor: Some(monitor.hint()),
        offset_x_dip: off_x,
        offset_y_dip: off_y,
        width_dip: w as f32,
        height_dip: h as f32,
        ratio_x: ratio(off_x, work_w - w),
        ratio_y: ratio(off_y, work_h - h),
        ref_work_w_dip: work_w,
        ref_work_h_dip: work_h,
        legacy_abs: None,
    }
}

/// Retângulo efetivo (px físicos) de `pref` no monitor `mon`. `position_override_dip` substitui a
/// posição (usado pela cascata); o tamanho preferido é preservado se couber.
pub fn effective_rect(pref: &PreferredPlacement, mon: &Monitor, position_override_dip: Option<(f64, f64)>) -> RectPx {
    let (work_w, work_h) = mon.work_dip();
    // Tamanho: o preferido, reduzido ao que cabe; nunca abaixo do piso de emergência.
    let fit = |want: f64, avail: f64, floor: f64| want.min(avail.max(floor)).max(floor.min(avail.max(1.0)));
    let w = fit(pref.width_dip as f64, work_w, EMERGENCY_MIN_DIP.0);
    let h = fit(pref.height_dip as f64, work_h, EMERGENCY_MIN_DIP.1);

    let (off_x, off_y) = match position_override_dip {
        Some(p) => p,
        None => {
            let same_area = (pref.ref_work_w_dip - work_w).abs() <= REF_TOLERANCE_DIP
                && (pref.ref_work_h_dip - work_h).abs() <= REF_TOLERANCE_DIP;
            if same_area {
                (pref.offset_x_dip, pref.offset_y_dip)
            } else {
                // A área útil mudou: usa as razões sobre o espaço livre atual.
                (pref.ratio_x * (work_w - w).max(0.0), pref.ratio_y * (work_h - h).max(0.0))
            }
        }
    };
    // Sempre inteiramente dentro da área útil (a faixa de movimentação fica alcançável).
    let x = off_x.clamp(0.0, (work_w - w).max(0.0));
    let y = off_y.clamp(0.0, (work_h - h).max(0.0));
    let px = |dip: f64| (dip * mon.dpi as f64 / 96.0).round() as i32;
    let left = mon.work.left + px(x);
    let top = mon.work.top + px(y);
    RectPx { left, top, right: left + px(w).max(1), bottom: top + px(h).max(1) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementKind {
    /// No monitor preferido (identificado por `MatchKind`).
    OnPreferred(MatchKind),
    /// Monitor ausente ou ambíguo: remapeado para o principal.
    Remapped,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resolved {
    pub monitor: usize,
    pub rect: RectPx,
    pub kind: PlacementKind,
}

/// Resolve o placement efetivo de todas as notas. Entradas na ordem recebida não importam: as
/// remapeadas são organizadas em cascata no principal por ordem estável de `key` (o ID).
pub fn resolve_all<K: Ord + Clone>(items: &[(K, PreferredPlacement)], monitors: &[Monitor]) -> Vec<(K, Resolved)> {
    let Some(primary_idx) = primary(monitors) else {
        return Vec::new();
    };
    let mut sorted: Vec<&(K, PreferredPlacement)> = items.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = Vec::with_capacity(sorted.len());
    let mut cascade_index = 0usize;
    for (key, pref) in sorted {
        let resolved = match locate(pref, monitors) {
            Some((idx, kind, pref)) => Resolved {
                monitor: idx,
                rect: effective_rect(&pref, &monitors[idx], None),
                kind: PlacementKind::OnPreferred(kind),
            },
            // Monitor ausente (outro monitor, sessão remota, troca de placa), mas com a posição
            // relativa conhecida: mesma posição proporcional no principal, sem empilhar os cartões.
            None if pref.ref_work_w_dip > 0.0 && pref.ref_work_h_dip > 0.0 => {
                let mon = &monitors[primary_idx];
                let mut rect = effective_rect(pref, mon, None);
                // Dois caindo no mesmo ponto: o seguinte desce um degrau da cascata.
                let step = (CASCADE_STEP_DIP * mon.dpi as f64 / 96.0).round() as i32;
                while out.iter().any(|(_, r): &(K, Resolved)| r.monitor == primary_idx && r.rect.left == rect.left && r.rect.top == rect.top)
                    && rect.right + step <= mon.work.right
                    && rect.bottom + step <= mon.work.bottom
                {
                    rect = RectPx { left: rect.left + step, top: rect.top + step, right: rect.right + step, bottom: rect.bottom + step };
                }
                Resolved { monitor: primary_idx, rect, kind: PlacementKind::Remapped }
            }
            None => {
                let mon = &monitors[primary_idx];
                let (work_w, work_h) = mon.work_dip();
                // Cascata com volta ao início quando passaria da metade da área útil.
                let max_steps = (((work_w.min(work_h) / 2.0) / CASCADE_STEP_DIP).floor() as usize).max(1);
                let step = (cascade_index % max_steps) as f64 * CASCADE_STEP_DIP;
                cascade_index += 1;
                Resolved {
                    monitor: primary_idx,
                    rect: effective_rect(pref, mon, Some((step, step))),
                    kind: PlacementKind::Remapped,
                }
            }
        };
        out.push((key.clone(), resolved));
    }
    out
}

/// Monitor preferido de `pref` entre os atuais e o placement já em forma relativa. Para o schema 1
/// (posição absoluta, sem identidade) o monitor que contém o ponto vira o preferido, por topologia.
pub fn locate(pref: &PreferredPlacement, monitors: &[Monitor]) -> Option<(usize, MatchKind, PreferredPlacement)> {
    if let Some(hint) = &pref.monitor {
        let (idx, kind) = match_monitor(hint, monitors);
        return idx.map(|i| (i, kind, pref.clone()));
    }
    let abs = pref.legacy_abs?;
    let probe = RectPx { left: abs.x, top: abs.y, right: abs.x + 1, bottom: abs.y + 1 };
    let idx = monitor_for_rect(probe, monitors)?;
    let mon = &monitors[idx];
    let b = mon.bounds;
    if !(abs.x >= b.left && abs.x < b.right && abs.y >= b.top && abs.y < b.bottom) {
        return None; // fora de todos os monitores: remapear
    }
    let px = |dip: f32| (dip as f64 * mon.dpi as f64 / 96.0).round() as i32;
    let window = RectPx { left: abs.x, top: abs.y, right: abs.x + px(pref.width_dip), bottom: abs.y + px(pref.height_dip) };
    Some((idx, MatchKind::Topology, to_preferred(window, mon.dpi, mon)))
}

#[cfg(test)]
mod tests;

