//! Galeria do modo de edição, encostada na base da tela (como no mockup "Modo de edição + galeria"): uma
//! faixa com as categorias e uma fileira de widgets. Clicar num widget o adiciona; arrastar o leva ao
//! desktop. "Concluído" (ou Esc) sai da edição. Os selos "−" nos cartões e os espaços "+" são desenhados
//! pelos próprios cartões e pela grade.
//!
//! O estado (`Dock`: itens, categoria, rolagem, seleção) e a geometria não dependem de janela e têm testes.

use std::rc::Rc;

use windows::core::{w, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::edit::{icon, outline, round, text};
use crate::gfx::{is_device_lost, Gfx, Surface};
use crate::icons;
use crate::shell::Shell;
use crate::ui::theme::{Mode, Theme};
use pod_model::ids::WidgetTypeId;
use pod_model::layout::Rect;
use pod_model::view::{Align, Kind, Node, TextStyle};

pub const HEIGHT: f32 = 196.0;
const MAX_W: f32 = 920.0;
const MARGIN: f32 = 16.0;
const PAD: f32 = 22.0;
const HEADER_Y: f32 = 16.0;
const HEADER_H: f32 = 32.0;
const CHIP_W: f32 = 92.0;
const CHIP_H: f32 = 108.0;
const TILE: f32 = 64.0;
const CHIPS_Y: f32 = 70.0;
const RADIUS: f32 = 22.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Chip(usize),
    Tab(usize),
    Restore,
    Done,
}

struct Entry {
    type_id: WidgetTypeId,
    name: &'static str,
    cat: usize,
    accent: usize,
    glyph: Option<char>,
}

/// Primeiro ícone desenhado na prévia do widget: é o símbolo do cabeçalho do cartão.
pub fn first_icon(n: &Node) -> Option<char> {
    match &n.kind {
        Kind::Icon { glyph, .. } => Some(*glyph),
        Kind::Text { text, .. } => {
            let mut it = text.chars();
            it.next().filter(|c| it.next().is_none() && !c.is_alphanumeric())
        }
        Kind::Stack { children, .. } | Kind::Layers { children } => children.iter().find_map(first_icon),
        Kind::Plate { child, .. } => first_icon(child),
        _ => None,
    }
}

/// Símbolo (Segoe MDL2) do widget quando a prévia não tem um ícone no cabeçalho: por nome e, na falta, pela categoria.
fn fallback_glyph(name: &str, cat: usize) -> char {
    match name {
        "Rel\u{f3}gio" => '\u{E121}',
        "Calend\u{e1}rio" => '\u{E787}',
        "Contagem regressiva" | "Timer" | "Cron\u{f4}metro" | "Pomodoro" => '\u{E916}',
        "Bateria" => '\u{EBA6}',
        "Notifica\u{e7}\u{f5}es" => '\u{E7E7}',
        "M\u{fa}sica" => '\u{E8D6}',
        _ => ['\u{E80A}', '\u{E823}', '\u{E8FD}', '\u{E7F4}', '\u{E8B7}', '\u{E774}'][cat.min(5)],
    }
}

pub struct Dock {
    items: Vec<Entry>,
    pub width: f32,
    pub category: usize,
    pub scroll: f32,
    pub hover: Option<Hit>,
    pub sel: Option<usize>,
}

fn tab_width(label: &str) -> f32 {
    label.chars().count() as f32 * 6.4 + 22.0
}

impl Dock {
    pub fn new(width: f32) -> Self {
        let items = crate::widgets::catalog()
            .into_iter()
            .map(|(type_id, name)| {
                let cat = crate::widgets::category_of(&type_id);
                let glyph = crate::gallery::preview(&type_id).and_then(|(_, v)| first_icon(&v.root)).or_else(|| Some(fallback_glyph(name, cat)));
                Entry { accent: crate::widgets::accent_for(&type_id).unwrap_or(0), cat, type_id, name, glyph }
            })
            .collect();
        Self { items, width, category: 0, scroll: 0.0, hover: None, sel: None }
    }

    /// Índices dos itens da categoria atual (0 = todos), na ordem do catálogo.
    pub fn visible(&self) -> Vec<usize> {
        (0..self.items.len()).filter(|&i| self.category == 0 || self.items[i].cat == self.category).collect()
    }

    pub fn done_rect(&self) -> Rect {
        Rect::new(self.width - PAD - 104.0, HEADER_Y, 104.0, HEADER_H)
    }

    pub fn restore_rect(&self) -> Rect {
        let done = self.done_rect();
        Rect::new(done.x - 8.0 - 120.0, HEADER_Y, 120.0, HEADER_H)
    }

    pub fn tab_rect(&self, c: usize) -> Rect {
        let mut x = 168.0;
        for i in 0..c {
            x += tab_width(crate::widgets::CATEGORIES[i]) + 6.0;
        }
        Rect::new(x, HEADER_Y, tab_width(crate::widgets::CATEGORIES[c]), HEADER_H)
    }

    fn viewport_w(&self) -> f32 {
        self.width - 2.0 * PAD
    }

    pub fn max_scroll(&self) -> f32 {
        (self.visible().len() as f32 * CHIP_W - self.viewport_w()).max(0.0)
    }

    pub fn scroll_by(&mut self, dx: f32) -> bool {
        let to = (self.scroll + dx).clamp(0.0, self.max_scroll());
        let changed = (to - self.scroll).abs() > 0.01;
        self.scroll = to;
        changed
    }

    /// Retângulo do `slot`-ésimo item visível (DIP da janela, já com a rolagem).
    pub fn chip_rect(&self, slot: usize) -> Rect {
        Rect::new(PAD + slot as f32 * CHIP_W - self.scroll, CHIPS_Y, CHIP_W, CHIP_H)
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        let inside = |r: Rect| x >= r.x && x < r.right() && y >= r.y && y < r.bottom();
        if inside(self.done_rect()) {
            return Some(Hit::Done);
        }
        if inside(self.restore_rect()) {
            return Some(Hit::Restore);
        }
        for c in 0..crate::widgets::CATEGORIES.len() {
            if inside(self.tab_rect(c)) {
                return Some(Hit::Tab(c));
            }
        }
        if x >= PAD && x < self.width - PAD {
            for (slot, &i) in self.visible().iter().enumerate() {
                if inside(self.chip_rect(slot)) {
                    return Some(Hit::Chip(i));
                }
            }
        }
        None
    }

    pub fn set_category(&mut self, c: usize) {
        self.category = c.min(crate::widgets::CATEGORIES.len() - 1);
        self.scroll = 0.0;
        self.sel = None;
    }

    /// Move a seleção do teclado para o item vizinho (dentro da categoria) e a traz para a vista.
    pub fn step(&mut self, forward: bool) {
        let vis = self.visible();
        if vis.is_empty() {
            return;
        }
        let pos = self.sel.and_then(|s| vis.iter().position(|&i| i == s));
        let next = match (pos, forward) {
            (Some(p), true) => (p + 1).min(vis.len() - 1),
            (Some(p), false) => p.saturating_sub(1),
            (None, _) => 0,
        };
        self.sel = Some(vis[next]);
        let r = self.chip_rect(next);
        if r.x < PAD {
            self.scroll_by(r.x - PAD);
        } else if r.right() > self.width - PAD {
            self.scroll_by(r.right() - (self.width - PAD));
        }
    }

    pub fn type_of(&self, i: usize) -> &WidgetTypeId {
        &self.items[i].type_id
    }
}

fn paint_dock(ctx: &windows::Win32::Graphics::Direct2D::ID2D1DeviceContext, gfx: &Gfx, d: &Dock, th: &Theme) -> Result<()> {
    let dark = th.mode == Mode::Dark;
    let size = (d.width, HEIGHT);
    let bg = if dark { th.surface_bottom } else { th.surface_top };
    round(ctx, Rect::new(0.0, 0.0, size.0, size.1), RADIUS, bg, th.surface_alpha)?;
    outline(ctx, Rect::new(0.0, 0.0, size.0, size.1), RADIUS, th.border, th.border_alpha.max(0.35), 1.0)?;
    text(ctx, gfx, "Galeria de widgets", TextStyle::Label, th.text, 1.0, Rect::new(PAD, HEADER_Y, 180.0, HEADER_H), Align::Start)?;
    for (c, name) in crate::widgets::CATEGORIES.iter().enumerate() {
        let r = d.tab_rect(c);
        let on = d.category == c;
        let hot = d.hover == Some(Hit::Tab(c));
        if on {
            round(ctx, r, r.h / 2.0, th.accent(0), 1.0)?;
        } else if hot {
            round(ctx, r, r.h / 2.0, th.text_secondary, 0.18)?;
        } else {
            round(ctx, r, r.h / 2.0, th.text_secondary, 0.10)?;
        }
        text(ctx, gfx, name, TextStyle::Caption, if on { th.on_accent } else { th.text }, 1.0, r, Align::Center)?;
    }
    let restore = d.restore_rect();
    round(ctx, restore, restore.h / 2.0, th.text_secondary, if d.hover == Some(Hit::Restore) { 0.26 } else { 0.14 })?;
    text(ctx, gfx, "Restaurar padr\u{e3}o", TextStyle::Caption, th.text, 1.0, restore, Align::Center)?;
    let done = d.done_rect();
    round(ctx, done, done.h / 2.0, th.accent(0), if d.hover == Some(Hit::Done) { 0.88 } else { 1.0 })?;
    text(ctx, gfx, "Conclu\u{ed}do", TextStyle::Label, th.on_accent, 1.0, done, Align::Center)?;

    for (slot, &i) in d.visible().iter().enumerate() {
        let r = d.chip_rect(slot);
        if r.right() < PAD - 4.0 || r.x > size.0 - PAD + 4.0 {
            continue;
        }
        let e = &d.items[i];
        let tile = Rect::new(r.x + (CHIP_W - TILE) / 2.0, r.y, TILE, TILE);
        let accent = th.accent(e.accent);
        let hot = d.hover == Some(Hit::Chip(i)) || d.sel == Some(i);
        round(ctx, tile, 16.0, accent, 1.0)?;
        round(ctx, Rect::new(tile.x, tile.y, tile.w, tile.h / 2.0), 16.0, [255, 255, 255], 0.12)?;
        if hot {
            outline(ctx, Rect::new(tile.x - 3.0, tile.y - 3.0, tile.w + 6.0, tile.h + 6.0), 19.0, th.text, 0.55, 2.0)?;
        }
        let (cx, cy) = (tile.x + tile.w / 2.0, tile.y + tile.h / 2.0);
        match e.glyph {
            Some(g) if icons::has(g) => {
                icons::paint(ctx, gfx, g, cx, cy, 34.0, &icons::Palette { ink: th.on_accent, neutral: th.on_accent, warm: th.on_accent })?;
            }
            Some(g) if (g as u32) >= 0xE000 && (g as u32) < 0xF900 => icon(ctx, gfx, g, 28.0, th.on_accent, tile)?,
            Some(g) => text(ctx, gfx, &g.to_string(), TextStyle::Title, th.on_accent, 1.0, tile, Align::Center)?,
            None => {
                let letter: String = e.name.chars().next().into_iter().collect();
                text(ctx, gfx, &letter, TextStyle::Title, th.on_accent, 1.0, tile, Align::Center)?;
            }
        }
        text(ctx, gfx, e.name, TextStyle::Caption, th.text, 1.0, Rect::new(r.x + 2.0, r.y + TILE + 8.0, CHIP_W - 4.0, 30.0), Align::Center)?;
    }
    // Setas de rolagem discretas quando há mais itens além da borda.
    if d.scroll > 0.5 {
        icon(ctx, gfx, '\u{E76B}', 12.0, th.text_secondary, Rect::new(2.0, CHIPS_Y + 20.0, 18.0, 24.0))?;
    }
    if d.scroll < d.max_scroll() - 0.5 {
        icon(ctx, gfx, '\u{E76C}', 12.0, th.text_secondary, Rect::new(size.0 - 20.0, CHIPS_Y + 20.0, 18.0, 24.0))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------- janela

struct Window {
    shell: Rc<Shell>,
    surface: Option<Surface>,
    dpi: u32,
    dock: Dock,
    /// Item pressionado e onde (tela): vira arraste ao passar de 6 px, ou clique ao soltar.
    pressed: Option<(usize, POINT)>,
}

unsafe fn win<'a>(hwnd: HWND) -> Option<&'a mut Window> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Window).as_mut()
}

/// Abre a galeria na base do monitor principal. `None` se algo falhar.
pub fn open(shell: &Rc<Shell>) -> Option<HWND> {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).ok()?.into();
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(dock_proc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW).ok()?,
            lpszClassName: w!("PodDock"),
            ..Default::default()
        };
        let _ = RegisterClassExW(&class);
        let dpi = GetDpiForSystem();
        let px = |d: f32| (d * dpi as f32 / 96.0).round() as i32;
        let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        let work_w = (mi.rcWork.right - mi.rcWork.left) as f32 * 96.0 / dpi as f32;
        let width = (work_w - 2.0 * MARGIN).clamp(520.0, MAX_W);
        let (wpx, hpx) = (px(width), px(HEIGHT));
        let x = mi.rcWork.left + ((mi.rcWork.right - mi.rcWork.left) - wpx) / 2;
        let y = mi.rcWork.bottom - hpx - px(MARGIN);
        let ptr = Box::into_raw(Box::new(Window { shell: shell.clone(), surface: None, dpi, dock: Dock::new(width), pressed: None }));
        let created = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST,
            w!("PodDock"),
            w!("Galeria de widgets"),
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
                crate::host::log(&format!("dock: CreateWindowExW falhou: {e}"));
                drop(Box::from_raw(ptr));
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

fn redraw(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn paint(hwnd: HWND) {
    unsafe {
        let Some(g) = win(hwnd) else { return };
        let shell = g.shell.clone();
        let (gfx, theme) = (shell.gfx(), shell.theme());
        let Some(surface) = &g.surface else { return };
        if let Err(e) = surface.draw_custom(g.dpi, |ctx| paint_dock(ctx, &gfx, &g.dock, &theme)) {
            if is_device_lost(&e) {
                shell.device_lost();
            }
        }
    }
}

fn mouse_dip(g: &Window, lp: LPARAM) -> (f32, f32) {
    let (x, y) = ((lp.0 & 0xffff) as i16 as f32, ((lp.0 >> 16) & 0xffff) as i16 as f32);
    let s = g.dpi as f32 / 96.0;
    (x / s, y / s)
}

fn add(hwnd: HWND, shell: &Rc<Shell>, t: &WidgetTypeId) {
    shell.new_widget(t);
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

unsafe extern "system" fn dock_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CREATE => {
            if let Some(g) = win(hwnd) {
                let gfx = g.shell.gfx();
                let (w, h) = ((g.dock.width * g.dpi as f32 / 96.0).round() as u32, (HEIGHT * g.dpi as f32 / 96.0).round() as u32);
                match Surface::new(&gfx, hwnd, w, h) {
                    Ok(s) => g.surface = Some(s),
                    Err(_) => return LRESULT(-1),
                }
                let round_corners: i32 = 2; // DWMWCP_ROUND
                let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                    hwnd,
                    windows::Win32::Graphics::Dwm::DWMWA_WINDOW_CORNER_PREFERENCE,
                    &round_corners as *const _ as *const core::ffi::c_void,
                    4,
                );
            }
            LRESULT(0)
        }
        WM_NCCALCSIZE if wp.0 != 0 => LRESULT(0),
        WM_NCACTIVATE => LRESULT(1),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
            let _ = ValidateRect(Some(hwnd), None);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let drag = win(hwnd).and_then(|g| {
                let (idx, start) = g.pressed?;
                let mut now = POINT::default();
                let _ = GetCursorPos(&mut now);
                let far = (now.x - start.x).pow(2) + (now.y - start.y).pow(2) >= 36;
                (wp.0 & 0x0001 != 0 && far).then(|| {
                    g.pressed = None;
                    (g.shell.clone(), g.dock.type_of(idx).clone(), now)
                })
            });
            if let Some((shell, type_id, now)) = drag {
                let _ = ReleaseCapture();
                if let Some(h) = shell.spawn_dragged(&type_id, (now.x, now.y)) {
                    let lparam = LPARAM(((now.y as isize & 0xffff) << 16) | (now.x as isize & 0xffff));
                    SendMessageW(h, WM_SYSCOMMAND, Some(WPARAM(0xF012)), Some(lparam));
                }
                return LRESULT(0);
            }
            if let Some(g) = win(hwnd) {
                let (x, y) = mouse_dip(g, lp);
                let now = g.dock.hit(x, y);
                if now != g.dock.hover {
                    g.dock.hover = now;
                    redraw(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if let Some(g) = win(hwnd) {
                let delta = ((wp.0 >> 16) & 0xffff) as i16 as f32 / 120.0;
                if g.dock.scroll_by(-delta * CHIP_W) {
                    redraw(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let Some(g) = win(hwnd) else { return LRESULT(0) };
            let (x, y) = mouse_dip(g, lp);
            match g.dock.hit(x, y) {
                Some(Hit::Done) => {
                    let _ = DestroyWindow(hwnd);
                }
                Some(Hit::Tab(c)) => {
                    g.dock.set_category(c);
                    redraw(hwnd);
                }
                Some(Hit::Restore) => {
                    let shell = g.shell.clone();
                    shell.restore_defaults(Some(hwnd));
                    redraw(hwnd);
                }
                Some(Hit::Chip(i)) => {
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
            let click = win(hwnd).and_then(|g| {
                let (idx, _) = g.pressed.take()?;
                Some((g.shell.clone(), g.dock.type_of(idx).clone()))
            });
            if let Some((shell, t)) = click {
                add(hwnd, &shell, &t);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            match wp.0 as u16 {
                0x1B => {
                    let _ = DestroyWindow(hwnd);
                }
                0x25 | 0x27 => {
                    if let Some(g) = win(hwnd) {
                        g.dock.step(wp.0 == 0x27);
                        redraw(hwnd);
                    }
                }
                0x0D | 0x20 => {
                    let pick = win(hwnd).and_then(|g| g.dock.sel.map(|i| (g.shell.clone(), g.dock.type_of(i).clone())));
                    if let Some((shell, t)) = pick {
                        add(hwnd, &shell, &t);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let ptr = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut Window;
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
    fn every_catalog_widget_is_in_the_dock_and_categories_filter_it() {
        let mut d = Dock::new(900.0);
        assert_eq!(d.visible().len(), crate::widgets::catalog().len());
        d.set_category(1);
        assert!(!d.visible().is_empty() && d.visible().len() < crate::widgets::catalog().len());
    }

    #[test]
    fn chips_and_buttons_fit_inside_the_window_and_do_not_overlap_the_done_button() {
        let d = Dock::new(900.0);
        let done = d.done_rect();
        assert!(done.right() <= d.width && done.bottom() <= HEIGHT);
        let last_tab = d.tab_rect(crate::widgets::CATEGORIES.len() - 1);
        assert!(last_tab.right() < d.restore_rect().x, "as abas não podem passar do botão Restaurar padrão");
        assert!(d.restore_rect().right() < done.x);
        let chip = d.chip_rect(0);
        assert!(chip.bottom() <= HEIGHT);
    }

    #[test]
    fn hit_finds_chip_tab_and_done() {
        let d = Dock::new(900.0);
        let c = d.chip_rect(0);
        assert!(matches!(d.hit(c.x + 10.0, c.y + 10.0), Some(Hit::Chip(_))));
        let t = d.tab_rect(2);
        assert_eq!(d.hit(t.x + 4.0, t.y + 4.0), Some(Hit::Tab(2)));
        let done = d.done_rect();
        assert_eq!(d.hit(done.x + 4.0, done.y + 4.0), Some(Hit::Done));
        let rr = d.restore_rect();
        assert_eq!(d.hit(rr.x + 4.0, rr.y + 4.0), Some(Hit::Restore));
        assert_eq!(d.hit(1.0, HEIGHT - 1.0), None);
    }

    #[test]
    fn scroll_and_keyboard_selection_stay_in_range() {
        let mut d = Dock::new(520.0);
        assert!(d.max_scroll() > 0.0);
        assert!(!d.scroll_by(-50.0));
        assert!(d.scroll_by(1.0e6));
        assert_eq!(d.scroll, d.max_scroll());
        d.scroll = 0.0;
        for _ in 0..40 {
            d.step(true);
        }
        let last = *d.visible().last().unwrap();
        assert_eq!(d.sel, Some(last));
        assert!(d.chip_rect(d.visible().len() - 1).right() <= d.width - PAD + 0.5);
        for _ in 0..80 {
            d.step(false);
        }
        assert_eq!(d.sel, Some(d.visible()[0]));
        assert_eq!(d.scroll, 0.0);
    }

    #[test]
    fn most_widgets_have_a_header_symbol_for_their_tile() {
        let d = Dock::new(900.0);
        let with = d.items.iter().filter(|e| e.glyph.is_some()).count();
        assert!(with == d.items.len(), "só {with} de {} têm símbolo", d.items.len());
    }
}
