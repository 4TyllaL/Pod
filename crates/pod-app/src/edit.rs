//! Pod Studio (modo de edição, ADR-0003 P4): uma janela com barra lateral (busca, seções e
//! "Concluir") e, à direita, o catálogo em cartões uniformes com a prévia de cada widget (dados de
//! demonstração), agrupados por categoria. Clicar num cartão adiciona o widget; arrastar leva ao
//! desktop. A seção "Ajustes" liga e desliga o vidro e os widgets de internet. Excluir um cartão fica
//! no menu do botão direito dele.
//!
//! O desenho (`paint_studio`) não depende de janela: a galeria offscreen o usa para conferir o visual.

use std::rc::Rc;

use windows::core::{w, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::{Matrix3x2, Vector2};

use crate::gfx::{is_device_lost, Gfx, Surface};
use crate::render::{self, col, DwMeasure, PaintStyle};
use crate::shell::Shell;
use crate::ui::theme::{self, Rgb, Theme};
use pod_model::ids::WidgetTypeId;
use pod_model::layout::{layout, Laid, Rect};
use pod_model::view::{Align, TextStyle, View};

pub const WIN_W: f32 = 960.0;
pub const WIN_H: f32 = 640.0;
const SIDEBAR_W: f32 = 224.0;
const PAD: f32 = 16.0;
const CONTENT_X: f32 = SIDEBAR_W + 20.0;
const CONTENT_W: f32 = WIN_W - CONTENT_X - 20.0;
const CONTENT_Y: f32 = 76.0;
const COLS: usize = 3;
const GAP: f32 = 12.0;
const CARD_W: f32 = (CONTENT_W - GAP * (COLS as f32 - 1.0)) / COLS as f32;
const CARD_H: f32 = 196.0;
const PREVIEW_H: f32 = 146.0;
const SECTION_H: f32 = 34.0;
const SEARCH: Rect = Rect { x: PAD, y: 80.0, w: SIDEBAR_W - 2.0 * PAD, h: 36.0 };
const NAV_Y: f32 = 132.0;
const NAV_H: f32 = 38.0;
const DONE: Rect = Rect { x: PAD, y: WIN_H - 60.0, w: SIDEBAR_W - 2.0 * PAD, h: 42.0 };
const MAX_QUERY: usize = 40;
const TOGGLE_H: f32 = 72.0;
const ICON_FONT: windows::core::PCWSTR = w!("Segoe MDL2 Assets");
/// Ícones (Segoe MDL2) das seções: Todos, Tempo, Produtividade, Sistema, Arquivos, Internet, Ajustes.
const NAV_ICONS: [char; 7] = ['\u{E80A}', '\u{E823}', '\u{E8FD}', '\u{E7F4}', '\u{E8B7}', '\u{E774}', '\u{E713}'];
const TOGGLES: [(&str, &str); 2] = [
    ("Vidro", "Cart\u{f5}es transl\u{fa}cidos sobre o papel de parede."),
    ("Widgets de internet", "Clima, Not\u{ed}cias e GitHub podem acessar a internet."),
];

/// Índice da seção "Ajustes" (depois das categorias do catálogo).
pub fn settings_section() -> usize {
    crate::widgets::CATEGORIES.len()
}

fn nav_names() -> Vec<&'static str> {
    let mut v = crate::widgets::CATEGORIES.to_vec();
    v.push("Ajustes");
    v
}

fn nav_rect(i: usize) -> Rect {
    Rect::new(PAD - 4.0, NAV_Y + i as f32 * (NAV_H + 2.0), SIDEBAR_W - 2.0 * PAD + 8.0, NAV_H)
}

fn toggle_rect(i: usize) -> Rect {
    Rect::new(CONTENT_X, CONTENT_Y + i as f32 * (TOGGLE_H + GAP), CONTENT_W, TOGGLE_H)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// Próximo item na direção `dir` a partir de `from` (setas do teclado): esquerda/direita seguem a
/// ordem de leitura; cima/baixo vão para a linha vizinha, ao item de centro mais próximo na horizontal.
pub fn neighbor(rects: &[Rect], from: usize, dir: Dir) -> usize {
    if rects.is_empty() {
        return 0;
    }
    let from = from.min(rects.len() - 1);
    match dir {
        Dir::Left => from.saturating_sub(1),
        Dir::Right => (from + 1).min(rects.len() - 1),
        Dir::Up | Dir::Down => {
            let cur = rects[from];
            let cx = cur.x + cur.w / 2.0;
            let rows = |r: &Rect| (r.y * 4.0).round() as i64;
            let candidates = rects.iter().enumerate().filter(|(_, r)| match dir {
                Dir::Down => rows(r) > rows(&cur),
                _ => rows(r) < rows(&cur),
            });
            let best_row = match dir {
                Dir::Down => candidates.clone().map(|(_, r)| rows(r)).min(),
                _ => candidates.clone().map(|(_, r)| rows(r)).max(),
            };
            let Some(row) = best_row else { return from };
            candidates
                .filter(|(_, r)| rows(r) == row)
                .min_by(|(_, a), (_, b)| {
                    let (da, db) = ((a.x + a.w / 2.0 - cx).abs(), (b.x + b.w / 2.0 - cx).abs());
                    da.total_cmp(&db)
                })
                .map_or(from, |(i, _)| i)
        }
    }
}

/// Quais itens `(categoria, texto de busca)` aparecem com a categoria (0 = todas) e a busca dadas.
pub fn filter_flags<'a>(items: impl Iterator<Item = (usize, &'a str)>, category: usize, query: &str) -> Vec<bool> {
    items.map(|(cat, hay)| (category == 0 || cat == category) && pod_model::search::matches(query, hay)).collect()
}

/// Grade de cartões: `groups` = (rótulo opcional de seção, quantidade). Devolve os retângulos dos
/// cartões (em ordem), os cabeçalhos (rótulo, y) e a altura total.
pub fn plan_grid(groups: &[(Option<&str>, usize)]) -> (Vec<Rect>, Vec<(String, f32)>, f32) {
    let (mut rects, mut headers) = (Vec::new(), Vec::new());
    let mut y = 0.0;
    for (label, n) in groups {
        if *n == 0 {
            continue;
        }
        if let Some(l) = label {
            headers.push((l.to_uppercase(), y));
            y += SECTION_H;
        }
        for i in 0..*n {
            let (c, r) = (i % COLS, i / COLS);
            rects.push(Rect::new(c as f32 * (CARD_W + GAP), y + r as f32 * (CARD_H + GAP), CARD_W, CARD_H));
        }
        y += n.div_ceil(COLS) as f32 * (CARD_H + GAP) + 6.0;
    }
    (rects, headers, y)
}

struct Item {
    type_id: WidgetTypeId,
    name: &'static str,
    shape_label: String,
    cat: usize,
    haystack: String,
    visible: bool,
    accent: usize,
    size: (f32, f32),
    /// Cartão em coordenadas do conteúdo (antes da rolagem).
    rect: Rect,
    view: View,
    laid: Vec<Laid>,
    added: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Card(usize),
    Nav(usize),
    Done,
    Toggle(usize),
}

/// Estado da tela do Studio (sem janela): o que desenhar e onde cada coisa está.
pub struct Studio {
    items: Vec<Item>,
    headers: Vec<(String, f32)>,
    content_h: f32,
    scroll: f32,
    pub hover: Option<Hit>,
    /// Item selecionado pelo teclado (anel de foco).
    sel: Option<usize>,
    pub query: String,
    pub section: usize,
    pub glass: bool,
    pub internet: bool,
}

impl Studio {
    pub fn new(gfx: &Gfx) -> Self {
        let mut items = Vec::new();
        for (type_id, name) in crate::widgets::catalog() {
            let Some((shape, view)) = crate::gallery::preview(&type_id) else { continue };
            let s = shape.size_dip();
            let laid = layout(&view, Rect::new(0.0, 0.0, s.w, s.h), &DwMeasure(gfx));
            let accent = crate::widgets::accent_for(&type_id).unwrap_or(0);
            let cat = crate::widgets::category_of(&type_id);
            let haystack = pod_model::search::fold(&format!("{name} {}", crate::widgets::CATEGORIES[cat]));
            items.push(Item {
                type_id,
                name,
                shape_label: shape.to_string(),
                cat,
                haystack,
                visible: true,
                accent,
                size: (s.w, s.h),
                rect: Rect::new(0.0, 0.0, CARD_W, CARD_H),
                view,
                laid,
                added: 0,
            });
        }
        let mut s = Studio {
            items,
            headers: Vec::new(),
            content_h: 0.0,
            scroll: 0.0,
            hover: None,
            sel: None,
            query: String::new(),
            section: 0,
            glass: false,
            internet: false,
        };
        s.relayout();
        s
    }

    fn view_h() -> f32 {
        WIN_H - CONTENT_Y - 12.0
    }

    fn max_scroll(&self) -> f32 {
        (self.content_h - Self::view_h()).max(0.0)
    }

    pub fn in_settings(&self) -> bool {
        self.section == settings_section()
    }

    fn visible_indices(&self) -> Vec<usize> {
        (0..self.items.len()).filter(|i| self.items[*i].visible).collect()
    }

    /// Reaplica seção e busca: decide quem aparece, agrupa por categoria em "Todos" e reposiciona.
    pub fn relayout(&mut self) {
        if self.in_settings() {
            self.items.iter_mut().for_each(|i| i.visible = false);
            self.headers.clear();
            self.content_h = 0.0;
            self.scroll = 0.0;
            self.sel = None;
            return;
        }
        let flags = filter_flags(self.items.iter().map(|i| (i.cat, i.haystack.as_str())), self.section, &self.query);
        for (it, f) in self.items.iter_mut().zip(flags) {
            it.visible = f;
        }
        let grouped = self.section == 0 && self.query.is_empty();
        // Ordem de exibição: por categoria quando agrupado; senão, a do catálogo.
        let mut order: Vec<usize> = self.visible_indices();
        let mut groups: Vec<(Option<&str>, usize)> = Vec::new();
        if grouped {
            order.sort_by_key(|i| self.items[*i].cat);
            for (c, name) in crate::widgets::CATEGORIES.iter().enumerate().skip(1) {
                groups.push((Some(name), order.iter().filter(|i| self.items[**i].cat == c).count()));
            }
            // Itens sem categoria (não deveria haver) vão para o fim.
            let none = order.iter().filter(|i| self.items[**i].cat == 0).count();
            order.sort_by_key(|i| if self.items[*i].cat == 0 { usize::MAX } else { self.items[*i].cat });
            groups.push((Some("Outros"), none));
        } else {
            groups.push((None, order.len()));
        }
        let (rects, headers, h) = plan_grid(&groups);
        for (i, r) in order.iter().zip(rects) {
            self.items[*i].rect = r;
        }
        self.headers = headers;
        self.content_h = h;
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        if self.sel.is_some_and(|s| !self.items[s].visible) {
            self.sel = None;
        }
    }

    /// O que está sob o ponto (DIP da janela).
    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        let inside = |r: &Rect| x >= r.x && x < r.right() && y >= r.y && y < r.bottom();
        if inside(&DONE) {
            return Some(Hit::Done);
        }
        if let Some(i) = (0..nav_names().len()).find(|i| inside(&nav_rect(*i))) {
            return Some(Hit::Nav(i));
        }
        if self.in_settings() {
            return (0..TOGGLES.len()).find(|i| inside(&toggle_rect(*i))).map(Hit::Toggle);
        }
        if x < CONTENT_X || y < CONTENT_Y || y > WIN_H - 12.0 {
            return None;
        }
        let (cx, cy) = (x - CONTENT_X, y - CONTENT_Y + self.scroll);
        self.items
            .iter()
            .position(|i| i.visible && cx >= i.rect.x && cx < i.rect.right() && cy >= i.rect.y && cy < i.rect.bottom())
            .map(Hit::Card)
    }

    pub fn scroll_by(&mut self, dy: f32) -> bool {
        let next = (self.scroll + dy).clamp(0.0, self.max_scroll());
        let moved = next != self.scroll;
        self.scroll = next;
        moved
    }

    pub fn set_section(&mut self, i: usize) {
        self.section = i;
        self.scroll = 0.0;
        self.sel = None;
        self.relayout();
    }

    /// Digitação na busca: sai de "Ajustes" para mostrar resultados.
    pub fn type_char(&mut self, ch: char) -> bool {
        let changed = match ch {
            '\u{8}' => self.query.pop().is_some(),
            ' ' if self.query.is_empty() || self.query.ends_with(' ') => false,
            c if !c.is_control() && self.query.chars().count() < MAX_QUERY => {
                self.query.push(c);
                true
            }
            _ => false,
        };
        if changed {
            if self.in_settings() {
                self.section = 0;
            }
            self.scroll = 0.0;
            self.relayout();
        }
        changed
    }

    /// Setas/Tab movem a seleção; devolve o item a adicionar quando for Enter.
    fn key(&mut self, vk: usize, shift: bool) -> Option<WidgetTypeId> {
        let mut vis = self.visible_indices();
        vis.sort_by(|a, b| {
            let (ra, rb) = (self.items[*a].rect, self.items[*b].rect);
            ra.y.total_cmp(&rb.y).then(ra.x.total_cmp(&rb.x))
        });
        if vis.is_empty() {
            return None;
        }
        let rects: Vec<Rect> = vis.iter().map(|i| self.items[*i].rect).collect();
        let cur = self.sel.and_then(|s| vis.iter().position(|v| *v == s));
        let next = match vk {
            0x25 => Some(neighbor(&rects, cur.unwrap_or(0), Dir::Left)),
            0x27 => Some(cur.map_or(0, |c| neighbor(&rects, c, Dir::Right))),
            0x26 => Some(neighbor(&rects, cur.unwrap_or(0), Dir::Up)),
            0x28 => Some(neighbor(&rects, cur.unwrap_or(0), Dir::Down)),
            0x09 if shift => Some(cur.map_or(rects.len() - 1, |c| c.saturating_sub(1))),
            0x09 => Some(cur.map_or(0, |c| (c + 1) % rects.len())),
            _ => None,
        };
        if let Some(n) = next {
            self.sel = Some(vis[n]);
            let r = rects[n];
            if r.y < self.scroll {
                self.scroll = (r.y - SECTION_H).max(0.0);
            } else if r.bottom() > self.scroll + Self::view_h() {
                self.scroll = (r.bottom() + GAP - Self::view_h()).clamp(0.0, self.max_scroll());
            }
            return None;
        }
        let i = match cur {
            Some(c) => vis[c],
            None if vis.len() == 1 => vis[0],
            None => return None,
        };
        self.items[i].added += 1;
        Some(self.items[i].type_id.clone())
    }

    fn mark_added(&mut self, i: usize) -> WidgetTypeId {
        self.items[i].added += 1;
        self.items[i].type_id.clone()
    }

    fn title(&self) -> String {
        if self.in_settings() {
            "Ajustes".into()
        } else if !self.query.is_empty() {
            "Resultados".into()
        } else if self.section == 0 {
            "Todos os widgets".into()
        } else {
            crate::widgets::CATEGORIES[self.section].to_string()
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Desenho

pub(crate) fn text(ctx: &ID2D1DeviceContext, gfx: &Gfx, s: &str, style: TextStyle, color: Rgb, alpha: f32, r: Rect, align: Align) -> Result<()> {
    unsafe {
        let layout = render::text_layout(gfx, s, style, r.w, r.h, false, align)?;
        layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        let b = ctx.CreateSolidColorBrush(&col(color, alpha), None)?;
        ctx.DrawTextLayout(Vector2 { X: r.x, Y: r.y }, &layout, &b, D2D1_DRAW_TEXT_OPTIONS_CLIP);
    }
    Ok(())
}

pub(crate) fn icon(ctx: &ID2D1DeviceContext, gfx: &Gfx, glyph: char, size: f32, color: Rgb, r: Rect) -> Result<()> {
    unsafe {
        let f = gfx.dwrite.CreateTextFormat(ICON_FONT, None, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-US"))?;
        f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        let mut buf = [0u16; 2];
        let s = glyph.encode_utf16(&mut buf);
        let b = ctx.CreateSolidColorBrush(&col(color, 1.0), None)?;
        ctx.DrawText(s, &f, &rf(&r), &b, D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL);
    }
    Ok(())
}

fn rf(r: &Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: r.x, top: r.y, right: r.right(), bottom: r.bottom() }
}

pub(crate) fn round(ctx: &ID2D1DeviceContext, r: Rect, radius: f32, color: Rgb, alpha: f32) -> Result<()> {
    unsafe {
        let b = ctx.CreateSolidColorBrush(&col(color, alpha), None)?;
        ctx.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rf(&r), radiusX: radius, radiusY: radius }, &b);
    }
    Ok(())
}

pub(crate) fn outline(ctx: &ID2D1DeviceContext, r: Rect, radius: f32, color: Rgb, alpha: f32, width: f32) -> Result<()> {
    unsafe {
        let b = ctx.CreateSolidColorBrush(&col(color, alpha), None)?;
        let h = width / 2.0;
        let rr = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: r.x + h, top: r.y + h, right: r.right() - h, bottom: r.bottom() - h }, radiusX: radius - h, radiusY: radius - h };
        ctx.DrawRoundedRectangle(&rr, &b, width, None);
    }
    Ok(())
}

/// Desenha o Studio inteiro em `ctx` (DIP; a janela ou um bitmap offscreen).
pub fn paint_studio(ctx: &ID2D1DeviceContext, gfx: &Gfx, s: &Studio, th: &Theme) -> Result<()> {
    let dark = th.mode == theme::Mode::Dark;
    let hc = th.mode == theme::Mode::HighContrast;
    let accent = th.accent(0);
    let panel = PaintStyle { theme: th, accent };
    let ink = th.text;
    let soft = th.text_secondary;
    let tint = |a_light: f32, a_dark: f32| if dark { a_dark } else { a_light };
    unsafe {
        // Fundo liso (o degradê do cartão faz faixas visíveis numa área deste tamanho).
        let _ = &panel;
        round(ctx, Rect::new(0.0, 0.0, WIN_W, WIN_H), render::TILE_RADIUS_DIP, if dark { th.surface_bottom } else { th.surface_top }, 1.0)?;
        outline(ctx, Rect::new(0.0, 0.0, WIN_W, WIN_H), render::TILE_RADIUS_DIP, th.border, th.border_alpha, th.border_width_dip.max(1.0))?;
        // Barra lateral: um tom acima do fundo e uma linha separando.
        if !hc {
            round(ctx, Rect::new(6.0, 6.0, SIDEBAR_W - 6.0, WIN_H - 12.0), 18.0, ink, tint(0.035, 0.05))?;
        }
        let sep = ctx.CreateSolidColorBrush(&col(ink, 0.08), None)?;
        ctx.DrawLine(Vector2 { X: SIDEBAR_W + 4.0, Y: 18.0 }, Vector2 { X: SIDEBAR_W + 4.0, Y: WIN_H - 18.0 }, &sep, 1.0, None);

        // Marca: quadradinho com a cor de acento e o nome.
        round(ctx, Rect::new(PAD + 2.0, 24.0, 34.0, 34.0), 10.0, accent, 1.0)?;
        icon(ctx, gfx, '\u{E8A9}', 16.0, th.on_accent, Rect::new(PAD + 2.0, 24.0, 34.0, 34.0))?;
        text(ctx, gfx, "Pod Studio", TextStyle::Title, ink, 1.0, Rect::new(PAD + 46.0, 22.0, 150.0, 22.0), Align::Start)?;
        text(ctx, gfx, "Widgets do desktop", TextStyle::Caption, soft, 1.0, Rect::new(PAD + 46.0, 42.0, 150.0, 18.0), Align::Start)?;

        // Busca.
        round(ctx, SEARCH, 10.0, ink, tint(0.06, 0.08))?;
        icon(ctx, gfx, '\u{E721}', 13.0, soft, Rect::new(SEARCH.x + 6.0, SEARCH.y, 24.0, SEARCH.h))?;
        let (q, q_color, q_alpha) = if s.query.is_empty() { ("Buscar widgets".to_string(), soft, 0.9) } else { (format!("{}|", s.query), ink, 1.0) };
        text(ctx, gfx, &q, TextStyle::Body, q_color, q_alpha, Rect::new(SEARCH.x + 32.0, SEARCH.y, SEARCH.w - 40.0, SEARCH.h), Align::Start)?;

        // Seções.
        for (i, name) in nav_names().into_iter().enumerate() {
            let r = nav_rect(i);
            let active = i == s.section && (s.query.is_empty() || i == 0);
            let hover = s.hover == Some(Hit::Nav(i));
            if active {
                round(ctx, r, 10.0, accent, if hc { 1.0 } else { 0.16 })?;
            } else if hover {
                round(ctx, r, 10.0, ink, 0.06)?;
            }
            let c = if active && !hc { accent } else if active { th.on_accent } else { ink };
            icon(ctx, gfx, NAV_ICONS[i.min(NAV_ICONS.len() - 1)], 15.0, c, Rect::new(r.x + 8.0, r.y, 26.0, r.h))?;
            text(ctx, gfx, name, TextStyle::Body, c, 1.0, Rect::new(r.x + 40.0, r.y, r.w - 48.0, r.h), Align::Start)?;
        }

        // Dica e "Concluir".
        text(ctx, gfx, "Clique ou arraste um cart\u{e3}o", TextStyle::Caption, soft, 1.0, Rect::new(PAD + 4.0, DONE.y - 62.0, DONE.w, 18.0), Align::Start)?;
        text(ctx, gfx, "para o desktop. Puxe as bordas", TextStyle::Caption, soft, 1.0, Rect::new(PAD + 4.0, DONE.y - 46.0, DONE.w, 18.0), Align::Start)?;
        text(ctx, gfx, "para mudar o tamanho.", TextStyle::Caption, soft, 1.0, Rect::new(PAD + 4.0, DONE.y - 30.0, DONE.w, 18.0), Align::Start)?;
        let done_hover = s.hover == Some(Hit::Done);
        round(ctx, DONE, DONE.h / 2.0, if hc { ink } else { accent }, if done_hover { 0.88 } else { 1.0 })?;
        text(ctx, gfx, "Concluir", TextStyle::Body, th.on_accent, 1.0, DONE, Align::Center)?;

        // Cabeçalho do conteúdo.
        text(ctx, gfx, &s.title(), TextStyle::Title, ink, 1.0, Rect::new(CONTENT_X, 24.0, CONTENT_W - 140.0, 30.0), Align::Start)?;
        if !s.in_settings() {
            let n = s.items.iter().filter(|i| i.visible).count();
            let count = if n == 1 { "1 widget".to_string() } else { format!("{n} widgets") };
            text(ctx, gfx, &count, TextStyle::Caption, soft, 1.0, Rect::new(CONTENT_X + CONTENT_W - 140.0, 28.0, 140.0, 22.0), Align::End)?;
        }

        if s.in_settings() {
            for (i, (title, desc)) in TOGGLES.iter().enumerate() {
                let r = toggle_rect(i);
                let on = if i == 0 { s.glass } else { s.internet };
                round(ctx, r, 16.0, ink, if s.hover == Some(Hit::Toggle(i)) { tint(0.08, 0.10) } else { tint(0.045, 0.06) })?;
                text(ctx, gfx, title, TextStyle::Body, ink, 1.0, Rect::new(r.x + 18.0, r.y + 14.0, r.w - 100.0, 22.0), Align::Start)?;
                text(ctx, gfx, desc, TextStyle::Caption, soft, 1.0, Rect::new(r.x + 18.0, r.y + 36.0, r.w - 100.0, 20.0), Align::Start)?;
                // Interruptor.
                let sw = Rect::new(r.right() - 64.0, r.y + (r.h - 26.0) / 2.0, 46.0, 26.0);
                round(ctx, sw, 13.0, if on { accent } else { ink }, if on { 1.0 } else { 0.22 })?;
                let kx = if on { sw.right() - 13.0 } else { sw.x + 13.0 };
                let knob = ctx.CreateSolidColorBrush(&col([255, 255, 255], 1.0), None)?;
                ctx.FillEllipse(&D2D1_ELLIPSE { point: Vector2 { X: kx, Y: sw.y + 13.0 }, radiusX: 10.0, radiusY: 10.0 }, &knob);
            }
            return Ok(());
        }

        // Catálogo rolável.
        ctx.PushAxisAlignedClip(&D2D_RECT_F { left: CONTENT_X - 6.0, top: CONTENT_Y - 4.0, right: WIN_W - 8.0, bottom: WIN_H - 12.0 }, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        let base_y = CONTENT_Y - s.scroll;
        for (label, y) in &s.headers {
            text(ctx, gfx, label, TextStyle::Caption, soft, 1.0, Rect::new(CONTENT_X + 4.0, base_y + y + 4.0, CONTENT_W, 20.0), Align::Start)?;
        }
        for (i, item) in s.items.iter().enumerate().filter(|(_, it)| it.visible) {
            let r = Rect::new(CONTENT_X + item.rect.x, base_y + item.rect.y, item.rect.w, item.rect.h);
            if r.bottom() < CONTENT_Y - 4.0 || r.y > WIN_H {
                continue;
            }
            let hot = s.hover == Some(Hit::Card(i)) || s.sel == Some(i);
            round(ctx, r, 18.0, ink, if hot { tint(0.08, 0.10) } else { tint(0.04, 0.055) })?;
            if hot {
                outline(ctx, r, 18.0, accent, 0.9, 2.0)?;
            }
            // Prévia reduzida para caber, centrada na área de cima do cartão.
            let (w, h) = item.size;
            let k = ((CARD_W - 32.0) / w).min((PREVIEW_H - 24.0) / h).min(1.0);
            let (px, py) = (r.x + (r.w - w * k) / 2.0, r.y + 14.0 + (PREVIEW_H - 18.0 - h * k) / 2.0);
            ctx.SetTransform(&Matrix3x2 { M11: k, M12: 0.0, M21: 0.0, M22: k, M31: px, M32: py });
            let st = PaintStyle { theme: th, accent: th.accent(item.accent) };
            render::paint_tile_background(ctx, (w, h), &st)?;
            render::paint_view(ctx, gfx, &item.view, &item.laid, &st)?;
            ctx.SetTransform(&Matrix3x2::identity());
            // Nome, tamanho e, depois de adicionar, a contagem.
            text(ctx, gfx, item.name, TextStyle::Body, ink, 1.0, Rect::new(r.x + 16.0, r.y + PREVIEW_H + 4.0, r.w - 80.0, 22.0), Align::Start)?;
            let (tag, tag_color) = if item.added > 0 { (format!("\u{2713} \u{d7}{}", item.added), accent) } else { (item.shape_label.clone(), soft) };
            text(ctx, gfx, &tag, TextStyle::Caption, tag_color, 1.0, Rect::new(r.right() - 76.0, r.y + PREVIEW_H + 4.0, 60.0, 22.0), Align::End)?;
            // A categoria só aparece nos resultados da busca (nas seções ela já é o título).
            if !s.query.is_empty() {
                text(ctx, gfx, crate::widgets::CATEGORIES[item.cat], TextStyle::Caption, soft, 0.8, Rect::new(r.x + 16.0, r.y + PREVIEW_H + 24.0, r.w - 32.0, 18.0), Align::Start)?;
            }
            if hot {
                render::paint_badge(ctx, r.right() - 18.0, r.y + 18.0, true, &st)?;
            }
        }
        ctx.PopAxisAlignedClip();
        if s.items.iter().all(|i| !i.visible) {
            text(ctx, gfx, "Nenhum widget encontrado", TextStyle::Body, soft, 1.0, Rect::new(CONTENT_X, CONTENT_Y + 60.0, CONTENT_W, 24.0), Align::Center)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Janela

struct Gallery {
    shell: Rc<Shell>,
    surface: Option<Surface>,
    dpi: u32,
    studio: Studio,
    /// Cartão pressionado e onde (tela): vira arraste ao passar de 6 px, ou clique ao soltar.
    pressed: Option<(usize, POINT)>,
}

unsafe fn gallery<'a>(hwnd: HWND) -> Option<&'a mut Gallery> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Gallery).as_mut()
}

/// Abre o Studio (encostado à direita do monitor principal). `None` se algo falhar.
pub fn open(shell: &Rc<Shell>) -> Option<HWND> {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).ok()?.into();
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(gallery_proc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW).ok()?,
            lpszClassName: w!("PodGallery"),
            ..Default::default()
        };
        let _ = RegisterClassExW(&class);

        let mut studio = Studio::new(&shell.gfx());
        studio.glass = shell.glass();
        studio.internet = shell.internet();
        let dpi = GetDpiForSystem();
        let px = |d: f32| (d * dpi as f32 / 96.0).round() as i32;
        let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        let (wpx, hpx) = (px(WIN_W), px(WIN_H));
        // Encostado à direita: os cartões do usuário (à esquerda) continuam livres para arrastar.
        let x = (mi.rcWork.right - wpx - px(16.0)).max(mi.rcWork.left);
        let y = mi.rcWork.top + ((mi.rcWork.bottom - mi.rcWork.top) - hpx).max(0) / 2;

        let ptr = Box::into_raw(Box::new(Gallery { shell: shell.clone(), surface: None, dpi, studio, pressed: None }));
        let created = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST,
            w!("PodGallery"),
            w!("Pod Studio"),
            WS_POPUP,
            x,
            y,
            wpx,
            hpx,
            None,
            None,
            Some(hinst),
            Some(ptr as *const _),
        );
        match created {
            Ok(h) => {
                let _ = ShowWindow(h, SW_SHOW);
                let _ = SetForegroundWindow(h);
                Some(h)
            }
            Err(e) => {
                crate::host::log(&format!("studio: CreateWindowExW falhou: {e}"));
                None
            }
        }
    }
}

pub fn close(hwnd: HWND) {
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

fn paint(hwnd: HWND) {
    unsafe {
        let Some(g) = gallery(hwnd) else { return };
        let shell = g.shell.clone();
        let gfx = shell.gfx();
        let theme = shell.theme();
        let Some(surface) = &g.surface else { return };
        let result = surface.draw_custom(g.dpi, |ctx| paint_studio(ctx, &gfx, &g.studio, &theme));
        if let Err(e) = result {
            if is_device_lost(&e) {
                shell.device_lost();
            }
        }
    }
}

fn mouse_dip(g: &Gallery, lp: LPARAM) -> (f32, f32) {
    let (x, y) = ((lp.0 & 0xffff) as i16 as f32, ((lp.0 >> 16) & 0xffff) as i16 as f32);
    let s = g.dpi as f32 / 96.0;
    (x / s, y / s)
}

fn redraw(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

unsafe extern "system" fn gallery_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CREATE => {
            let mut c = RECT::default();
            let _ = GetClientRect(hwnd, &mut c);
            if let Some(g) = gallery(hwnd) {
                let gfx = g.shell.gfx();
                match Surface::new(&gfx, hwnd, (c.right - c.left) as u32, (c.bottom - c.top) as u32) {
                    Ok(s) => g.surface = Some(s),
                    Err(_) => return LRESULT(-1),
                }
                let d = (render::TILE_RADIUS_DIP * 2.0 * g.dpi as f32 / 96.0).round() as i32;
                let rgn = CreateRoundRectRgn(0, 0, c.right - c.left + 1, c.bottom - c.top + 1, d, d);
                SetWindowRgn(hwnd, Some(rgn), true);
            }
            LRESULT(0)
        }
        WM_NCCALCSIZE if wp.0 != 0 => LRESULT(0),
        WM_NCACTIVATE => LRESULT(1),
        WM_NCHITTEST => {
            // A marca (topo da barra lateral) e a faixa do título arrastam a janela.
            let Some(g) = gallery(hwnd) else { return LRESULT(HTCLIENT as isize) };
            let mut r = RECT::default();
            let _ = GetWindowRect(hwnd, &mut r);
            let s = g.dpi as f32 / 96.0;
            let (x, y) = (
                ((lp.0 & 0xffff) as i16 as i32 - r.left) as f32 / s,
                (((lp.0 >> 16) & 0xffff) as i16 as i32 - r.top) as f32 / s,
            );
            let grab = (x < SIDEBAR_W && y < SEARCH.y - 4.0) || (x >= SIDEBAR_W && y < CONTENT_Y - 12.0);
            LRESULT(if grab { HTCAPTION } else { HTCLIENT } as isize)
        }
        WM_PAINT => {
            paint(hwnd);
            let _ = ValidateRect(Some(hwnd), None);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            // Arrasto do Studio para o desktop: depois de 6 px com o botão apertado, cria o cartão
            // sob o cursor e entrega a ele o movimento.
            let drag = gallery(hwnd).and_then(|g| {
                let (idx, start) = g.pressed?;
                let mut now = POINT::default();
                let _ = GetCursorPos(&mut now);
                let far = (now.x - start.x).pow(2) + (now.y - start.y).pow(2) >= 36;
                (wp.0 & 0x0001 != 0 && far).then(|| {
                    g.pressed = None;
                    (g.shell.clone(), g.studio.mark_added(idx), now)
                })
            });
            if let Some((shell, type_id, now)) = drag {
                let _ = ReleaseCapture();
                if let Some(h) = shell.spawn_dragged(&type_id, (now.x, now.y)) {
                    let lparam = LPARAM(((now.y as isize & 0xffff) << 16) | (now.x as isize & 0xffff));
                    SendMessageW(h, WM_SYSCOMMAND, Some(WPARAM(0xF012)), Some(lparam));
                    redraw(hwnd);
                }
                return LRESULT(0);
            }
            if let Some(g) = gallery(hwnd) {
                let (x, y) = mouse_dip(g, lp);
                let now = g.studio.hit(x, y);
                if now != g.studio.hover {
                    g.studio.hover = now;
                    redraw(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if let Some(g) = gallery(hwnd) {
                let delta = ((wp.0 >> 16) & 0xffff) as i16 as f32 / 120.0;
                if g.studio.scroll_by(-delta * 64.0) {
                    redraw(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let Some(g) = gallery(hwnd) else { return LRESULT(0) };
            let (x, y) = mouse_dip(g, lp);
            match g.studio.hit(x, y) {
                Some(Hit::Done) => {
                    let _ = DestroyWindow(hwnd);
                }
                Some(Hit::Nav(i)) => {
                    g.studio.query.clear();
                    g.studio.set_section(i);
                    redraw(hwnd);
                }
                Some(Hit::Toggle(i)) => {
                    let shell = g.shell.clone();
                    if i == 0 {
                        shell.toggle_glass();
                    } else {
                        shell.toggle_internet();
                    }
                    if let Some(g) = gallery(hwnd) {
                        g.studio.glass = shell.glass();
                        g.studio.internet = shell.internet();
                    }
                    redraw(hwnd);
                }
                Some(Hit::Card(i)) => {
                    let mut p = POINT::default();
                    let _ = GetCursorPos(&mut p);
                    g.pressed = Some((i, p));
                    SetCapture(hwnd);
                }
                None => {}
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            let click = gallery(hwnd).and_then(|g| {
                let (idx, _) = g.pressed.take()?;
                Some((g.shell.clone(), g.studio.mark_added(idx)))
            });
            if let Some((shell, t)) = click {
                shell.new_widget(&t);
                let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                redraw(hwnd);
            }
            LRESULT(0)
        }
        // Teclado: setas e Tab movem a seleção, Enter adiciona; letras vão para a busca.
        WM_KEYDOWN if matches!(wp.0, 0x25..=0x28 | 0x09 | 0x0D) => {
            let shift = GetKeyState(0x10) < 0;
            let add = gallery(hwnd).and_then(|g| g.studio.key(wp.0, shift).map(|t| (g.shell.clone(), t)));
            if let Some((shell, t)) = add {
                shell.new_widget(&t);
                let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            redraw(hwnd);
            LRESULT(0)
        }
        WM_CHAR => {
            if let Some(g) = gallery(hwnd) {
                let ch = char::from_u32(wp.0 as u32).unwrap_or('\0');
                if g.studio.type_char(ch) {
                    redraw(hwnd);
                }
            }
            LRESULT(0)
        }
        // Esc limpa a busca; com a busca vazia, fecha o Studio.
        WM_KEYDOWN if wp.0 == 0x1B => {
            let cleared = gallery(hwnd).is_some_and(|g| {
                if g.studio.query.is_empty() {
                    return false;
                }
                g.studio.query.clear();
                g.studio.relayout();
                true
            });
            if cleared {
                redraw(hwnd);
            } else {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Gallery;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !ptr.is_null() {
                let g = Box::from_raw(ptr);
                let shell = g.shell.clone();
                drop(g);
                shell.gallery_closed();
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_flows_in_three_columns_with_section_headers() {
        let (r, h, total) = plan_grid(&[(Some("Tempo"), 4), (Some("Sistema"), 2), (Some("Vazio"), 0)]);
        assert_eq!(r.len(), 6);
        assert_eq!(h.len(), 2, "se\u{e7}\u{e3}o vazia n\u{e3}o tem cabe\u{e7}alho");
        assert_eq!(h[0], ("TEMPO".to_string(), 0.0));
        assert_eq!((r[0].x, r[0].y), (0.0, SECTION_H));
        assert_eq!(r[1].x, CARD_W + GAP);
        assert_eq!(r[3].x, 0.0, "quarto quebra a linha");
        assert_eq!(r[3].y, SECTION_H + CARD_H + GAP);
        assert!(h[1].1 > r[3].bottom(), "segunda se\u{e7}\u{e3}o abaixo da primeira");
        assert!(total > r[5].bottom());
        for a in &r {
            assert!(a.right() <= CONTENT_W + 0.01, "cabe na largura: {a:?}");
        }
    }

    #[test]
    fn grid_without_headers_and_empty() {
        let (r, h, _) = plan_grid(&[(None, 2)]);
        assert!(h.is_empty() && r[0].y == 0.0);
        let (r, h, total) = plan_grid(&[]);
        assert!(r.is_empty() && h.is_empty() && total == 0.0);
    }

    #[test]
    fn category_and_search_filter_together() {
        let items = [(1, "relogio tempo"), (1, "calendario tempo"), (3, "cpu sistema"), (4, "area de transferencia arquivos")];
        let f = |c, q: &str| filter_flags(items.iter().copied(), c, q);
        assert_eq!(f(0, ""), vec![true; 4]);
        assert_eq!(f(1, ""), vec![true, true, false, false]);
        assert_eq!(f(0, "area"), vec![false, false, false, true]);
        assert_eq!(f(0, "Calend\u{e1}rio"), vec![false, true, false, false], "acento e mai\u{fa}scula");
        assert_eq!(f(3, "relogio"), vec![false; 4]);
    }

    #[test]
    fn arrow_keys_move_through_rows_and_stay_in_range() {
        let r = vec![
            Rect::new(16.0, 16.0, 188.0, 88.0),
            Rect::new(220.0, 16.0, 188.0, 88.0),
            Rect::new(424.0, 16.0, 188.0, 88.0),
            Rect::new(16.0, 150.0, 188.0, 188.0),
            Rect::new(220.0, 150.0, 88.0, 88.0),
        ];
        assert_eq!(neighbor(&r, 0, Dir::Right), 1);
        assert_eq!(neighbor(&r, 4, Dir::Right), 4);
        assert_eq!(neighbor(&r, 0, Dir::Left), 0);
        assert_eq!(neighbor(&r, 1, Dir::Down), 4);
        assert_eq!(neighbor(&r, 3, Dir::Up), 0);
        assert_eq!(neighbor(&r, 0, Dir::Up), 0);
        assert_eq!(neighbor(&[], 0, Dir::Down), 0);
    }

    #[test]
    fn sidebar_and_buttons_fit_the_window_and_do_not_overlap() {
        let n = nav_names().len();
        assert_eq!(n, NAV_ICONS.len());
        let last = nav_rect(n - 1);
        assert!(last.bottom() + 60.0 < DONE.y, "a dica cabe entre a \u{fa}ltima se\u{e7}\u{e3}o e Concluir");
        assert!(DONE.bottom() <= WIN_H - 12.0 && SEARCH.right() <= SIDEBAR_W);
        assert!(toggle_rect(TOGGLES.len() - 1).bottom() < WIN_H);
        assert!(CARD_W > 200.0, "cart\u{e3}o largo o bastante para a pr\u{e9}via 2\u{d7}1");
    }
}
