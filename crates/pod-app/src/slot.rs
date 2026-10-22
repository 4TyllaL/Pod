//! Espaço "+" do modo de edição: um contorno tracejado do tamanho de uma célula da grade, numa posição livre.
//! Clicar escolhe o lugar do próximo widget (que a galeria da base coloca ali). Não ativa e some ao sair da edição.

use std::rc::Rc;

use windows::core::{w, Interface, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::Vector2;

use crate::gfx::{is_device_lost, Gfx, Surface};
use crate::render::{self, col};
use crate::shell::Shell;
use crate::ui::theme::{Mode, Theme};

struct State {
    shell: Rc<Shell>,
    surface: Option<Surface>,
    dpi: u32,
    size_dip: (f32, f32),
    pos: (i32, i32),
    hover: bool,
    tracking: bool,
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut State> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State).as_mut()
}

/// Cria o espaço em `pos` (canto superior esquerdo, pixels de tela) com `size_px`.
pub fn open(shell: &Rc<Shell>, pos: (i32, i32), size_px: (i32, i32), dpi: u32) -> Option<HWND> {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).ok()?.into();
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_HAND).ok()?,
            lpszClassName: w!("PodSlot"),
            ..Default::default()
        };
        let _ = RegisterClassExW(&class);
        let s = dpi as f32 / 96.0;
        let st = Box::into_raw(Box::new(State {
            shell: shell.clone(),
            surface: None,
            dpi,
            size_dip: (size_px.0 as f32 / s, size_px.1 as f32 / s),
            pos,
            hover: false,
            tracking: false,
        }));
        let created = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            w!("PodSlot"),
            w!("Espa\u{e7}o livre"),
            WS_POPUP,
            pos.0,
            pos.1,
            size_px.0,
            size_px.1,
            None,
            None,
            Some(hinst),
            Some(st as *const _),
        );
        match created {
            Ok(h) => {
                let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
                Some(h)
            }
            Err(e) => {
                crate::host::log(&format!("slot: CreateWindowExW falhou: {e}"));
                drop(Box::from_raw(st));
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

pub fn redraw(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn dashed(gfx: &Gfx) -> Result<ID2D1StrokeStyle> {
    let props = D2D1_STROKE_STYLE_PROPERTIES1 {
        startCap: D2D1_CAP_STYLE_ROUND,
        endCap: D2D1_CAP_STYLE_ROUND,
        dashCap: D2D1_CAP_STYLE_ROUND,
        lineJoin: D2D1_LINE_JOIN_ROUND,
        miterLimit: 10.0,
        dashStyle: D2D1_DASH_STYLE_DASH,
        dashOffset: 0.0,
        transformType: D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
    };
    unsafe { gfx.d2d_factory.CreateStrokeStyle(&props, None)?.cast() }
}

/// Desenha o espaço (DIP): fundo suave, contorno tracejado e o "+" central. `lit` = sob o mouse ou escolhido.
pub fn paint_slot(ctx: &ID2D1DeviceContext, gfx: &Gfx, size: (f32, f32), lit: bool, th: &Theme) -> Result<()> {
    let dark = th.mode == Mode::Dark;
    let base = if dark { [255, 255, 255] } else { [20, 24, 36] };
    let ink = if lit { th.accent(0) } else { th.text_secondary };
    unsafe {
        let r = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F { left: 2.0, top: 2.0, right: size.0 - 2.0, bottom: size.1 - 2.0 },
            radiusX: render::TILE_RADIUS_DIP,
            radiusY: render::TILE_RADIUS_DIP,
        };
        let fill = if lit { ctx.CreateSolidColorBrush(&col(th.accent(0), 0.16), None)? } else { ctx.CreateSolidColorBrush(&col(base, if dark { 0.06 } else { 0.05 }), None)? };
        ctx.FillRoundedRectangle(&r, &fill);
        let stroke = ctx.CreateSolidColorBrush(&col(ink, if lit { 0.95 } else { 0.55 }), None)?;
        let style = dashed(gfx)?;
        ctx.DrawRoundedRectangle(&r, &stroke, 2.0, &style);
        let (cx, cy, arm) = (size.0 / 2.0, size.1 / 2.0, 11.0);
        ctx.DrawLine(Vector2 { X: cx - arm, Y: cy }, Vector2 { X: cx + arm, Y: cy }, &stroke, 2.4, None);
        ctx.DrawLine(Vector2 { X: cx, Y: cy - arm }, Vector2 { X: cx, Y: cy + arm }, &stroke, 2.4, None);
    }
    Ok(())
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CREATE => {
            if let Some(s) = state(hwnd) {
                let gfx = s.shell.gfx();
                let k = s.dpi as f32 / 96.0;
                match Surface::new(&gfx, hwnd, (s.size_dip.0 * k).round() as u32, (s.size_dip.1 * k).round() as u32) {
                    Ok(surf) => s.surface = Some(surf),
                    Err(_) => return LRESULT(-1),
                }
            }
            LRESULT(0)
        }
        WM_NCCALCSIZE if wp.0 != 0 => LRESULT(0),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            if let Some(s) = state(hwnd) {
                let shell = s.shell.clone();
                let (gfx, theme) = (shell.gfx(), shell.theme());
                let lit = s.hover || shell.slot_target() == Some(s.pos);
                if let Some(surf) = &s.surface {
                    if let Err(e) = surf.draw_custom(s.dpi, |ctx| paint_slot(ctx, &gfx, s.size_dip, lit, &theme)) {
                        if is_device_lost(&e) {
                            shell.device_lost();
                        }
                    }
                }
            }
            let _ = ValidateRect(Some(hwnd), None);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(s) = state(hwnd) {
                if !s.tracking {
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    let _ = TrackMouseEvent(&mut t);
                    s.tracking = true;
                }
                if !s.hover {
                    s.hover = true;
                    redraw(hwnd);
                }
            }
            LRESULT(0)
        }
        0x02A3 /* WM_MOUSELEAVE */ => {
            if let Some(s) = state(hwnd) {
                s.tracking = false;
                s.hover = false;
                redraw(hwnd);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(s) = state(hwnd) {
                let (shell, pos) = (s.shell.clone(), s.pos);
                shell.select_slot(pos);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let p = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut State;
            if !p.is_null() {
                drop(Box::from_raw(p));
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
