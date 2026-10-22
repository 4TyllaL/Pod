use super::*;

fn rect(l: i32, t: i32, r: i32, b: i32) -> RectPx {
    RectPx { left: l, top: t, right: r, bottom: b }
}

fn mon(path: &str, edid: Option<&str>, bounds: RectPx, dpi: u32, primary: bool) -> Monitor {
    // A área útil perde 40 px embaixo (barra de tarefas) no monitor principal.
    let work = if primary { rect(bounds.left, bounds.top, bounds.right, bounds.bottom - 40) } else { bounds };
    Monitor {
        device_path: path.into(),
        edid_id: edid.map(Into::into),
        name: path.into(),
        connection: None,
        bounds,
        work,
        dpi,
        primary,
    }
}

/// Três monitores como no desktop de teste: o secundário esquerdo tem origem negativa.
fn desk() -> Vec<Monitor> {
    vec![
        mon("A", Some("DEL:1"), rect(0, 0, 1920, 1080), 96, true),
        mon("B", Some("LGD:2"), rect(-1920, 0, 0, 1080), 96, false),
        mon("C", Some("SAM:3"), rect(1920, 0, 3840, 1080), 96, false),
    ]
}

fn pref_on(m: &Monitor, window: RectPx) -> PreferredPlacement {
    to_preferred(window, m.dpi, m)
}

// ---- identificação -------------------------------------------------------------------------

#[test]
fn exact_device_path_wins() {
    let ms = desk();
    let hint = ms[1].hint();
    assert_eq!(match_monitor(&hint, &ms), (Some(1), MatchKind::Exact));
}

#[test]
fn a_new_device_path_is_found_by_unique_edid() {
    // Dock ou porta diferentes mudam o caminho, não o EDID.
    let ms = desk();
    let mut hint = ms[2].hint();
    hint.device_path = "caminho-antigo".into();
    assert_eq!(match_monitor(&hint, &ms), (Some(2), MatchKind::Auxiliary));
}

#[test]
fn identical_monitors_are_disambiguated_only_by_known_geometry() {
    let twins = vec![
        mon("P1", Some("DEL:9"), rect(0, 0, 1920, 1080), 96, true),
        mon("P2", Some("DEL:9"), rect(1920, 0, 3840, 1080), 96, false),
    ];
    let mut hint = twins[1].hint();
    hint.device_path = "mudou".into();
    assert_eq!(match_monitor(&hint, &twins), (Some(1), MatchKind::Auxiliary), "geometria única desempata");

    // Trocaram de lugar: a geometria conhecida aponta para um que tem o mesmo EDID, ainda único.
    hint.known_left = 1920;
    assert_eq!(match_monitor(&hint, &twins).1, MatchKind::Auxiliary);

    // Geometria que não existe mais: ambíguo, nunca adivinhado.
    hint.known_left = 5000;
    assert_eq!(match_monitor(&hint, &twins), (None, MatchKind::Fallback));
}

#[test]
fn topology_only_when_there_is_no_identity_at_all() {
    let ms = desk();
    let hint = MonitorHint { known_left: 1920, known_top: 0, known_width: 1920, known_height: 1080, ..Default::default() };
    assert_eq!(match_monitor(&hint, &ms), (Some(2), MatchKind::Topology));
    // Com identidade que não bate, a topologia NÃO é usada para "adivinhar".
    let with_identity = MonitorHint { edid_id: Some("XXX:0".into()), ..hint };
    assert_eq!(match_monitor(&with_identity, &ms), (None, MatchKind::Fallback));
}

// ---- remapeamento --------------------------------------------------------------------------

#[test]
fn removed_monitor_remaps_to_primary_proportionally_in_stable_id_order() {
    let full = desk();
    let b = &full[1];
    let items: Vec<(u32, PreferredPlacement)> = [30u32, 10, 20]
        .iter()
        .map(|&id| (id, pref_on(b, rect(-1500, 100, -1180, 340))))
        .collect();
    let without_b: Vec<Monitor> = vec![full[0].clone(), full[2].clone()];
    let r = resolve_all(&items, &without_b);
    let ids: Vec<u32> = r.iter().map(|(k, _)| *k).collect();
    assert_eq!(ids, vec![10, 20, 30], "ordem estável por ID, independente da ordem de entrada");
    assert!(r.iter().all(|(_, x)| x.kind == PlacementKind::Remapped && x.monitor == 0));
    // Mesma posição proporcional no principal; as que caem no mesmo ponto descem um degrau.
    let lefts: Vec<i32> = r.iter().map(|(_, x)| x.rect.left).collect();
    assert_eq!(lefts, vec![lefts[0], lefts[0] + 24, lefts[0] + 48]);
    assert!(lefts[0] > 0, "não volta para o canto: usa a razão guardada");
    // Tudo dentro da área útil do principal (que perdeu a barra de tarefas).
    assert!(r.iter().all(|(_, x)| x.rect.bottom <= without_b[0].work.bottom && x.rect.right <= 1920));
}

#[test]
fn reconnecting_restores_the_original_position_because_preferred_never_changed() {
    let full = desk();
    let b = &full[1];
    let items = vec![(1u32, pref_on(b, rect(-1600, 200, -1280, 440)))];
    let before = resolve_all(&items, &full)[0].1;
    assert_eq!(before.kind, PlacementKind::OnPreferred(MatchKind::Exact));
    assert_eq!(before.rect, rect(-1600, 200, -1280, 440));

    let away = resolve_all(&items, &[full[0].clone(), full[2].clone()])[0].1;
    assert_eq!(away.kind, PlacementKind::Remapped);

    let back = resolve_all(&items, &full)[0].1;
    assert_eq!(back, before, "voltar o monitor restaura a posição, sem deriva");
}

#[test]
fn no_monitors_means_nothing_to_resolve() {
    let items = vec![(1u32, pref_on(&desk()[0], rect(0, 0, 320, 240)))];
    assert!(resolve_all(&items, &[]).is_empty());
}

// ---- área útil e DPI -----------------------------------------------------------------------

#[test]
fn work_area_change_uses_ratios_and_stays_inside() {
    let big = mon("A", Some("DEL:1"), rect(0, 0, 1920, 1080), 96, true);
    let p = pref_on(&big, rect(1500, 700, 1820, 940)); // canto inferior direito
    let small = mon("A", Some("DEL:1"), rect(0, 0, 1024, 768), 96, true);
    let r = effective_rect(&p, &small, None);
    assert!(r.left >= 0 && r.top >= 0 && r.right <= small.work.right && r.bottom <= small.work.bottom, "{r:?}");
    assert_eq!((r.width(), r.height()), (320, 240), "o tamanho preferido é preservado se couber");
    // Continua no canto inferior direito, proporcionalmente.
    assert!(r.right > 900 && r.bottom > 650, "{r:?}");
}

#[test]
fn unchanged_work_area_uses_exact_offsets_not_ratios() {
    let m = mon("A", Some("DEL:1"), rect(0, 0, 1920, 1080), 96, true);
    let win = rect(333, 217, 653, 457);
    assert_eq!(effective_rect(&pref_on(&m, win), &m, None), win);
}

#[test]
fn tiny_work_area_gets_an_emergency_size_inside_bounds() {
    let tiny = mon("T", Some("TNY:1"), rect(0, 0, 100, 80), 96, false);
    let p = pref_on(&desk()[0], rect(0, 0, 320, 240));
    let r = effective_rect(&p, &tiny, None);
    assert!(r.width() > 0 && r.height() > 0);
    assert!(r.left >= 0 && r.top >= 0 && r.right <= 100 && r.bottom <= 80, "{r:?}");
    let degenerate = mon("Z", None, rect(0, 0, 10, 10), 96, false);
    let r = effective_rect(&p, &degenerate, None);
    assert!(r.width() >= 1 && r.height() >= 1, "nunca um retângulo inválido: {r:?}");
}

#[test]
fn mixed_dpi_roundtrip_has_no_drift_and_never_uses_a_global_scale() {
    // B está a 200% e tem origem negativa; A está a 100%.
    let ms = vec![
        mon("A", Some("DEL:1"), rect(0, 0, 1920, 1080), 96, true),
        mon("B", Some("LGD:2"), rect(-3840, 0, 0, 2160), 192, false),
    ];
    let win_on_b = rect(-3000, 400, -2360, 880); // 640x480 px a 192 dpi = 320x240 dip
    let pref = pref_on(&ms[1], win_on_b);
    assert!((pref.width_dip - 320.0).abs() < 1e-3 && (pref.height_dip - 240.0).abs() < 1e-3);
    let mut r = effective_rect(&pref, &ms[1], None);
    assert_eq!(r, win_on_b);
    // Ida e volta repetida: nenhum acúmulo de arredondamento.
    for _ in 0..200 {
        let p = to_preferred(r, 192, &ms[1]);
        r = effective_rect(&p, &ms[1], None);
    }
    assert_eq!(r, win_on_b);
}

#[test]
fn moving_between_scales_keeps_dip_size() {
    let ms = vec![
        mon("A", Some("DEL:1"), rect(0, 0, 1920, 1080), 96, true),
        mon("B", Some("LGD:2"), rect(1920, 0, 5760, 2160), 192, false),
    ];
    let on_a = pref_on(&ms[0], rect(100, 100, 420, 340));
    let on_b = effective_rect(&PreferredPlacement { monitor: Some(ms[1].hint()), ..on_a.clone() }, &ms[1], None);
    assert_eq!(on_b.width(), 640, "320 dip a 200% = 640 px");
    assert_eq!(on_b.height(), 480);
}

// ---- monitor por retângulo / legado ---------------------------------------------------------

#[test]
fn monitor_for_rect_prefers_overlap_then_nearest() {
    let ms = desk();
    assert_eq!(monitor_for_rect(rect(1800, 100, 2200, 300), &ms), Some(2), "mais pixels em C");
    assert_eq!(monitor_for_rect(rect(-200, 0, -100, 100), &ms), Some(1));
    // Fora de todos: o mais próximo (abaixo de A).
    assert_eq!(monitor_for_rect(rect(500, 5000, 600, 5100), &ms), Some(0));
}

#[test]
fn schema1_absolute_position_resolves_to_the_containing_monitor() {
    let ms = desk();
    let legacy = PreferredPlacement::from_legacy(-1500, 100, 320.0, 240.0);
    let r = resolve_all(&[(1u32, legacy)], &ms)[0].1;
    assert_eq!(r.monitor, 1);
    assert_eq!(r.kind, PlacementKind::OnPreferred(MatchKind::Topology));
    assert_eq!(r.rect, rect(-1500, 100, -1180, 340));
}

#[test]
fn schema1_position_outside_every_monitor_is_remapped_not_lost() {
    let ms = desk();
    let legacy = PreferredPlacement::from_legacy(9000, 9000, 320.0, 240.0);
    let r = resolve_all(&[(1u32, legacy)], &ms)[0].1;
    assert_eq!(r.kind, PlacementKind::Remapped);
    assert!(r.rect.left >= 0 && r.rect.right <= 1920);
}

#[test]
fn locate_converts_legacy_into_a_monitor_relative_preference() {
    let ms = desk();
    let legacy = PreferredPlacement::from_legacy(100, 100, 320.0, 240.0);
    let (idx, _, converted) = locate(&legacy, &ms).unwrap();
    assert_eq!(idx, 0);
    assert!(converted.legacy_abs.is_none() && converted.monitor.is_some());
    assert!((converted.offset_x_dip - 100.0).abs() < 1e-9);
}

// ---- validação -----------------------------------------------------------------------------

#[test]
fn invalid_numbers_are_detected() {
    let ok = pref_on(&desk()[0], rect(0, 0, 320, 240));
    assert!(ok.is_valid());
    for bad in [f64::NAN, f64::INFINITY, -f64::INFINITY, 1e12] {
        let mut p = ok.clone();
        p.offset_x_dip = bad;
        assert!(!p.is_valid(), "{bad}");
        let mut p = ok.clone();
        p.ratio_y = bad;
        assert!(!p.is_valid(), "{bad}");
    }
    let mut p = ok.clone();
    p.width_dip = 0.0;
    assert!(!p.is_valid());
    p.width_dip = f32::NAN;
    assert!(!p.is_valid());
}
