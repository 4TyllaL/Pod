//! Menu próprio da bandeja (estilo macOS): painel de cantos arredondados, item sob o cursor em pílula
//! com a cor de destaque, ícones, marca de seleção à esquerda e submenus ao lado.
//!
//! Uma janela principal recebe todo o mouse (captura) e o teclado e guarda o estado dos dois painéis;
//! o submenu é uma segunda janela só de desenho (transparente ao mouse). O menu não bloqueia: ao
//! escolher um item, fecha e chama `on_pick` com o `cmd` do item.
//!
//! A estrutura (`Item`), a medida (`plan`), a busca de item (`hit`) e a navegação (`step`) não
//! dependem de janela e têm testes.

use std::cell::Cell;
use std::rc::Rc;

use windows::core::{w, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::edit::{icon, outline, round, text};
use crate::gfx::{is_device_lost, Gfx, Surface};
use crate::shell::Shell;
use crate::ui::theme::{Mode, Theme};
use pod_model::layout::Rect;
use pod_model::view::{Align, TextStyle};

const PAD_Y: f32 = 6.0;
const PAD_X: f32 = 6.0;
const ROW_H: f32 = 30.0;
const SEP_H: f32 = 11.0;
const MIN_W: f32 = 232.0;
const RADIUS: f32 = 12.0;
const PILL_RADIUS: f32 = 7.0;
const CHECK: char = '\u{E73E}';
const CHEVRON: char = '\u{E76C}';

pub enum Kind {
    Action,
    Separator,
    Sub(Vec<Item>),
}

pub struct Item {
    pub cmd: usize,
    pub label: String,
    pub glyph: Option<char>,
    pub checked: bool,
    pub kind: Kind,
}

impl Item {
    pub fn action(cmd: usize, label: impl Into<String>) -> Self {
        Self { cmd, label: label.into(), glyph: None, checked: false, kind: Kind::Action }
    }
    pub fn separator() -> Self {
        Self { cmd: 0, label: String::new(), glyph: None, checked: false, kind: Kind::Separator }
    }
    pub fn sub(label: impl Into<String>, children: Vec<Item>) -> Self {
        Self { cmd: 0, label: label.into(), glyph: None, checked: false, kind: Kind::Sub(children) }
    }
    pub fn glyph(mut self, g: char) -> Self {
        self.glyph = Some(g);
        self
    }
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }
    fn selectable(&self) -> bool {
        !matches!(self.kind, Kind::Separator)
    }
}

/// Retângulos dos itens (DIP, relativos ao painel) e o tamanho do painel.
pub fn plan(items: &[Item]) -> (Vec<Rect>, f32, f32) {
    let longest = items.iter().map(|i| i.label.chars().count()).max().unwrap_or(0) as f32;
    let has_icon = items.iter().any(|i| i.glyph.is_some());
    let w = (longest * 6.7 + if has_icon { 98.0 } else { 74.0 }).max(MIN_W).ceil();
    let mut y = PAD_Y;
    let mut rects = Vec::with_capacity(items.len());
    for i in items {
        let h = if i.selectable() { ROW_H } else { SEP_H };
        rects.push(Rect::new(PAD_X, y, w - 2.0 * PAD_X, h));
        y += h;
    }
    (rects, w, y + PAD_Y)
}

/// Item sob o ponto (DIP do painel); separadores não contam.
pub fn hit(items: &[Item], rects: &[Rect], x: f32, y: f32) -> Option<usize> {
    rects.iter().position(|r| x >= r.x && x < r.right() && y >= r.y && y < r.bottom()).filter(|&i| items[i].selectable())
}

/// Próximo item selecionável a partir de `from` (sem ele: o primeiro ou o último), dando a volta.
pub fn step(items: &[Item], from: Option<usize>, down: bool) -> Option<usize> {
    let n = items.len();
    if !items.iter().any(Item::selectable) {
        return None;
    }
    let mut i = match (from, down) {
        (Some(f), _) => f,
        (None, true) => n - 1,
        (None, false) => 0,
    };
    loop {
        i = if down { (i + 1) % n } else { (i + n - 1) % n };
        if items[i].selectable() {
            return Some(i);
        }
    }
}

fn paint_panel(ctx: &windows::Win32::Graphics::Direct2D::ID2D1DeviceContext, gfx: &Gfx, items: &[Item], rects: &[Rect], size: (f32, f32), hover: Option<usize>, th: &Theme) -> Result<()> {
    let dark = th.mode == Mode::Dark;
    let bg = if dark { th.surface_bottom } else { th.surface_top };
    round(ctx, Rect::new(0.0, 0.0, size.0, size.1), RADIUS, bg, th.surface_alpha)?;
    outline(ctx, Rect::new(0.0, 0.0, size.0, size.1), RADIUS, th.border, th.border_alpha.max(0.35), 1.0)?;
    let has_icon = items.iter().any(|i| i.glyph.is_some());
    let label_x = if has_icon { 56.0 } else { 32.0 };
    for (i, (item, r)) in items.iter().zip(rects).enumerate() {
        if let Kind::Separator = item.kind {
            round(ctx, Rect::new(r.x + 8.0, r.y + r.h / 2.0, r.w - 16.0, 1.0), 0.5, th.text_secondary, 0.28)?;
            continue;
        }
        let on = hover == Some(i);
        let ink = if on { th.on_accent } else { th.text };
        if on {
            round(ctx, *r, PILL_RADIUS, th.accent(0), 1.0)?;
        }
        if item.checked {
            icon(ctx, gfx, CHECK, 12.0, ink, Rect::new(r.x + 4.0, r.y, 20.0, r.h))?;
        }
        if let Some(g) = item.glyph {
            icon(ctx, gfx, g, 15.0, if on { th.on_accent } else { th.text_secondary }, Rect::new(r.x + 26.0, r.y, 22.0, r.h))?;
        }
        text(ctx, gfx, &item.label, TextStyle::Body, ink, 1.0, Rect::new(r.x + label_x - PAD_X, r.y, r.w - label_x - 6.0, r.h), Align::Start)?;
        if matches!(item.kind, Kind::Sub(_)) {
            icon(ctx, gfx, CHEVRON, 11.0, if on { th.on_accent } else { th.text_secondary }, Rect::new(r.right() - 24.0, r.y, 18.0, r.h))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------- janelas

struct Panel {
    hwnd: HWND,
    surface: Option<Surface>,
    items_rects: Vec<Rect>,
    size: (f32, f32),
    hover: Option<usize>,
    /// Canto superior esquerdo na tela (px).
    origin: (i32, i32),
}

struct Menu {
    shell: Rc<Shell>,
    dpi: u32,
    items: Vec<Item>,
    main: Panel,
    sub: Option<(usize, Panel)>,
    focus_sub: bool,
    on_pick: Rc<dyn Fn(usize)>,
    closing: Cell<bool>,
    /// Última posição do cursor tratada: o Windows reenvia WM_MOUSEMOVE sem movimento (janela nova sob o cursor) e isso não deve mexer no realce do teclado.
    last_pt: Cell<(i32, i32)>,
}

thread_local! {
    static OPEN: Cell<isize> = const { Cell::new(0) };
}

unsafe fn menu<'a>(hwnd: HWND) -> Option<&'a mut Menu> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Menu).as_mut()
}

fn px(d: f32, dpi: u32) -> i32 {
    (d * dpi as f32 / 96.0).round() as i32
}

/// Abre o menu na posição do cursor. Se já há um aberto, fecha-o (o clique no ícone alterna).
pub fn open(shell: &Rc<Shell>, items: Vec<Item>, on_pick: Rc<dyn Fn(usize)>) {
    unsafe {
        let prev = OPEN.with(Cell::get);
        if prev != 0 {
            let _ = DestroyWindow(HWND(prev as *mut _));
            return;
        }
        let Ok(hinst) = GetModuleHandleW(None).map(HINSTANCE::from) else { return };
        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        for (name, proc) in [(w!("PodMenu"), Some(menu_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT)), (w!("PodMenuSub"), Some(sub_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT))] {
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_DROPSHADOW,
                lpfnWndProc: proc,
                hInstance: hinst,
                hCursor: cursor,
                lpszClassName: name,
                ..Default::default()
            };
            let _ = RegisterClassExW(&class);
        }
        let dpi = GetDpiForSystem();
        let (rects, w, h) = plan(&items);
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let work = work_area(pt);
        let (wpx, hpx) = (px(w, dpi), px(h, dpi));
        let x = if pt.x + wpx > work.right { (pt.x - wpx).max(work.left) } else { pt.x };
        let y = if pt.y + hpx > work.bottom { (pt.y - hpx).max(work.top) } else { pt.y };
        let state = Menu {
            shell: shell.clone(),
            dpi,
            items,
            main: Panel { hwnd: HWND::default(), surface: None, items_rects: rects, size: (w, h), hover: None, origin: (x, y) },
            sub: None,
            focus_sub: false,
            on_pick,
            closing: Cell::new(false),
            last_pt: Cell::new((i32::MIN, i32::MIN)),
        };
        let ptr = Box::into_raw(Box::new(state));
        let created = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST,
            w!("PodMenu"),
            w!("Pod"),
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
            Ok(hwnd) => {
                OPEN.with(|o| o.set(hwnd.0 as isize));
                let _ = ShowWindow(hwnd, SW_SHOW);
                take_foreground(hwnd);
                SetCapture(hwnd);
            }
            Err(e) => {
                drop(Box::from_raw(ptr));
            }
        }
    }
}

/// Pede o primeiro plano (teclado) mesmo quando o Windows não deu o direito, anexando a fila de
/// entrada à do primeiro plano atual por um instante.
unsafe fn take_foreground(hwnd: HWND) {
    let fg = GetForegroundWindow();
    let fg_thread = GetWindowThreadProcessId(fg, None);
    let me = GetCurrentThreadId();
    let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
    let _ = SetForegroundWindow(hwnd);
    let _ = SetFocus(Some(hwnd));
    if attached {
        let _ = AttachThreadInput(me, fg_thread, false);
    }
}

fn work_area(pt: POINT) -> RECT {
    unsafe {
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        mi.rcWork
    }
}

fn close_all(hwnd: HWND) {
    unsafe {
        if let Some(m) = menu(hwnd) {
            if m.closing.replace(true) {
                return;
            }
            if let Some((_, p)) = m.sub.take() {
                let _ = DestroyWindow(p.hwnd);
            }
        }
        let _ = ReleaseCapture();
        let _ = DestroyWindow(hwnd);
    }
}

fn pick(hwnd: HWND, cmd: usize) {
    let cb = unsafe { menu(hwnd).map(|m| m.on_pick.clone()) };
    close_all(hwnd);
    if let Some(cb) = cb {
        cb(cmd);
    }
}

fn close_sub(m: &mut Menu) {
    if let Some((_, p)) = m.sub.take() {
        unsafe {
            let _ = DestroyWindow(p.hwnd);
        }
    }
    m.focus_sub = false;
}

/// Abre (ou troca) o submenu do item `idx` ao lado do painel principal.
fn open_sub(hwnd: HWND, m: &mut Menu, idx: usize) {
    if matches!(&m.sub, Some((i, _)) if *i == idx) {
        return;
    }
    close_sub(m);
    let Kind::Sub(children) = &m.items[idx].kind else { return };
    unsafe {
        let Ok(hinst) = GetModuleHandleW(None).map(HINSTANCE::from) else { return };
        let (rects, w, h) = plan(children);
        let dpi = m.dpi;
        let (wpx, hpx) = (px(w, dpi), px(h, dpi));
        let row = m.main.items_rects[idx];
        let work = work_area(POINT { x: m.main.origin.0, y: m.main.origin.1 });
        let main_w = px(m.main.size.0, dpi);
        let mut x = m.main.origin.0 + main_w - px(4.0, dpi);
        if x + wpx > work.right {
            x = (m.main.origin.0 - wpx + px(4.0, dpi)).max(work.left);
        }
        let y = (m.main.origin.1 + px(row.y - PAD_Y, dpi)).min(work.bottom - hpx).max(work.top);
        let created = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            w!("PodMenuSub"),
            w!("Pod"),
            WS_POPUP,
            x,
            y,
            wpx,
            hpx,
            Some(hwnd),
            None,
            Some(hinst),
            Some(m as *mut Menu as *const _),
        );
        if let Ok(sub) = created {
            m.sub = Some((idx, Panel { hwnd: sub, surface: None, items_rects: rects, size: (w, h), hover: None, origin: (x, y) }));
            let _ = ShowWindow(sub, SW_SHOWNOACTIVATE);
        }
    }
}

fn redraw(m: &Menu) {
    unsafe {
        let _ = InvalidateRect(Some(m.main.hwnd), None, false);
        if let Some((_, p)) = &m.sub {
            let _ = InvalidateRect(Some(p.hwnd), None, false);
        }
    }
}

fn to_dip(m: &Menu, p: &Panel, pt: POINT) -> (f32, f32) {
    let s = m.dpi as f32 / 96.0;
    ((pt.x - p.origin.0) as f32 / s, (pt.y - p.origin.1) as f32 / s)
}

fn inside(m: &Menu, p: &Panel, pt: POINT) -> bool {
    let (x, y) = to_dip(m, p, pt);
    x >= 0.0 && y >= 0.0 && x < p.size.0 && y < p.size.1
}

/// Atualiza o realce pelo cursor (tela) e abre/fecha o submenu conforme o item.
fn hover_at(hwnd: HWND, m: &mut Menu, pt: POINT) {
    let in_sub = m.sub.as_ref().is_some_and(|(_, p)| inside(m, p, pt));
    if in_sub {
        let (idx, p) = m.sub.as_ref().map(|(i, p)| (*i, p)).unwrap();
        let (x, y) = to_dip(m, p, pt);
        let Kind::Sub(children) = &m.items[idx].kind else { return };
        let h = hit(children, &p.items_rects, x, y);
        m.focus_sub = true;
        if let Some((_, p)) = m.sub.as_mut() {
            p.hover = h;
        }
    } else if inside(m, &m.main, pt) {
        let (x, y) = to_dip(m, &m.main, pt);
        let h = hit(&m.items, &m.main.items_rects, x, y);
        m.main.hover = h;
        m.focus_sub = false;
        match h {
            Some(i) if matches!(m.items[i].kind, Kind::Sub(_)) => open_sub(hwnd, m, i),
            _ => close_sub(m),
        }
    }
    redraw(m);
}

fn paint(m: &mut Menu, sub: bool) {
    let shell = m.shell.clone();
    let gfx = shell.gfx();
    let theme = shell.theme();
    let dpi = m.dpi;
    let result = if sub {
        let Some((idx, p)) = m.sub.as_ref() else { return };
        let Kind::Sub(children) = &m.items[*idx].kind else { return };
        let Some(surface) = &p.surface else { return };
        surface.draw_custom(dpi, |ctx| paint_panel(ctx, &gfx, children, &p.items_rects, p.size, p.hover, &theme))
    } else {
        let Some(surface) = &m.main.surface else { return };
        surface.draw_custom(dpi, |ctx| paint_panel(ctx, &gfx, &m.items, &m.main.items_rects, m.main.size, m.main.hover, &theme))
    };
    if let Err(e) = result {
        if is_device_lost(&e) {
            shell.device_lost();
        }
    }
}

fn make_surface(hwnd: HWND, m: &Menu, size: (f32, f32)) -> Option<Surface> {
    unsafe {
        let gfx = m.shell.gfx();
        let s = Surface::new(&gfx, hwnd, px(size.0, m.dpi).max(1) as u32, px(size.1, m.dpi).max(1) as u32).ok()?;
        // Cantos e sombra arredondados pelo DWM (Windows 11); o painel já é desenhado com alpha.
        let round: i32 = 2; // DWMWCP_ROUND
        let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(hwnd, windows::Win32::Graphics::Dwm::DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const _ as *const core::ffi::c_void, 4);
        Some(s)
    }
}

fn key(hwnd: HWND, m: &mut Menu, vk: u16) {
    const UP: u16 = 0x26;
    const DOWN: u16 = 0x28;
    const LEFT: u16 = 0x25;
    const RIGHT: u16 = 0x27;
    const ENTER: u16 = 0x0D;
    const SPACE: u16 = 0x20;
    const ESC: u16 = 0x1B;
    match vk {
        ESC => {
            if m.focus_sub {
                close_sub(m);
                redraw(m);
            } else {
                close_all(hwnd);
            }
        }
        UP | DOWN => {
            let down = vk == DOWN;
            if m.focus_sub {
                if let Some((idx, p)) = m.sub.as_mut() {
                    if let Kind::Sub(children) = &m.items[*idx].kind {
                        p.hover = step(children, p.hover, down);
                    }
                }
            } else {
                m.main.hover = step(&m.items, m.main.hover, down);
                match m.main.hover {
                    Some(i) if matches!(m.items[i].kind, Kind::Sub(_)) => {}
                    _ => close_sub(m),
                }
            }
            redraw(m);
        }
        RIGHT => {
            if let Some(i) = m.main.hover {
                if matches!(m.items[i].kind, Kind::Sub(_)) && !m.focus_sub {
                    open_sub(hwnd, m, i);
                    m.focus_sub = true;
                    if let Some((_, p)) = m.sub.as_mut() {
                        if let Kind::Sub(children) = &m.items[i].kind {
                            p.hover = step(children, None, true);
                        }
                    }
                    redraw(m);
                }
            }
        }
        LEFT => {
            if m.focus_sub {
                close_sub(m);
                redraw(m);
            }
        }
        ENTER | SPACE => {
            let target = if m.focus_sub {
                m.sub.as_ref().and_then(|(idx, p)| match (&m.items[*idx].kind, p.hover) {
                    (Kind::Sub(c), Some(h)) => Some(c[h].cmd),
                    _ => None,
                })
            } else {
                match m.main.hover {
                    Some(i) if matches!(m.items[i].kind, Kind::Action) => Some(m.items[i].cmd),
                    Some(i) if matches!(m.items[i].kind, Kind::Sub(_)) => {
                        open_sub(hwnd, m, i);
                        m.focus_sub = true;
                        if let (Some((_, p)), Kind::Sub(c)) = (m.sub.as_mut(), &m.items[i].kind) {
                            p.hover = step(c, None, true);
                        }
                        redraw(m);
                        None
                    }
                    _ => None,
                }
            };
            if let Some(cmd) = target {
                pick(hwnd, cmd);
            }
        }
        _ => {}
    }
}

fn cursor() -> POINT {
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    pt
}

unsafe extern "system" fn menu_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CREATE => {
            if let Some(m) = menu(hwnd) {
                m.main.hwnd = hwnd;
                match make_surface(hwnd, m, m.main.size) {
                    Some(s) => m.main.surface = Some(s),
                    None => return LRESULT(-1),
                }
            }
            LRESULT(0)
        }
        WM_NCCALCSIZE if wp.0 != 0 => LRESULT(0),
        WM_NCACTIVATE => LRESULT(1),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            if let Some(m) = menu(hwnd) {
                paint(m, false);
            }
            let _ = ValidateRect(Some(hwnd), None);
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wp.0 & 0xffff) as u32 == WA_INACTIVE {
                close_all(hwnd);
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            // Outro tomou o mouse (ou o sistema o retirou): um menu sem captura não fecha ao clicar fora.
            if lp.0 != hwnd.0 as isize {
                close_all(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(m) = menu(hwnd) {
                let pt = cursor();
                if m.last_pt.replace((pt.x, pt.y)) != (pt.x, pt.y) {
                    hover_at(hwnd, m, pt);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            if let Some(m) = menu(hwnd) {
                let pt = cursor();
                let in_any = inside(m, &m.main, pt) || m.sub.as_ref().is_some_and(|(_, p)| inside(m, p, pt));
                if !in_any {
                    close_all(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let target = menu(hwnd).and_then(|m| {
                let pt = cursor();
                if let Some((idx, p)) = m.sub.as_ref() {
                    if inside(m, p, pt) {
                        let (x, y) = to_dip(m, p, pt);
                        if let Kind::Sub(c) = &m.items[*idx].kind {
                            return hit(c, &p.items_rects, x, y).map(|h| c[h].cmd);
                        }
                    }
                }
                if inside(m, &m.main, pt) {
                    let (x, y) = to_dip(m, &m.main, pt);
                    return hit(&m.items, &m.main.items_rects, x, y).filter(|&i| matches!(m.items[i].kind, Kind::Action)).map(|i| m.items[i].cmd);
                }
                None
            });
            if let Some(cmd) = target {
                pick(hwnd, cmd);
            }
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if let Some(m) = menu(hwnd) {
                key(hwnd, m, (wp.0 & 0xffff) as u16);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            OPEN.with(|o| o.set(0));
            let p = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut Menu;
            if !p.is_null() {
                drop(Box::from_raw(p));
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn sub_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CREATE => {
            if let Some(m) = menu(hwnd) {
                let size = m.sub.as_ref().map(|(_, p)| p.size);
                // O painel ainda não está em `m.sub` durante o CreateWindow: a superfície é criada no 1º paint.
                let _ = size;
            }
            LRESULT(0)
        }
        WM_NCCALCSIZE if wp.0 != 0 => LRESULT(0),
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            if let Some(m) = menu(hwnd) {
                if let Some((_, p)) = m.sub.as_ref() {
                    if p.surface.is_none() && p.hwnd == hwnd {
                        let size = p.size;
                        let s = make_surface(hwnd, m, size);
                        if let Some((_, p)) = m.sub.as_mut() {
                            p.surface = s;
                        }
                    }
                }
                paint(m, true);
            }
            let _ = ValidateRect(Some(hwnd), None);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Item> {
        vec![
            Item::action(1, "Nova nota"),
            Item::separator(),
            Item::sub("Ajustes", vec![Item::action(2, "Vidro")]),
            Item::action(3, "Sair"),
        ]
    }

    #[test]
    fn plan_stacks_rows_inside_the_panel() {
        let items = sample();
        let (rects, w, h) = plan(&items);
        assert_eq!(rects.len(), 4);
        assert!(w >= MIN_W);
        assert_eq!(rects[0].y, PAD_Y);
        assert_eq!(rects[1].h, SEP_H);
        assert!((rects[3].bottom() + PAD_Y - h).abs() < 0.01);
        for pair in rects.windows(2) {
            assert!((pair[0].bottom() - pair[1].y).abs() < 0.01);
        }
    }

    #[test]
    fn hit_ignores_separators_and_outside() {
        let items = sample();
        let (rects, _, _) = plan(&items);
        assert_eq!(hit(&items, &rects, 20.0, rects[0].y + 5.0), Some(0));
        assert_eq!(hit(&items, &rects, 20.0, rects[1].y + 5.0), None);
        assert_eq!(hit(&items, &rects, 20.0, rects[2].y + 5.0), Some(2));
        assert_eq!(hit(&items, &rects, -3.0, 10.0), None);
    }

    #[test]
    fn step_skips_separators_and_wraps() {
        let items = sample();
        assert_eq!(step(&items, None, true), Some(0));
        assert_eq!(step(&items, None, false), Some(3));
        assert_eq!(step(&items, Some(0), true), Some(2));
        assert_eq!(step(&items, Some(3), true), Some(0));
        assert_eq!(step(&items, Some(0), false), Some(3));
        assert_eq!(step(&[Item::separator()], None, true), None);
    }

    #[test]
    fn wider_labels_make_wider_panels() {
        let narrow = plan(&[Item::action(1, "Sair")]).1;
        let wide = plan(&[Item::action(1, "Modo desempenho (esconder em tela cheia)")]).1;
        assert!(wide > narrow);
    }
}
