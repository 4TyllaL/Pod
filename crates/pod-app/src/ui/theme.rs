//! Tema visual: tokens de cor para claro, escuro e alto contraste, e a conta de contraste WCAG que
//! os testes usam para provar legibilidade. Puro (sem Win32): o host escolhe o tema.
//!
//! Direção visual (referências de widgets de desktop): cartões arredondados, hierarquia tipográfica,
//! contorno fino e brilho sutil. Por padrão as superfícies são opacas e legíveis. O material "vidro"
//! (ADR-0002, opção B, opt-in) só torna a superfície do cartão translúcida: não desfoca o fundo, e
//! as cores de texto são ajustadas para continuar legíveis sobre qualquer papel de parede.

pub type Rgb = [u8; 3];

/// Opacidade do vidro: baixa o bastante para o fundo transparecer, alta o bastante para o texto
/// continuar legível sobre fundo totalmente branco ou totalmente preto (testado).
const GLASS_ALPHA_LIGHT: f32 = 0.86;
const GLASS_ALPHA_DARK: f32 = 0.93;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
    HighContrast,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub mode: Mode,
    /// Gradiente vertical sutil da superfície (topo → base). No alto contraste, iguais.
    pub surface_top: Rgb,
    pub surface_bottom: Rgb,
    pub text: Rgb,
    pub text_secondary: Rgb,
    /// Contorno fino do cartão e realce interno do topo (alpha de 0..1 sobre a superfície).
    pub border: Rgb,
    pub border_alpha: f32,
    pub border_width_dip: f32,
    pub highlight_alpha: f32,
    /// Opacidade da superfície do cartão (1 = opaca; < 1 = vidro simulado).
    pub surface_alpha: f32,
    /// Esmaecido (um aplicativo está em foco): cores de acento e semânticas viram cinza, como os
    /// widgets de desktop do macOS quando inativos.
    pub mute: f32,
    /// Quanto do acento tinge a faixa de movimentação.
    pub header_tint_alpha: f32,
    /// Acentos por nota e a cor do texto sobre um acento cheio (botões, anel de foco).
    pub accents: [Rgb; 4],
    pub on_accent: Rgb,
}

pub fn lin(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn luminance(c: Rgb) -> f64 {
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// Razão de contraste WCAG 2.x entre duas cores opacas (1..=21).
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// `over` com opacidade `alpha` sobre `under` (composição simples em sRGB, como o D2D faz).
pub fn blend(over: Rgb, under: Rgb, alpha: f32) -> Rgb {
    let mix = |o: u8, u: u8| (o as f32 * alpha + u as f32 * (1.0 - alpha)).round().clamp(0.0, 255.0) as u8;
    [mix(over[0], under[0]), mix(over[1], under[1]), mix(over[2], under[2])]
}

/// `c` a `amount` do caminho até o seu cinza.
pub fn mix_gray(c: Rgb, amount: f32) -> Rgb {
    blend(gray_of(c), c, amount)
}

/// Cinza de mesma luminância percebida.
pub fn gray_of(c: Rgb) -> Rgb {
    let l = (0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32).round().clamp(0.0, 255.0) as u8;
    [l, l, l]
}

impl Theme {
    /// Versão esmaecida (monocromática) do tema; o alto contraste não esmaece.
    pub fn muted(self) -> Self {
        self.muted_by(1.0)
    }

    /// Esmaecimento parcial (0 = cores normais, 1 = cinza): é o que a transição anima.
    pub fn muted_by(mut self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        if self.mode == Mode::HighContrast || self.mute > 0.0 || amount <= 0.0 {
            return self;
        }
        self.mute = amount;
        self.accents = self.accents.map(|c| mix_gray(c, amount));
        self
    }

    /// Cor de um papel semântico da view. `accent` é a identidade da instância.
    pub fn role_rgb(&self, role: pod_model::view::Role, accent: Rgb) -> Rgb {
        let c = self.role_rgb_color(role, accent);
        if self.mute > 0.0 && !matches!(role, pod_model::view::Role::Primary | pod_model::view::Role::Secondary | pod_model::view::Role::Tertiary) {
            mix_gray(c, self.mute)
        } else {
            c
        }
    }

    fn role_rgb_color(&self, role: pod_model::view::Role, accent: Rgb) -> Rgb {
        use pod_model::view::Role::*;
        let dark = self.mode == Mode::Dark;
        let glass = self.surface_alpha < 1.0;
        match (role, self.mode) {
            (Primary, _) => self.text,
            (Secondary, _) => self.text_secondary,
            (Tertiary, _) => blend(self.text_secondary, self.surface_bottom, 0.85),
            (Accent, _) => accent,
            (Up | Down, Mode::HighContrast) => self.text,
            (Up, _) => [0x3D, 0xE0, 0x6B],
            (Down, _) => [0xFF, 0x55, 0x55],
            (OnFill, Mode::HighContrast) => self.text,
            (OnFill, _) => [0xFF, 0xFF, 0xFF],
            (OnFillDim, Mode::HighContrast) => self.text,
            (OnFillDim, _) => [0xD6, 0xD6, 0xE0],
            // Alto contraste: nada depende de matiz, tudo usa a cor de texto do sistema.
            (_, Mode::HighContrast) => self.text,
            // Papéis de símbolo: no vidro o fundo é desconhecido, então claro escurece e escuro clareia
            // (medido em papéis de parede reais: docs/validation/glass-legibility.md).
            (Blue, _) => if glass { if dark { [0x4D, 0xA3, 0xFF] } else { [0x00, 0x5A, 0xC8] } } else if dark { [0x0A, 0x84, 0xFF] } else { [0x00, 0x6A, 0xE0] },
            (Indigo, _) => if glass { if dark { [0x93, 0x91, 0xFF] } else { [0x45, 0x43, 0xB8] } } else if dark { [0x7D, 0x7A, 0xFF] } else { [0x58, 0x56, 0xD6] },
            (Orange, _) => if glass { if dark { [0xFF, 0xA8, 0x1F] } else { [0x9A, 0x55, 0x00] } } else if dark { [0xFF, 0x9F, 0x0A] } else { [0xE0, 0x83, 0x00] },
            (Pink, _) => if glass { if dark { [0xFF, 0x62, 0x80] } else { [0xB0, 0x14, 0x3A] } } else if dark { [0xFF, 0x37, 0x5F] } else { [0xE0, 0x1E, 0x4B] },
            (Red, _) => if glass { if dark { [0xFF, 0x92, 0x88] } else { [0xA8, 0x1F, 0x16] } } else if dark { [0xFF, 0x45, 0x3A] } else { [0xE0, 0x30, 0x25] },
            (Clay, _) => if glass && !dark { [0x9A, 0x45, 0x2B] } else if dark { [0xE8, 0x8B, 0x6A] } else { [0xC4, 0x5F, 0x3E] },
            (Cyan, _) => if glass && !dark { [0x0B, 0x6E, 0x99] } else if dark { [0x64, 0xD2, 0xFF] } else { [0x1C, 0x8F, 0xC9] },
            (Positive, _) if dark && glass => [0x7E, 0xE0, 0xA6],
            (Warning, _) if dark && glass => [0xFF, 0xCB, 0x6B],
            (Danger, _) if dark && glass => [0xFF, 0x9A, 0x9A],
            (Positive, _) if dark => [0x5B, 0xD1, 0x8B],
            // Vidro claro: o fundo atrás é desconhecido, então os semânticos escurecem.
            (Positive, _) if glass => [0x14, 0x5C, 0x32],
            (Warning, _) if glass && !dark => [0x6E, 0x40, 0x00],
            (Danger, _) if glass && !dark => [0x92, 0x1F, 0x1F],
            (Positive, _) => [0x1E, 0x7A, 0x45],
            (Warning, _) if dark => [0xF0, 0xB4, 0x4C],
            (Warning, _) => [0x8F, 0x55, 0x00],
            (Danger, _) if dark => [0xF2, 0x7A, 0x7A],
            (Danger, _) => [0xB0, 0x2A, 0x2A],
        }
    }

    /// Material Papel para o chrome de notas, sem alterar o tema dos cartões/galeria.
    /// Conserva a escolha explícita de vidro e todos os tokens de alto contraste.
    pub fn editorial(mut self) -> Self {
        match self.mode {
            Mode::HighContrast => return self,
            Mode::Light => {
                self.surface_top = [0xF6, 0xF4, 0xEE];
                self.text = [0x25, 0x2B, 0x28];
                self.text_secondary = if self.surface_alpha < 1.0 { [0x3F, 0x43, 0x4F] } else { [0x59, 0x63, 0x5C] };
            }
            Mode::Dark => {
                self.surface_top = [0x25, 0x28, 0x27];
                self.text = [0xF1, 0xF1, 0xE9];
                self.text_secondary = [0xBC, 0xC4, 0xBD];
            }
        }
        self.surface_bottom = self.surface_top;
        self.header_tint_alpha = 0.0;
        self.highlight_alpha = 0.0;
        self
    }

    pub fn light() -> Self {
        Self {
            mode: Mode::Light,
            surface_top: [0xFF, 0xFF, 0xFF],
            surface_bottom: [0xF3, 0xF4, 0xF7],
            text: [0x1C, 0x1C, 0x1E],
            text_secondary: [0x5F, 0x62, 0x6B],
            border: [0x00, 0x00, 0x00],
            border_alpha: 0.09,
            border_width_dip: 1.0,
            highlight_alpha: 0.9,
            surface_alpha: 1.0,
            mute: 0.0,
            header_tint_alpha: 0.10,
            accents: [[0x2F, 0x5B, 0xB8], [0xB4, 0x42, 0x32], [0x23, 0x70, 0x4F], [0x6A, 0x45, 0xB5]],
            on_accent: [0xFF, 0xFF, 0xFF],
        }
    }

    pub fn dark() -> Self {
        Self {
            mode: Mode::Dark,
            surface_top: [0x2E, 0x2F, 0x34],
            surface_bottom: [0x1F, 0x20, 0x24],
            text: [0xF5, 0xF5, 0xF7],
            text_secondary: [0xA4, 0xA7, 0xB0],
            border: [0xFF, 0xFF, 0xFF],
            border_alpha: 0.10,
            border_width_dip: 1.0,
            highlight_alpha: 0.10,
            surface_alpha: 1.0,
            mute: 0.0,
            header_tint_alpha: 0.12,
            accents: [[0x86, 0xA9, 0xF7], [0xF3, 0x93, 0x82], [0x6C, 0xCB, 0xA0], [0xBC, 0xA3, 0xF4]],
            on_accent: [0x12, 0x14, 0x19],
        }
    }

    /// Alto contraste: cores do sistema, sem gradiente, tinta nem realce; contorno de 2 DIP.
    /// Nada depende de transparência nem somente de cor.
    pub fn high_contrast(window: Rgb, window_text: Rgb, gray_text: Rgb, highlight: Rgb, highlight_text: Rgb) -> Self {
        Self {
            mode: Mode::HighContrast,
            surface_top: window,
            surface_bottom: window,
            text: window_text,
            text_secondary: gray_text,
            border: window_text,
            border_alpha: 1.0,
            border_width_dip: 2.0,
            highlight_alpha: 0.0,
            surface_alpha: 1.0,
            mute: 0.0,
            header_tint_alpha: 0.0,
            accents: [highlight; 4],
            on_accent: highlight_text,
        }
    }

    /// Vidro simulado: superfície translúcida (sem desfocar o fundo). Não faz nada no alto contraste
    /// (que não depende de transparência) nem em um tema que já é vidro. O texto secundário fica mais
    /// forte porque o fundo atrás do cartão é desconhecido: ver `glass_is_readable_on_any_backdrop`.
    pub fn glass(mut self) -> Self {
        if self.mode == Mode::HighContrast || self.surface_alpha < 1.0 {
            return self;
        }
        match self.mode {
            Mode::Light => {
                self.surface_alpha = GLASS_ALPHA_LIGHT;
                self.text = [0x14, 0x16, 0x1B];
                self.text_secondary = [0x3F, 0x43, 0x4F];
                // Acentos usados como texto/anel precisam de 4,5:1 mesmo sobre fundo preto.
                self.accents = self.accents.map(|c| c.map(|v| (v as f32 * 0.78) as u8));
            }
            _ => {
                self.surface_alpha = GLASS_ALPHA_DARK;
                self.text = [0xF7, 0xF8, 0xFA];
                self.text_secondary = [0xC9, 0xCE, 0xD9];
            }
        }
        self.border_alpha = 0.22;
        self
    }

    /// Cor da superfície vista sobre um fundo `backdrop` (o que o usuário realmente enxerga).
    pub fn surface_over(&self, surface: Rgb, backdrop: Rgb) -> Rgb {
        blend(surface, backdrop, self.surface_alpha)
    }

    pub fn accent(&self, index: usize) -> Rgb {
        self.accents[index % self.accents.len()]
    }

    /// Cor da faixa de movimentação: o acento tingindo a superfície (cheio no alto contraste).
    pub fn header(&self, accent: Rgb) -> Rgb {
        if self.mode == Mode::HighContrast {
            return self.surface_top;
        }
        blend(accent, self.surface_top, self.header_tint_alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editorial_paper_remains_readable_and_respects_high_contrast() {
        for base in [Theme::light(), Theme::dark(), Theme::light().glass(), Theme::dark().glass()] {
            let note = base.editorial();
            for backdrop in [[0, 0, 0], [255, 255, 255]] {
                let surface = note.surface_over(note.surface_top, backdrop);
                assert!(contrast(note.text, surface) >= 7.0);
                assert!(contrast(note.text_secondary, surface) >= 4.5);
                for accent in note.accents {
                    assert!(contrast(accent, surface) >= 3.0, "anel de foco precisa se distinguir");
                }
            }
        }
        let hc = Theme::high_contrast([0, 0, 0], [255, 255, 255], [0, 255, 0], [0, 255, 255], [0, 0, 0]);
        assert_eq!(hc.clone().editorial(), hc);
    }

    #[test]
    fn every_role_is_legible_on_both_ends_of_the_surface() {
        use pod_model::view::Role::*;
        let hc = Theme::high_contrast([0, 0, 0], [255, 255, 255], [0x3F, 0xF2, 0x3F], [0x1A, 0xEB, 0xFF], [0, 0, 0]);
        for th in [Theme::light(), Theme::dark(), hc] {
            for accent in th.accents {
                for role in [Primary, Secondary, Tertiary, Accent, Positive, Warning, Danger] {
                    let c = th.role_rgb(role, accent);
                    for bg in [th.surface_top, th.surface_bottom] {
                        // Tertiary é apoio visual (metade do limiar de texto grande); o resto é texto.
                        let need = if role == Tertiary { 3.0 } else { 4.5 };
                        let got = contrast(c, bg);
                        assert!(got >= need, "{:?} {role:?} {c:?} sobre {bg:?}: {got:.2}", th.mode);
                    }
                }
            }
        }
    }

    #[test]
    fn glass_is_readable_on_any_backdrop() {
        use pod_model::view::Role::*;
        for base in [Theme::light(), Theme::dark()] {
            let th = base.clone().glass();
            assert!(th.surface_alpha < 1.0 && th.surface_alpha >= 0.7, "translúcido, mas não demais");
            // Os extremos de luminância são o pior caso para qualquer papel de parede.
            for backdrop in [[0, 0, 0], [255, 255, 255], [128, 128, 128]] {
                for surface in [th.surface_top, th.surface_bottom] {
                    let bg = th.surface_over(surface, backdrop);
                    assert!(contrast(th.text, bg) >= 7.0, "{:?} texto sobre {backdrop:?}: {:.2}", th.mode, contrast(th.text, bg));
                    for accent in th.accents {
                        for role in [Secondary, Accent, Positive, Warning, Danger] {
                            let c = th.role_rgb(role, accent);
                            let got = contrast(c, bg);
                            assert!(got >= 4.5, "{:?} {role:?} {c:?} sobre {backdrop:?}: {got:.2}", th.mode);
                        }
                    }
                    // Ícones dos cabeçalhos (3:1) e o vermelho de texto do Calendário (4,5:1).
                    for (role, need) in [(Blue, 3.0), (Indigo, 3.0), (Orange, 3.0), (Pink, 3.0), (Clay, 3.0), (Cyan, 3.0), (Red, 4.5)] {
                        let c = th.role_rgb(role, th.accents[0]);
                        let got = contrast(c, bg);
                        assert!(got >= need, "{:?} {role:?} {c:?} sobre {backdrop:?}: {got:.2} < {need}", th.mode);
                    }
                }
            }
        }
    }

    /// Mede a legibilidade do vidro sobre papéis de parede reais (ADR-0005, critério V1). Não roda na
    /// suíte normal: precisa de amostras brutas geradas por `tools/dump-wallpapers.ps1`
    /// (`POD_WALLPAPER_DIR`) e grava o relatório em `POD_REPORT_OUT` (ou imprime).
    /// Para cada cartão (grade de posições) compõe a superfície translúcida sobre os pixels e mede o contraste
    /// de cada papel de texto; o critério é o 5º percentil do cartão (um pixel isolado não define leitura).
    #[test]
    #[ignore = "mede com papéis de parede reais; ver docs/validation/glass-legibility.md"]
    fn measure_glass_legibility_on_real_wallpapers() {
        use pod_model::view::Role::*;
        use std::fmt::Write as _;
        let dir = std::env::var("POD_WALLPAPER_DIR").expect("defina POD_WALLPAPER_DIR");
        let mut walls: Vec<(String, u32, u32, Vec<u8>)> = Vec::new();
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
        files.sort();
        for f in files {
            let name = f.file_stem().unwrap().to_string_lossy().to_string();
            if f.extension().is_none_or(|e| e != "rgb") || name.contains("1920x1200") {
                continue; // duplicatas de Windows_img0/img19
            }
            // `POD_WALLPAPER_ONLY`: mede só os papéis cujo nome contém o texto (ex.: o do usuário).
            if std::env::var("POD_WALLPAPER_ONLY").is_ok_and(|only| !name.contains(&only)) {
                continue;
            }
            let raw = std::fs::read(&f).unwrap();
            let (w, h) = (u32::from_le_bytes(raw[0..4].try_into().unwrap()), u32::from_le_bytes(raw[4..8].try_into().unwrap()));
            walls.push((name, w, h, raw[8..].to_vec()));
        }
        assert!(!walls.is_empty() && (walls.len() >= 6 || std::env::var("POD_WALLPAPER_ONLY").is_ok()), "precisa de ao menos 6 papéis de parede");
        let lin_table: Vec<f64> = (0..=255u8).map(lin).collect();
        let lum = |c: Rgb| 0.2126 * lin_table[c[0] as usize] + 0.7152 * lin_table[c[1] as usize] + 0.0722 * lin_table[c[2] as usize];
        let ratio = |a: f64, b: f64| (a.max(b) + 0.05) / (a.min(b) + 0.05);
        const BINS: usize = 512;
        // Telas de 1920x1080 DIP a 96 dpi; as amostras têm metade da resolução.
        let sizes: [(&str, f32); 3] = [("1x1", 88.0), ("2x2", 188.0), ("4x4", 388.0)];
        let mut report = String::new();
        let _ = writeln!(report, "# Legibilidade do vidro simulado sobre papéis de parede reais\n");
        let _ = writeln!(report, "Papéis de parede analisados ({}): {}\n", walls.len(), walls.iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(", "));
        for (theme_name, base) in [("claro", Theme::light()), ("escuro", Theme::dark())] {
            let th = base.glass();
            let alpha = th.surface_alpha;
            let roles: Vec<(String, Rgb, f64)> = {
                let mut v = vec![("Primário".to_string(), th.text, 7.0), ("Secundário".to_string(), th.role_rgb(Secondary, th.accents[0]), 4.5)];
                for (i, a) in th.accents.iter().enumerate() {
                    v.push((format!("Acento {i}"), th.role_rgb(Accent, *a), 4.5));
                }
                v.push(("Positivo".to_string(), th.role_rgb(Positive, th.accents[0]), 4.5));
                v.push(("Aviso".to_string(), th.role_rgb(Warning, th.accents[0]), 4.5));
                v.push(("Perigo".to_string(), th.role_rgb(Danger, th.accents[0]), 4.5));
                // Papéis de símbolo dos cabeçalhos (ícones: 3:1) e o vermelho do dia da semana (texto: 4,5:1).
                for (name, role, need) in [("Azul", Blue, 3.0), ("Índigo", Indigo, 3.0), ("Laranja", Orange, 3.0), ("Rosa", Pink, 3.0), ("Argila", Clay, 3.0), ("Ciano", Cyan, 3.0), ("Vermelho (texto)", Red, 4.5)] {
                    v.push((name.to_string(), th.role_rgb(role, th.accents[0]), need));
                }
                v
            };
            let _ = writeln!(report, "## Tema {theme_name} (alfa {alpha})\n");
            for (size_name, dip) in sizes {
                // (pior p5, onde, pior pixel, onde, cartões abaixo do limite, total de cartões)
                let mut agg: Vec<(f64, String, f64, String, usize, usize)> = vec![(99.0, String::new(), 99.0, String::new(), 0, 0); roles.len()];
                for (wname, w, h, px) in &walls {
                    let (w, h) = (*w as usize, *h as usize);
                    // Luminância do cartão composto sobre cada pixel, nas duas pontas do gradiente da superfície.
                    let maps: Vec<Vec<u16>> = [th.surface_top, th.surface_bottom]
                        .iter()
                        .map(|surface| {
                            (0..w * h)
                                .map(|i| {
                                    let back = [px[i * 3], px[i * 3 + 1], px[i * 3 + 2]];
                                    let l = lum(blend(*surface, back, alpha));
                                    ((l * (BINS as f64 - 1.0)).round() as u16).min(BINS as u16 - 1)
                                })
                                .collect()
                        })
                        .collect();
                    let side = (dip / 2.0) as usize;
                    let step = 24usize;
                    let mut y0 = 0;
                    while y0 + side <= h {
                        let mut x0 = 0;
                        while x0 + side <= w {
                            // Histograma das duas pontas juntas (conservador: o pior contraste de qualquer uma).
                            let mut hist = [0u32; BINS];
                            for map in &maps {
                                for y in (y0..y0 + side).step_by(2) {
                                    for x in (x0..x0 + side).step_by(2) {
                                        hist[map[y * w + x] as usize] += 1;
                                    }
                                }
                            }
                            let total: u32 = hist.iter().sum();
                            for (ri, (_, fg, need)) in roles.iter().enumerate() {
                                let lf = lum(*fg);
                                let mut by_contrast: Vec<(f64, u32)> = (0..BINS)
                                    .filter(|b| hist[*b] > 0)
                                    .map(|b| (ratio(lf, b as f64 / (BINS as f64 - 1.0)), hist[b]))
                                    .collect();
                                by_contrast.sort_by(|a, b| a.0.total_cmp(&b.0));
                                let worst = by_contrast[0].0;
                                let mut acc = 0u32;
                                let mut p5 = by_contrast[0].0;
                                for (c, n) in &by_contrast {
                                    acc += n;
                                    p5 = *c;
                                    if acc as f64 >= total as f64 * 0.05 {
                                        break;
                                    }
                                }
                                let a = &mut agg[ri];
                                a.5 += 1;
                                if p5 < *need {
                                    a.4 += 1;
                                }
                                if p5 < a.0 {
                                    a.0 = p5;
                                    a.1 = format!("{wname} ({x0},{y0})");
                                }
                                if worst < a.2 {
                                    a.2 = worst;
                                    a.3 = format!("{wname} ({x0},{y0})");
                                }
                            }
                            x0 += step;
                        }
                        y0 += step;
                    }
                }
                let _ = writeln!(report, "### Cartão {size_name} ({dip} DIP)\n");
                let _ = writeln!(report, "| Papel | Mínimo exigido | Pior 5º percentil (onde) | Pior pixel | Cartões abaixo do mínimo |");
                let _ = writeln!(report, "| --- | --- | --- | --- | --- |");
                for ((name, _, need), a) in roles.iter().zip(&agg) {
                    let _ = writeln!(
                        report,
                        "| {name} | {need:.1} | {:.2} ({}) | {:.2} | {} de {} ({:.1}%) |",
                        a.0, a.1, a.2, a.4, a.5, 100.0 * a.4 as f64 / a.5.max(1) as f64
                    );
                }
                let _ = writeln!(report);
            }
        }
        match std::env::var("POD_REPORT_OUT") {
            Ok(path) => std::fs::write(path, &report).unwrap(),
            Err(_) => println!("{report}"),
        }
    }

    #[test]
    fn muted_theme_is_gray_and_keeps_text_readable() {
        use pod_model::view::Role::*;
        for base in [Theme::light(), Theme::dark(), Theme::light().glass(), Theme::dark().glass()] {
            let m = base.clone().muted();
            assert!(m.mute == 1.0 && base.mute == 0.0);
            for a in m.accents {
                assert!(a[0] == a[1] && a[1] == a[2], "acento cinza: {a:?}");
            }
            for role in [Accent, Positive, Warning, Danger] {
                let c = m.role_rgb(role, m.accent(0));
                assert!(c[0] == c[1] && c[1] == c[2], "{role:?} {c:?}");
            }
            assert_eq!(m.role_rgb(Primary, m.accent(0)), base.role_rgb(Primary, base.accent(0)), "texto n\u{e3}o muda");
            assert_eq!(m.clone().muted(), m, "idempotente");
        }
        let hc = Theme::high_contrast([0, 0, 0], [255, 255, 255], [63, 242, 63], [55, 255, 255], [0, 0, 0]);
        assert_eq!(hc.clone().muted(), hc, "alto contraste n\u{e3}o esmaece");
    }

    #[test]
    fn partial_muting_is_between_color_and_gray() {
        let base = Theme::dark();
        let half = base.clone().muted_by(0.5);
        let full = base.clone().muted();
        for i in 0..4 {
            let (c, h, g) = (base.accents[i], half.accents[i], full.accents[i]);
            for k in 0..3 {
                let (lo, hi) = (c[k].min(g[k]), c[k].max(g[k]));
                assert!(h[k] >= lo && h[k] <= hi, "canal {k} do acento {i}: {c:?} {h:?} {g:?}");
            }
        }
        assert_eq!(base.clone().muted_by(0.0), base, "zero nada muda");
        assert_eq!(base.clone().muted_by(7.0), full, "limitado a 1");
    }

    #[test]
    fn glass_never_applies_to_high_contrast_or_twice() {
        let hc = Theme::high_contrast([0, 0, 0], [255, 255, 255], [63, 242, 63], [55, 255, 255], [0, 0, 0]);
        assert_eq!(hc.clone().glass(), hc, "alto contraste continua opaco");
        let g = Theme::dark().glass();
        assert_eq!(g.clone().glass(), g, "idempotente");
        assert_eq!(Theme::light().surface_alpha, 1.0, "opaco por padrão");
    }

    #[test]
    fn contrast_math_matches_wcag_reference_values() {
        assert!((contrast([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 1e-9);
        assert!((contrast([255, 255, 255], [255, 255, 255]) - 1.0).abs() < 1e-9);
        // Referência conhecida: #767676 sobre branco é ~4.54:1.
        assert!((contrast([0x76, 0x76, 0x76], [255, 255, 255]) - 4.54).abs() < 0.01);
        assert_eq!(contrast([10, 20, 30], [200, 210, 220]), contrast([200, 210, 220], [10, 20, 30]));
    }

    fn check(theme: &Theme) {
        let name = format!("{:?}", theme.mode);
        for surface in [theme.surface_top, theme.surface_bottom] {
            assert!(contrast(theme.text, surface) >= 7.0, "{name}: texto/superfície {}", contrast(theme.text, surface));
            assert!(
                contrast(theme.text_secondary, surface) >= 4.5,
                "{name}: texto secundário/superfície {}",
                contrast(theme.text_secondary, surface)
            );
        }
        for (i, accent) in theme.accents.iter().enumerate() {
            let label = contrast(theme.on_accent, *accent);
            assert!(label >= 4.5, "{name}: rótulo sobre acento {i}: {label}");
            // O anel de foco e o contorno de controles precisam de 3:1 contra a superfície.
            let ring = contrast(*accent, theme.surface_top).max(contrast(*accent, theme.surface_bottom));
            let ring_min = contrast(*accent, theme.surface_top).min(contrast(*accent, theme.surface_bottom));
            assert!(ring_min >= 3.0, "{name}: anel de foco do acento {i} sobre a superfície: {ring_min} (max {ring})");
            // O texto continua legível sobre a faixa tingida.
            let header = theme.header(*accent);
            assert!(contrast(theme.text, header) >= 7.0, "{name}: texto sobre a faixa {i}: {}", contrast(theme.text, header));
        }
    }

    #[test]
    fn light_theme_is_readable() {
        check(&Theme::light());
    }

    #[test]
    fn dark_theme_is_readable() {
        check(&Theme::dark());
    }

    #[test]
    fn high_contrast_uses_system_colors_without_alpha_tricks() {
        // Esquema "Aquatic"/escuro de exemplo.
        let t = Theme::high_contrast([0, 0, 0], [255, 255, 255], [63, 242, 63], [55, 255, 255], [0, 0, 0]);
        assert_eq!(t.surface_top, t.surface_bottom, "sem gradiente");
        assert_eq!(t.header_tint_alpha, 0.0);
        assert_eq!(t.highlight_alpha, 0.0);
        assert!(t.border_width_dip >= 2.0 && t.border_alpha == 1.0, "contorno cheio e mais grosso");
        assert_eq!(t.header(t.accent(0)), t.surface_top, "faixa sem tinta");
        check(&t);
    }

    #[test]
    fn blend_is_a_plain_alpha_composite() {
        assert_eq!(blend([255, 0, 0], [0, 0, 255], 0.0), [0, 0, 255]);
        assert_eq!(blend([255, 0, 0], [0, 0, 255], 1.0), [255, 0, 0]);
        assert_eq!(blend([200, 100, 0], [0, 100, 200], 0.5), [100, 100, 100]);
    }

    #[test]
    fn accent_index_wraps() {
        let t = Theme::light();
        assert_eq!(t.accent(0), t.accent(4));
        assert_ne!(t.accent(0), t.accent(1));
    }
}
