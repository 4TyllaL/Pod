//! Prova S2 da sombra ampla (ADR-0005, etapa 4): a janela de sombra em camadas acompanhando um cartão que vive
//! na "camada desktop" (`HWND_BOTTOM` + hook de `WM_WINDOWPOSCHANGING`), com arrastar (captura de mouse, como
//! `host::begin_drag`), redimensionar (programático e pelo canto, com o laço modal do sistema), modo de edição
//! (`TOPMOST` e volta) e foco. **Não faz parte do produto**: só roda com `pod-app --shadow-layer <pasta>`.
//!
//! Reproduz o contrato do host: `WM_WINDOWPOSCHANGING` sem `SWP_NOZORDER` vira `HWND_BOTTOM` (salvo edição ou
//! `RAISE_OK`), `WM_MOUSEACTIVATE` devolve `MA_NOACTIVATE`, `WM_NCCALCSIZE` zera a moldura. A sombra é uma
//! janela `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE` por cartão, com o mesmo hook, e acompanha
//! posição, tamanho e ordem Z do cartão. A imagem da sombra vem de nove fatias (cantos fixos, bordas e miolo
//! repetidos), para o redimensionar não refazer o desfoque a cada passo.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::time::Instant;

use windows::core::{implement, w, ComObject, Result};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::gfx::{Gfx, Surface};
use crate::shadow_proof::{box_blur, paint, premul, pump, rounded_mask, Buf, MARGIN_DIP, RADIUS_DIP, SHADOW_ALPHA, SHADOW_DY_DIP};

static EDITING: AtomicBool = AtomicBool::new(false);
static RAISE_OK: AtomicBool = AtomicBool::new(false);
static MOVE_N: AtomicU64 = AtomicU64::new(0);
static MOVE_US: AtomicU64 = AtomicU64::new(0);
static RESIZE_N: AtomicU64 = AtomicU64::new(0);
static RESIZE_US: AtomicU64 = AtomicU64::new(0);
static RESTACK_N: AtomicU64 = AtomicU64::new(0);
static RESTACK_US: AtomicU64 = AtomicU64::new(0);
static MISMATCH: AtomicU64 = AtomicU64::new(0);
static SNAP: AtomicBool = AtomicBool::new(false);
/// Variante do cartão para o experimento de foco: 0 igual ao host; 1 sem SetCapture; 2 com WS_EX_NOACTIVATE; 3 WM_NCACTIVATE padrão; 4 ativação padrão (controle); 5 sem SetCapture e com WS_EX_NOACTIVATE.
static VARIANT: AtomicI32 = AtomicI32::new(0);
/// Verificação rígida de foco (desligada depois de um clique real, que nesta sessão deixa "nenhuma janela" com ou sem sombra).
static FG_STRICT: AtomicBool = AtomicBool::new(true);
/// Quantos cartões exigiram NOTOPMOST → TOPMOST para subir.
static RAISE_RETRIES: AtomicU64 = AtomicU64::new(0);
static NON_SNAPPED: AtomicU64 = AtomicU64::new(0);
/// -1 = ler o sistema; 0 = forçar sombra desligada; 1 = forçar ligada (testes da política).
static POLICY_OVERRIDE: AtomicI32 = AtomicI32::new(-1);
/// 0 = provedor padrão do HWND; 1 = provedor que não é elemento de controle nem de conteúdo; 2 = sem provedor.
static UIA_MODE: AtomicI32 = AtomicI32::new(0);

struct Slicer {
    base: Buf,
    cut: usize,
}

struct CardState {
    hwnd: isize,
    shadow: isize,
    kind: u8, // 0 = cartão de formato fixo (arrasta pela captura), 1 = nota (borda redimensiona)
    cw: i32,
    ch: i32,
    /// DPI do cartão (monitor onde está) e DPI com que a imagem da sombra foi montada.
    dpi: u32,
    sdpi: u32,
    drag: Option<(POINT, POINT)>,
}

thread_local! {
    static CARDS: RefCell<Vec<CardState>> = const { RefCell::new(Vec::new()) };
    static SURFACES: RefCell<HashMap<isize, Surface>> = RefCell::new(HashMap::new());
    static SLICERS: RefCell<HashMap<u32, Rc<Slicer>>> = RefCell::new(HashMap::new());
    static CLICKS: RefCell<Vec<(isize, i32, i32)>> = const { RefCell::new(Vec::new()) };
    static LIVE_LOG: RefCell<Option<std::path::PathBuf>> = const { RefCell::new(None) };
}

fn hw(v: isize) -> HWND {
    HWND(v as *mut core::ffi::c_void)
}

fn scale_of(dpi: u32) -> f32 {
    dpi as f32 / 96.0
}

/// Margem da sombra em pixels físicos no DPI do monitor do cartão.
pub(crate) fn margin_px_for(dpi: u32) -> i32 {
    (MARGIN_DIP * scale_of(dpi)).round() as i32
}

fn slicer_for(dpi: u32) -> Rc<Slicer> {
    SLICERS.with(|m| m.borrow_mut().entry(dpi).or_insert_with(|| Rc::new(make_slicer(scale_of(dpi)))).clone())
}

// ---------------------------------------------------------------------------------------------
// Imagens
// ---------------------------------------------------------------------------------------------

/// Sombra completa (referência): máscara do cartão deslocada e borrada em três passes de caixa.
pub(crate) fn shadow_full(cw: usize, ch: usize, scale: f32) -> Buf {
    let m = (MARGIN_DIP * scale).round();
    let (r, dy) = (RADIUS_DIP * scale, (SHADOW_DY_DIP * scale).round());
    let (tw, th) = (cw + 2 * m as usize, ch + 2 * m as usize);
    let mask = rounded_mask(tw, th, m, m + dy, cw as f32, ch as f32, r);
    let rad = ((MARGIN_DIP * scale) / 3.0).round().max(1.0) as usize;
    let mut b = mask;
    for _ in 0..3 {
        b = box_blur(&b, tw, th, rad);
    }
    Buf { w: tw, h: th, px: b.iter().map(|a| premul(a * SHADOW_ALPHA, [0.0; 3])).collect() }
}

fn make_slicer(scale: f32) -> Slicer {
    let m = (MARGIN_DIP * scale).round() as usize;
    let cut = m + (40.0 * scale).round() as usize;
    let cs0 = 2 * (cut - m) + 8;
    Slicer { base: shadow_full(cs0, cs0, scale), cut }
}

/// Sombra por nove fatias: cantos e bordas copiados da base, miolo repetido.
pub(crate) fn nine_slice(base: &Buf, cut: usize, cw: usize, ch: usize, scale: f32) -> Buf {
    let m = (MARGIN_DIP * scale).round() as usize;
    let (tw, th) = (cw + 2 * m, ch + 2 * m);
    if tw < 2 * cut + 8 || th < 2 * cut + 8 {
        return shadow_full(cw, ch, scale);
    }
    let (bw, bh) = (base.w, base.h);
    let mx = |x: usize| if x < cut { x } else if x >= tw - cut { bw - (tw - x) } else { cut };
    let my = |y: usize| if y < cut { y } else if y >= th - cut { bh - (th - y) } else { cut };
    let mut px = vec![0u32; tw * th];
    for y in 0..th {
        let sy = my(y) * bw;
        for x in 0..tw {
            px[y * tw + x] = base.px[sy + mx(x)];
        }
    }
    Buf { w: tw, h: th, px }
}

pub(crate) fn card_buf(cw: usize, ch: usize, scale: f32) -> Buf {
    let m = rounded_mask(cw, ch, 0.0, 0.0, cw as f32, ch as f32, RADIUS_DIP * scale);
    Buf { w: cw, h: ch, px: m.iter().map(|a| premul(*a, [255.0, 255.0, 255.0])).collect() }
}

/// Atualiza a imagem e a posição de uma janela em camadas.
unsafe fn ulw(hwnd: HWND, buf: &Buf, x: i32, y: i32) -> Result<()> {
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(Some(screen));
    let bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: buf.w as i32,
            biHeight: -(buf.h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let dib = CreateDIBSection(Some(mem), &bi, DIB_RGB_COLORS, &mut bits, None, 0)?;
    std::ptr::copy_nonoverlapping(buf.px.as_ptr(), bits as *mut u32, buf.px.len());
    let old = SelectObject(mem, dib.into());
    let size = SIZE { cx: buf.w as i32, cy: buf.h as i32 };
    let dst = POINT { x, y };
    let src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
    let r = UpdateLayeredWindow(hwnd, Some(screen), Some(&dst), Some(&size), Some(mem), Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA);
    SelectObject(mem, old);
    let _ = DeleteObject(dib.into());
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
    r
}

// ---------------------------------------------------------------------------------------------
// Ordem Z
// ---------------------------------------------------------------------------------------------

unsafe fn z_rank(h: HWND) -> usize {
    let mut cur = GetTopWindow(None).unwrap_or_default();
    let mut i = 0;
    while !cur.0.is_null() {
        if cur == h {
            return i;
        }
        cur = GetWindow(cur, GW_HWNDNEXT).unwrap_or_default();
        i += 1;
    }
    usize::MAX
}

/// Sombras logo abaixo de todos os cartões: no fundo (modo normal) ou logo abaixo do cartão mais baixo (edição).
unsafe fn restack() {
    let t0 = Instant::now();
    let pairs: Vec<(isize, isize)> = CARDS.with(|c| c.borrow().iter().map(|x| (x.hwnd, x.shadow)).collect());
    RAISE_OK.store(true, Ordering::Relaxed);
    if !EDITING.load(Ordering::Relaxed) {
        for (_, s) in &pairs {
            let _ = SetWindowPos(hw(*s), Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    } else if let Some(low) = pairs.iter().map(|(c, _)| *c).max_by_key(|c| z_rank(hw(*c))) {
        for (_, s) in &pairs {
            let _ = SetWindowPos(hw(*s), Some(hw(low)), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
    RAISE_OK.store(false, Ordering::Relaxed);
    RESTACK_N.fetch_add(1, Ordering::Relaxed);
    RESTACK_US.fetch_add(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
}

/// A sombra segue o cartão: posição (rápido) ou tamanho (imagem nova por nove fatias).
unsafe fn follow(hwnd: HWND) {
    let t0 = Instant::now();
    let mut r = RECT::default();
    if GetWindowRect(hwnd, &mut r).is_err() {
        return;
    }
    let (cw, ch) = (r.right - r.left, r.bottom - r.top);
    let key = hwnd.0 as isize;
    let Some((shadow, ocw, och, dpi, sdpi, kind)) = CARDS.with(|c| c.borrow().iter().find(|x| x.hwnd == key).map(|x| (hw(x.shadow), x.cw, x.ch, x.dpi, x.sdpi, x.kind))) else { return };
    if !IsWindowVisible(shadow).as_bool() {
        return;
    }
    let m = margin_px_for(dpi);
    let scale = scale_of(dpi);
    if SNAP.load(Ordering::Relaxed) && kind == 0 {
        let mon = MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut mi).as_bool() && !(pod_model::grid::is_snapped(r.left, mi.rcWork.left, dpi) && pod_model::grid::is_snapped(r.top, mi.rcWork.top, dpi)) {
            NON_SNAPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
    if (cw, ch) != (ocw, och) || dpi != sdpi {
        SURFACES.with(|s| {
            if let Some(sf) = s.borrow_mut().get_mut(&key) {
                let _ = sf.resize(cw as u32, ch as u32);
                let _ = paint(sf, &card_buf(cw as usize, ch as usize, scale), None);
            }
        });
        let sl = slicer_for(dpi);
        let img = nine_slice(&sl.base, sl.cut, cw as usize, ch as usize, scale);
        let _ = SetWindowPos(shadow, None, r.left - m, r.top - m, img.w as i32, img.h as i32, SWP_NOZORDER | SWP_NOACTIVATE);
        let _ = ulw(shadow, &img, r.left - m, r.top - m);
        CARDS.with(|c| {
            if let Some(x) = c.borrow_mut().iter_mut().find(|x| x.hwnd == key) {
                x.cw = cw;
                x.ch = ch;
                x.sdpi = dpi;
            }
        });
        RESIZE_N.fetch_add(1, Ordering::Relaxed);
        RESIZE_US.fetch_add(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
    } else {
        let _ = SetWindowPos(shadow, None, r.left - m, r.top - m, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        MOVE_N.fetch_add(1, Ordering::Relaxed);
        MOVE_US.fetch_add(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
    }
    let mut sr = RECT::default();
    let _ = GetWindowRect(shadow, &mut sr);
    if (sr.left, sr.top, sr.right, sr.bottom) != (r.left - m, r.top - m, r.right + m, r.bottom + m) {
        MISMATCH.fetch_add(1, Ordering::Relaxed);
    }
}

fn kind_of(hwnd: HWND) -> u8 {
    CARDS.with(|c| c.borrow().iter().find(|x| x.hwnd == hwnd.0 as isize).map(|x| x.kind).unwrap_or(0))
}

unsafe extern "system" fn card_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCALCSIZE if wp.0 != 0 => LRESULT(0),
        WM_NCACTIVATE if VARIANT.load(Ordering::Relaxed) != 3 => LRESULT(1),
        WM_MOUSEACTIVATE if VARIANT.load(Ordering::Relaxed) != 4 => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => {
            if kind_of(hwnd) == 1 {
                let mut r = RECT::default();
                let _ = GetWindowRect(hwnd, &mut r);
                let (x, y) = ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32);
                if x >= r.right - 12 && y >= r.bottom - 12 {
                    return LRESULT(HTBOTTOMRIGHT as isize);
                }
                if y < r.top + 24 {
                    return LRESULT(HTCAPTION as isize);
                }
            }
            LRESULT(HTCLIENT as isize)
        }
        // Como no host: arrastar pela legenda sem ativar (o tratamento padrão ativaria e levantaria o cartão).
        WM_NCLBUTTONDOWN if wp.0 as u32 == HTCAPTION => DefWindowProcW(hwnd, WM_SYSCOMMAND, WPARAM(0xF012), lp),
        WM_LBUTTONDOWN if kind_of(hwnd) == 0 => {
            CLICKS.with(|c| c.borrow_mut().push((hwnd.0 as isize, (lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)));
            let (mut cursor, mut r) = (POINT::default(), RECT::default());
            if GetCursorPos(&mut cursor).is_ok() && GetWindowRect(hwnd, &mut r).is_ok() {
                CARDS.with(|c| {
                    if let Some(x) = c.borrow_mut().iter_mut().find(|x| x.hwnd == hwnd.0 as isize) {
                        x.drag = Some((cursor, POINT { x: r.left, y: r.top }));
                    }
                });
                if !matches!(VARIANT.load(Ordering::Relaxed), 1 | 5) {
                    SetCapture(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let drag = CARDS.with(|c| c.borrow().iter().find(|x| x.hwnd == hwnd.0 as isize).and_then(|x| x.drag));
            if let Some((c0, o)) = drag {
                let mut c1 = POINT::default();
                let _ = GetCursorPos(&mut c1);
                let (mut nx, mut ny) = (o.x + c1.x - c0.x, o.y + c1.y - c0.y);
                if SNAP.load(Ordering::Relaxed) {
                    // Mesmo encaixe de `host::snap_tile_rect`: grade do monitor e dentro da área útil.
                    let mut wr = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut wr);
                    let (w, h) = (wr.right - wr.left, wr.bottom - wr.top);
                    let mut r = RECT { left: nx, top: ny, right: nx + w, bottom: ny + h };
                    let dpi = CARDS.with(|c| c.borrow().iter().find(|x| x.hwnd == hwnd.0 as isize).map_or(96, |x| x.dpi));
                    snap_tile_rect(&mut r, dpi);
                    (nx, ny) = (r.left, r.top);
                }
                let _ = SetWindowPos(hwnd, None, nx, ny, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            CARDS.with(|c| {
                if let Some(x) = c.borrow_mut().iter_mut().find(|x| x.hwnd == hwnd.0 as isize) {
                    x.drag = None;
                }
            });
            let _ = ReleaseCapture();
            LRESULT(0)
        }
        // Qualquer pedido de mudança de ordem Z vira "para o fundo" (salvo edição ou RAISE_OK), como no host.
        WM_WINDOWPOSCHANGING => {
            let pos = &mut *(lp.0 as *mut WINDOWPOS);
            if pos.flags.0 & SWP_NOZORDER.0 == 0 && !RAISE_OK.load(Ordering::Relaxed) && !EDITING.load(Ordering::Relaxed) {
                pos.hwndInsertAfter = HWND_BOTTOM;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_WINDOWPOSCHANGED => {
            let pos = &*(lp.0 as *const WINDOWPOS);
            if pos.flags.0 & SWP_NOMOVE.0 == 0 || pos.flags.0 & SWP_NOSIZE.0 == 0 {
                follow(hwnd);
            }
            if pos.flags.0 & SWP_NOZORDER.0 == 0 {
                restack();
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_SYSCOLORCHANGE => {
            apply_policy();
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_SHOWWINDOW if wp.0 != 0 => {
            if !EDITING.load(Ordering::Relaxed) {
                let _ = SetWindowPos(hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// A janela de sombra tem o mesmo hook: ninguém a levanta, só o nosso `restack`.
/// Provedor mínimo para a janela de sombra: não é elemento de controle nem de conteúdo (leitores de tela
/// que usam a visão de controle não a anunciam). A janela continua existindo na visão bruta.
#[implement(IRawElementProviderSimple)]
struct HiddenProvider {
    hwnd: HWND,
}

impl IRawElementProviderSimple_Impl for HiddenProvider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions(ProviderOptions_ServerSideProvider.0 | ProviderOptions_UseComThreading.0))
    }
    fn GetPatternProvider(&self, _: UIA_PATTERN_ID) -> Result<windows::core::IUnknown> {
        Err(windows::core::Error::empty())
    }
    fn GetPropertyValue(&self, id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        Ok(match id.0 {
            30016 | 30017 => VARIANT::from(false), // IsControlElement, IsContentElement
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(self.hwnd) }
    }
}

unsafe extern "system" fn shadow_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_GETOBJECT if lp.0 as i32 == UiaRootObjectId => match UIA_MODE.load(Ordering::Relaxed) {
            1 => {
                let p: IRawElementProviderSimple = ComObject::new(HiddenProvider { hwnd }).to_interface();
                UiaReturnRawElementProvider(hwnd, wp, lp, &p)
            }
            2 => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        },
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_WINDOWPOSCHANGING => {
            let pos = &mut *(lp.0 as *mut WINDOWPOS);
            if pos.flags.0 & SWP_NOZORDER.0 == 0 && !RAISE_OK.load(Ordering::Relaxed) && !EDITING.load(Ordering::Relaxed) {
                pos.hwndInsertAfter = HWND_BOTTOM;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn probe_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe fn register(name: windows::core::PCWSTR, proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT) {
    let hinst = GetModuleHandleW(None).unwrap().into();
    let class = WNDCLASSEXW { cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32, lpfnWndProc: Some(proc), hInstance: hinst, lpszClassName: name, ..Default::default() };
    RegisterClassExW(&class);
}

unsafe fn make_card(gfx: &Gfx, x: i32, y: i32, cw: i32, ch: i32, kind: u8, dpi: u32) -> Result<HWND> {
    let scale = scale_of(dpi);
    let hinst = GetModuleHandleW(None)?.into();
    let extra = if matches!(VARIANT.load(Ordering::Relaxed), 2 | 5) { WS_EX_NOACTIVATE } else { WINDOW_EX_STYLE(0) };
    let hwnd = CreateWindowExW(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | extra, w!("PodLayerCard"), w!("pod-layer-card"), WS_POPUP | WS_THICKFRAME, x, y, cw, ch, None, None, Some(hinst), None)?;
    let surface = Surface::new(gfx, hwnd, cw as u32, ch as u32)?;
    paint(&surface, &card_buf(cw as usize, ch as usize, scale), None)?;
    SURFACES.with(|s| s.borrow_mut().insert(hwnd.0 as isize, surface));
    let m = margin_px_for(dpi);
    let sh = CreateWindowExW(WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW, w!("PodLayerShadow"), w!("pod-layer-shadow"), WS_POPUP, x - m, y - m, cw + 2 * m, ch + 2 * m, None, None, Some(hinst), None)?;
    let sl = slicer_for(dpi);
    let img = nine_slice(&sl.base, sl.cut, cw as usize, ch as usize, scale);
    ulw(sh, &img, x - m, y - m)?;
    CARDS.with(|c| c.borrow_mut().push(CardState { hwnd: hwnd.0 as isize, shadow: sh.0 as isize, kind, cw, ch, dpi, sdpi: dpi, drag: None }));
    let _ = ShowWindow(sh, SW_SHOWNOACTIVATE);
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    restack();
    Ok(hwnd)
}

// ---------------------------------------------------------------------------------------------
// Entrada real (SendInput numa thread; a principal bombeia mensagens)
// ---------------------------------------------------------------------------------------------

/// Pressiona em `from`, move em `steps` passos até `to` e solta. Restaura o cursor ao fim.
fn input_thread(from: (i32, i32), to: (i32, i32), steps: u32) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || unsafe {
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let mut old = POINT::default();
        let _ = GetCursorPos(&mut old);
        let abs = |v: i32, max: i32| (v as i64 * 65535 / (max - 1) as i64) as i32;
        let ev = |x: i32, y: i32, flags: MOUSE_EVENT_FLAGS| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: abs(x, sw), dy: abs(y, sh), mouseData: 0, dwFlags: flags | MOUSEEVENTF_ABSOLUTE, time: 0, dwExtraInfo: 0 } },
        };
        let send = |i: INPUT| {
            SendInput(&[i], std::mem::size_of::<INPUT>() as i32);
        };
        send(ev(from.0, from.1, MOUSEEVENTF_MOVE));
        std::thread::sleep(std::time::Duration::from_millis(120));
        send(ev(from.0, from.1, MOUSEEVENTF_LEFTDOWN));
        std::thread::sleep(std::time::Duration::from_millis(120));
        for i in 1..=steps {
            let (x, y) = (from.0 + (to.0 - from.0) * i as i32 / steps as i32, from.1 + (to.1 - from.1) * i as i32 / steps as i32);
            send(ev(x, y, MOUSEEVENTF_MOVE));
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
        send(ev(to.0, to.1, MOUSEEVENTF_LEFTUP));
        std::thread::sleep(std::time::Duration::from_millis(150));
        let _ = SetCursorPos(old.x, old.y);
    })
}

/// Bombeia mensagens até a thread de entrada terminar.
unsafe fn pump_until(h: std::thread::JoinHandle<()>) {
    while !h.is_finished() {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let _ = h.join();
    pump();
}

fn rect_of(h: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(h, &mut r);
    }
    r
}

/// Invariantes de ordem Z, geometria e foco.
unsafe fn invariants(probe: HWND, fg0: HWND, edit: bool) -> Vec<(String, bool)> {
    let all: Vec<(isize, isize, u32)> = CARDS.with(|c| c.borrow().iter().map(|x| (x.hwnd, x.shadow, x.dpi)).collect());
    let pairs: Vec<(isize, isize)> = all.iter().map(|(c, s, _)| (*c, *s)).collect();
    let crank: Vec<usize> = pairs.iter().map(|(c, _)| z_rank(hw(*c))).collect();
    let srank: Vec<usize> = pairs.iter().map(|(_, s)| z_rank(hw(*s))).collect();
    let prank = z_rank(probe);
    let mut v = Vec::new();
    v.push(("todas as sombras abaixo de todos os cartões".into(), srank.iter().min() > crank.iter().max()));
    let tops: Vec<String> = pairs.iter().zip(&crank).map(|((c, _), r)| format!("{}{}", if GetWindowLongPtrW(hw(*c), GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0 { "T" } else { "n" }, r)).collect();
    let zinfo = format!("posição Z (0 = topo): janela normal {prank}; cartões [{}] (T = TOPMOST); sombras {}–{}", tops.join(" "), srank.iter().min().unwrap(), srank.iter().max().unwrap());
    if edit {
        let ok = *srank.iter().max().unwrap() < prank;
        v.push((if ok { "cartões e sombras ACIMA da janela normal (modo de edição)".to_string() } else { format!("cartões e sombras ACIMA da janela normal (modo de edição) [{zinfo}]") }, ok));
    } else {
        let ok = *crank.iter().min().unwrap() > prank;
        v.push((if ok { "cartões e sombras ABAIXO da janela normal".to_string() } else { format!("cartões e sombras ABAIXO da janela normal [{zinfo}]") }, ok));
    }
    let geo = all.iter().all(|(c, s, dpi)| {
        let m = margin_px_for(*dpi);
        let (cr, sr) = (rect_of(hw(*c)), rect_of(hw(*s)));
        (sr.left, sr.top, sr.right, sr.bottom) == (cr.left - m, cr.top - m, cr.right + m, cr.bottom + m)
    });
    v.push(("retângulo da sombra = cartão ± margem, em todos".into(), geo));
    let fg_now = GetForegroundWindow();
    if !FG_STRICT.load(Ordering::Relaxed) {
        v.push((format!("(informativo) primeiro plano agora: {}", crate::shadow_proof::describe(fg_now)), true));
    } else {
        v.push((if fg_now == fg0 { "primeiro plano não mudou".to_string() } else { format!("primeiro plano não mudou (era {}, agora {})", crate::shadow_proof::describe(fg0), crate::shadow_proof::describe(fg_now)) }, fg_now == fg0));
    }
    v
}

pub fn run(dir: &Path) -> std::result::Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    unsafe { inner(dir).map_err(|e| e.to_string()) }
}

unsafe fn inner(dir: &Path) -> Result<()> {
    register(w!("PodLayerCard"), card_proc);
    register(w!("PodLayerShadow"), shadow_proc);
    register(w!("PodLayerProbe"), probe_proc);
    let fg_start = GetForegroundWindow();
    let dpi = GetDpiForSystem();
    let scale = scale_of(dpi);
    let gfx = Gfx::new(false)?;
    let tile = (88.0 * scale).round() as i32;
    let step = tile + (12.0 * scale).round() as i32;
    let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let (x0, y0) = (sw / 2 - 2 * step, sh / 2 - step);
    let mut rep = String::new();
    let _ = writeln!(rep, "# Prova S2: sombra em camadas na camada desktop (ADR-0005, etapa 4)\n");
    let _ = writeln!(rep, "Escala {scale:.2}; cartão {tile} px; 6 cartões de formato fixo (grade 3×2) + 1 nota (200×140 DIP); sombras em camadas com margem de {} px.\n", margin_px_for(dpi));

    // Cartões: 6 de formato fixo e 1 nota à direita.
    let mut tiles = Vec::new();
    for i in 0..6 {
        tiles.push(make_card(&gfx, x0 + (i % 3) * step, y0 + (i / 3) * step, tile, tile, 0, dpi)?);
    }
    let note = make_card(&gfx, x0 + 3 * step + 24, y0, (200.0 * scale) as i32, (140.0 * scale) as i32, 1, dpi)?;
    // Janela "de aplicativo" (normal, não topmost) sobre a parte de baixo dos cartões do meio.
    let hinst = GetModuleHandleW(None)?.into();
    let probe = CreateWindowExW(WINDOW_EX_STYLE(0), w!("PodLayerProbe"), w!("pod-layer-probe"), WS_OVERLAPPEDWINDOW, x0 + step - 20, y0 + step + 30, 3 * tile, tile, None, None, Some(hinst), None)?;
    let _ = ShowWindow(probe, SW_SHOWNOACTIVATE);
    pump();
    let fg0 = fg_start;

    let mut all_ok = true;
    let mut check = |rep: &mut String, step: &str, items: Vec<(String, bool)>, ok_all: &mut bool| {
        let _ = writeln!(rep, "### {step}\n");
        let _ = writeln!(rep, "| Verificação | OK |");
        let _ = writeln!(rep, "| --- | --- |");
        for (name, ok) in items {
            *ok_all &= ok;
            let _ = writeln!(rep, "| {name} | {} |", if ok { "sim" } else { "**NÃO**" });
        }
        let _ = writeln!(rep, "
_Primeiro plano ao fim deste passo: {}._", crate::shadow_proof::describe(GetForegroundWindow()));
        let _ = writeln!(rep);
    };

    check(&mut rep, "1. Depois de criar (cartões no fundo, janela normal por cima; o primeiro plano é o de antes de criar qualquer janela)", invariants(probe, fg0, false), &mut all_ok);

    // 2. Tentativas de levantar (cartão e sombra) sem RAISE_OK: o hook reescreve para o fundo.
    //    `HWND_TOP` com NOACTIVATE testa só a ordem Z; `BringWindowToTop` também ativa (como no host, o hook
    //    corrige a ordem mas não impede a ativação), então o foco é tratado à parte.
    let _ = SetWindowPos(tiles[0], Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    let sh0 = hw(CARDS.with(|c| c.borrow()[0].shadow));
    let _ = SetWindowPos(sh0, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    pump();
    check(&mut rep, "2a. Tentativas de levantar cartão e sombra (HWND_TOP sem ativar)", invariants(probe, fg0, false), &mut all_ok);
    let _ = BringWindowToTop(tiles[1]);
    pump();
    let fg_changed = GetForegroundWindow() != fg0;
    let mut v = invariants(probe, fg0, false);
    v.retain(|(n, _)| !n.contains("primeiro plano"));
    check(&mut rep, "2b. BringWindowToTop no cartão (ativa a janela; o hook só corrige a ordem)", v, &mut all_ok);
    let _ = writeln!(rep, "Informativo: `BringWindowToTop` {} o primeiro plano ({}). Isso é o comportamento do host (o hook reescreve `hwndInsertAfter`, não bloqueia a ativação) e não depende da sombra; o foco foi devolvido para o resto da prova.\n", if fg_changed { "mudou" } else { "não mudou" }, crate::shadow_proof::describe(GetForegroundWindow()));
    if fg_changed {
        let _ = SetForegroundWindow(fg0);
        pump();
    }
    let fg0 = GetForegroundWindow();

    // 3. Clique real no cartão (se estiver visível).
    let t0 = rect_of(tiles[0]);
    let (cx, cy) = ((t0.left + t0.right) / 2, (t0.top + t0.bottom) / 2);
    let visible = |h: HWND, x: i32, y: i32| WindowFromPoint(POINT { x, y }) == h;
    // Se outra janela (por exemplo o navegador) cobre os cartões, a entrada real só alcança os cartões em modo de
    // edição (TOPMOST); os passos 3, 4 e 6 rodam assim e a ordem Z do modo normal já foi verificada nos passos 1 e 2.
    let covered = !visible(tiles[0], cx, cy);
    let cover_desc = crate::shadow_proof::describe(WindowFromPoint(POINT { x: cx, y: cy }));
    let cards_all: Vec<isize> = CARDS.with(|c| c.borrow().iter().map(|x| x.hwnd).collect());
    if covered {
        EDITING.store(true, Ordering::Relaxed);
        for c in &cards_all {
            let _ = SetWindowPos(hw(*c), Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        restack();
        pump();
        let _ = writeln!(rep, "> Os cartões estavam cobertos por outra janela ({}); os passos 3, 4 e 6 abaixo rodam em **modo de edição (TOPMOST)** para a entrada real alcançá-los.\n", cover_desc);
    }
    if visible(tiles[0], cx, cy) {
        pump_until(input_thread((cx, cy), (cx, cy), 1));
        let fg_after_click = GetForegroundWindow();
        let mut v3 = invariants(probe, fg0, covered);
        // O clique real num cartão (MA_NOACTIVATE, como no host) pode deixar "nenhuma janela" em primeiro plano quando a
        // janela em foco é de outro processo. Controle: o mesmo clique SEM sombra decide se isso vem da sombra.
        v3.retain(|(n, _)| !n.contains("primeiro plano"));
        check(&mut rep, "3. Clique real no cartão (SendInput)", v3, &mut all_ok);
        let _ = SetForegroundWindow(fg0);
        pump();
        {
        POLICY_OVERRIDE.store(0, Ordering::Relaxed);
        apply_policy();
        pump();
        pump_until(input_thread((cx, cy), (cx, cy), 1));
        let fg_after_click_no_shadow = GetForegroundWindow();
        POLICY_OVERRIDE.store(-1, Ordering::Relaxed);
        apply_policy();
        pump();
        let _ = writeln!(rep, "Controle do foco depois de um clique real num cartão (primeiro plano antes: {}): **com sombra** → {}; **sem sombra** (sombras escondidas) → {}. {}
", crate::shadow_proof::describe(fg0), crate::shadow_proof::describe(fg_after_click), crate::shadow_proof::describe(fg_after_click_no_shadow), if fg_after_click == fg_after_click_no_shadow { "O resultado é o mesmo com e sem sombra: **não vem da sombra**; é o comportamento dos cartões com `MA_NOACTIVATE` nesta sessão." } else { "**Diferente com e sem sombra: investigar.**" });
        let _ = SetForegroundWindow(fg0);
        pump();
        if fg_after_click == fg_after_click_no_shadow {
            FG_STRICT.store(false, Ordering::Relaxed);
        }
        }
    } else {
        let _ = writeln!(rep, "### 3. Clique real no cartão\n\n**pulado**: o cartão está coberto por outra janela em ({cx},{cy}) ({}).\n", crate::shadow_proof::describe(WindowFromPoint(POINT { x: cx, y: cy })));
    }

    // 4. Arrastar de verdade (captura de mouse, como o host).
    let before = rect_of(tiles[0]);
    let (n0, mm0) = (MOVE_N.load(Ordering::Relaxed), MISMATCH.load(Ordering::Relaxed));
    let (dx, dy) = (140, 90);
    let moved_real;
    if visible(tiles[0], cx, cy) {
        pump_until(input_thread((cx, cy), (cx + dx, cy + dy), 12));
        let after = rect_of(tiles[0]);
        moved_real = true;
        let items = {
            let mut v = invariants(probe, fg0, covered);
            v.push((format!("cartão moveu exatamente ({dx},{dy}) px"), (after.left - before.left, after.top - before.top) == (dx, dy)));
            v.push((format!("sombra acompanhou em cada passo ({} atualizações, 0 divergências)", MOVE_N.load(Ordering::Relaxed) - n0), MISMATCH.load(Ordering::Relaxed) == mm0 && MOVE_N.load(Ordering::Relaxed) > n0));
            v
        };
        check(&mut rep, "4. Arrastar de verdade (SendInput, captura de mouse)", items, &mut all_ok);
    } else {
        moved_real = false;
        let _ = writeln!(rep, "### 4. Arrastar de verdade\n\n**pulado**: cartão coberto por outra janela; usando movimento programático (SetWindowPos) no lugar.\n");
        for i in 1..=12 {
            let _ = SetWindowPos(tiles[0], None, before.left + dx * i / 12, before.top + dy * i / 12, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            pump();
        }
        check(&mut rep, "4b. Arrastar programático", invariants(probe, fg0, false), &mut all_ok);
    }

    // 5. Redimensionar por mudança de formato (programático): 88x88 -> 188x88 -> 188x188 -> 88x88.
    let mut items = Vec::new();
    for (cw, ch) in [(188.0, 88.0), (188.0, 188.0), (88.0, 88.0)] {
        let (pw, ph) = ((cw * scale).round() as i32, (ch * scale).round() as i32);
        let r = rect_of(tiles[5]);
        let _ = SetWindowPos(tiles[5], None, r.left, r.top, pw, ph, SWP_NOZORDER | SWP_NOACTIVATE);
        pump();
        let rr = rect_of(tiles[5]);
        items.push((format!("formato {cw}×{ch} DIP: cartão {pw}×{ph} px"), (rr.right - rr.left, rr.bottom - rr.top) == (pw, ph)));
    }
    items.extend(invariants(probe, fg0, covered));
    check(&mut rep, "5. Mudança de formato (SetWindowPos de tamanho)", items, &mut all_ok);

    // 6. Redimensionar a nota pelo canto (laço modal do sistema, com SendInput).
    let nr0 = rect_of(note);
    let (rx, ry) = (nr0.right - 5, nr0.bottom - 5);
    if visible(note, rx, ry) {
        let (r0, mm0) = (RESIZE_N.load(Ordering::Relaxed), MISMATCH.load(Ordering::Relaxed));
        pump_until(input_thread((rx, ry), (rx + 90, ry + 60), 14));
        let nr1 = rect_of(note);
        let mut v = invariants(probe, fg0, covered);
        v.push(("nota cresceu pelo canto (laço modal do sistema)".into(), nr1.right - nr1.left > nr0.right - nr0.left && nr1.bottom - nr1.top > nr0.bottom - nr0.top));
        v.push((format!("sombra acompanhou o tamanho em cada passo ({} imagens novas, 0 divergências)", RESIZE_N.load(Ordering::Relaxed) - r0), MISMATCH.load(Ordering::Relaxed) == mm0 && RESIZE_N.load(Ordering::Relaxed) > r0));
        let fg_changed = GetForegroundWindow() != fg0;
        v.retain(|(n, _)| !n.contains("primeiro plano"));
        check(&mut rep, "6. Redimensionar a nota pelo canto (SendInput)", v, &mut all_ok);
        let _ = writeln!(rep, "Informativo: o laço modal de redimensionamento do sistema {} o primeiro plano ({}). Como no host, notas ativam (são para digitar) e o hook só corrige a ordem Z; cartões de formato fixo não redimensionam por borda. O foco foi devolvido para o resto da prova.\n", if fg_changed { "mudou" } else { "não mudou" }, crate::shadow_proof::describe(GetForegroundWindow()));
        if fg_changed {
            let _ = SetForegroundWindow(fg0);
            pump();
        }
    } else {
        let _ = writeln!(rep, "### 6. Redimensionar a nota pelo canto\n\n**pulado**: canto da nota coberto por outra janela ({}).\n", crate::shadow_proof::describe(WindowFromPoint(POINT { x: rx, y: ry })));
    }

    let fg0 = GetForegroundWindow();
    if covered {
        EDITING.store(false, Ordering::Relaxed);
        for c in &cards_all {
            let _ = SetWindowPos(hw(*c), Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        restack();
        pump();
        check(&mut rep, "6b. Depois da entrada real: modo de edição desligado, tudo de volta ao fundo", invariants(probe, fg0, false), &mut all_ok);
    }
    // 7. Modo de edição: tudo sobe (TOPMOST) e volta ao fundo.
    EDITING.store(true, Ordering::Relaxed);
    let cards: Vec<isize> = CARDS.with(|c| c.borrow().iter().map(|x| x.hwnd).collect());
    RAISE_RETRIES.store(0, Ordering::Relaxed);
    for c in &cards {
        raise_topmost(hw(*c));
    }
    restack();
    pump();
    let retries = RAISE_RETRIES.load(Ordering::Relaxed);
    if retries > 0 {
        let _ = writeln!(rep, "> **Achado (não é da sombra):** {retries} cartão(ões) não ficaram `TOPMOST` com `SetWindowPos(HWND_TOPMOST)` (a chamada devolveu sucesso, o estilo não mudou) e só subiram depois de `HWND_NOTOPMOST` + `HWND_TOPMOST`. Aconteceu com a nota **depois do redimensionamento real pelo laço modal do sistema** (passo 6); sem esse redimensionamento todos subiram direto. Vale conferir `host::set_edit_layer` no produto.\n");
    }
    check(&mut rep, "7a. Modo de edição ligado (cartões TOPMOST, sombras logo abaixo)", invariants(probe, fg0, true), &mut all_ok);
    EDITING.store(false, Ordering::Relaxed);
    for c in &cards {
        let _ = SetWindowPos(hw(*c), Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    restack();
    pump();
    check(&mut rep, "7b. Modo de edição desligado (tudo de volta ao fundo)", invariants(probe, fg0, false), &mut all_ok);

    // Custos medidos.
    let avg = |us: &AtomicU64, n: &AtomicU64| us.load(Ordering::Relaxed) as f64 / n.load(Ordering::Relaxed).max(1) as f64;
    let _ = writeln!(rep, "### Custo do acompanhamento (build de debug)\n");
    let _ = writeln!(rep, "| Evento | Ocorrências | Tempo médio |");
    let _ = writeln!(rep, "| --- | --- | --- |");
    let _ = writeln!(rep, "| mover (sombra acompanha, `SetWindowPos`) | {} | {:.0} µs |", MOVE_N.load(Ordering::Relaxed), avg(&MOVE_US, &MOVE_N));
    let _ = writeln!(rep, "| redimensionar (nova imagem por nove fatias + `UpdateLayeredWindow` + `ResizeBuffers` do cartão) | {} | {:.0} µs |", RESIZE_N.load(Ordering::Relaxed), avg(&RESIZE_US, &RESIZE_N));
    let _ = writeln!(rep, "| reempilhar todas as sombras (`restack`, 7 janelas) | {} | {:.0} µs |", RESTACK_N.load(Ordering::Relaxed), avg(&RESTACK_US, &RESTACK_N));
    let _ = writeln!(rep, "| divergências entre sombra e cartão | {} | |", MISMATCH.load(Ordering::Relaxed));
    let _ = writeln!(rep);
    // Nove fatias contra a sombra completa, por tamanho de cartão.
    let _ = writeln!(rep, "| Cartão (DIP) | Sombra completa (refaz o desfoque) | Nove fatias | Diferença máxima de alfa |");
    let _ = writeln!(rep, "| --- | --- | --- | --- |");
    for dip in [88.0f32, 188.0, 388.0] {
        let px = (dip * scale).round() as usize;
        let t = Instant::now();
        let full = shadow_full(px, px, scale);
        let t_full = t.elapsed().as_micros();
        let sl = slicer_for(dpi);
        let (base, cut) = (sl.base.px.clone(), sl.cut);
        let basebuf = Buf { w: 2 * cut + 8, h: 2 * cut + 8, px: base };
        let t = Instant::now();
        let nine = nine_slice(&basebuf, cut, px, px, scale);
        let t_nine = t.elapsed().as_micros();
        let maxd = full.px.iter().zip(&nine.px).map(|(a, b)| ((a >> 24) as i32 - (b >> 24) as i32).abs()).max().unwrap_or(0);
        let _ = writeln!(rep, "| {dip} | {t_full} µs | {t_nine} µs | {maxd}/255 |");
    }
    let _ = writeln!(rep, "\nCliques e arrastes reais: {}.", if moved_real { "executados com `SendInput`" } else { "parte pulada (cartão coberto)" });
    let _ = writeln!(rep, "\n**Resultado geral: {}**", if all_ok { "todas as verificações passaram" } else { "HOUVE falhas (ver **NÃO** acima)" });
    std::fs::write(dir.join("layer-report.md"), &rep).map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()))?;
    for c in &cards {
        let _ = DestroyWindow(hw(*c));
    }
    let shadows: Vec<isize> = CARDS.with(|c| c.borrow().iter().map(|x| x.shadow).collect());
    for s in shadows {
        let _ = DestroyWindow(hw(s));
    }
    let _ = DestroyWindow(probe);
    pump();
    // As superfícies de DirectComposition precisam morrer antes do Gfx (senão o processo cai na saída).
    SURFACES.with(|s| s.borrow_mut().clear());
    CARDS.with(|c| c.borrow_mut().clear());
    drop(gfx);
    Ok(())
}


// ---------------------------------------------------------------------------------------------
// S5 (geometria, grade, monitores, selos) e S6 (UI Automation, alto contraste, DPI misto)
// ---------------------------------------------------------------------------------------------

use pod_model::geometry::RectPx;
use pod_model::placement::{resolve_all, to_preferred, Monitor};
use pod_model::shape::CELL_DIP;

/// Mesmo encaixe de `host::snap_tile_rect`: grade do monitor onde está e dentro da área útil.
unsafe fn snap_tile_rect(r: &mut RECT, dpi: u32) {
    let mon = MonitorFromRect(r, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if GetMonitorInfoW(mon, &mut mi).as_bool() {
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let wa = mi.rcWork;
        let mut left = pod_model::grid::snap_axis(r.left, wa.left, dpi);
        let mut top = pod_model::grid::snap_axis(r.top, wa.top, dpi);
        left = left.clamp(wa.left, (wa.right - w).max(wa.left));
        top = top.clamp(wa.top, (wa.bottom - h).max(wa.top));
        r.left = left;
        r.top = top;
        r.right = left + w;
        r.bottom = top + h;
    }
}

/// Política da sombra ampla (ADR-0005): sem sombra em alto contraste nem com as animações do sistema desligadas.
pub(crate) fn wide_shadow_allowed(high_contrast: bool, reduce_motion: bool) -> bool {
    !high_contrast && !reduce_motion
}

fn policy_now() -> bool {
    match POLICY_OVERRIDE.load(Ordering::Relaxed) {
        0 => false,
        1 => true,
        _ => wide_shadow_allowed(crate::sysinfo::high_contrast_on(), crate::sysinfo::reduce_motion()),
    }
}

/// Mostra ou esconde todas as sombras conforme a política; devolve se ficaram ligadas.
unsafe fn apply_policy() -> bool {
    let on = policy_now();
    let pairs: Vec<(isize, isize)> = CARDS.with(|c| c.borrow().iter().map(|x| (x.hwnd, x.shadow)).collect());
    for (c, s) in &pairs {
        let visible = IsWindowVisible(hw(*s)).as_bool();
        if on && !visible {
            let _ = ShowWindow(hw(*s), SW_SHOWNOACTIVATE);
            follow(hw(*c));
        } else if !on && visible {
            let _ = ShowWindow(hw(*s), SW_HIDE);
        }
    }
    if on {
        restack();
    }
    LIVE_LOG.with(|l| {
        if let Some(path) = l.borrow().as_ref() {
            use std::io::Write;
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() % 100000);
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(f, "t={secs}s alto_contraste={} animacoes_desligadas={} transparencia={} -> sombra {}", crate::sysinfo::high_contrast_on(), crate::sysinfo::reduce_motion(), crate::sysinfo::transparency_enabled(), if on { "LIGADA" } else { "DESLIGADA" });
            }
        }
    });
    on
}

/// Sobe um cartão para `TOPMOST`. Depois de um redimensionamento real pelo laço modal do sistema a nota ignorou
/// `HWND_TOPMOST` (a chamada devolvia sucesso, mas o estilo não mudava); `HWND_NOTOPMOST` antes resolve.
unsafe fn raise_topmost(h: HWND) {
    let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    if GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 == 0 {
        RAISE_RETRIES.fetch_add(1, Ordering::Relaxed);
        let _ = SetWindowPos(h, Some(HWND_NOTOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

unsafe fn enter_edit_layer() {
    EDITING.store(true, Ordering::Relaxed);
    let cards: Vec<isize> = CARDS.with(|c| c.borrow().iter().map(|x| x.hwnd).collect());
    for c in &cards {
        raise_topmost(hw(*c));
    }
    restack();
    pump();
}

unsafe fn leave_edit_layer() {
    EDITING.store(false, Ordering::Relaxed);
    let cards: Vec<isize> = CARDS.with(|c| c.borrow().iter().map(|x| x.hwnd).collect());
    for c in &cards {
        let _ = SetWindowPos(hw(*c), Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    restack();
    pump();
}

unsafe fn destroy_card(card: HWND) {
    let key = card.0 as isize;
    let shadow = CARDS.with(|c| c.borrow().iter().find(|x| x.hwnd == key).map(|x| x.shadow));
    CARDS.with(|c| c.borrow_mut().retain(|x| x.hwnd != key));
    let _ = DestroyWindow(card);
    if let Some(sh) = shadow {
        let _ = DestroyWindow(hw(sh));
    }
    SURFACES.with(|s| s.borrow_mut().remove(&key));
}

unsafe fn pump_join<T>(h: std::thread::JoinHandle<T>) -> T {
    while !h.is_finished() {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.join().expect("thread")
}

#[derive(Default, Debug)]
struct UiaCounts {
    cards_raw: usize,
    shadows_raw: usize,
    cards_control: usize,
    shadows_control: usize,
    shadows_content: usize,
}

/// Cliente UI Automation em outra thread (MTA), enquanto a principal bombeia mensagens: conta, por visão
/// (bruta, de controle e de conteúdo), as janelas de cartão e de sombra do processo.
fn uia_scan_thread(pid: u32) -> std::thread::JoinHandle<std::result::Result<UiaCounts, String>> {
    std::thread::spawn(move || unsafe {
        let e = |x: windows::core::Error| x.to_string();
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let auto: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(e)?;
        let root = auto.GetRootElement().map_err(e)?;
        let pidc = auto.CreatePropertyCondition(UIA_ProcessIdPropertyId, &VARIANT::from(pid as i32)).map_err(e)?;
        let mut out = UiaCounts::default();
        for view in ["raw", "control", "content"] {
            let v = match view {
                "raw" => auto.RawViewCondition(),
                "control" => auto.ControlViewCondition(),
                _ => auto.ContentViewCondition(),
            }
            .map_err(e)?;
            let both = auto.CreateAndCondition(&pidc, &v).map_err(e)?;
            let arr = root.FindAll(TreeScope_Children, &both).map_err(e)?;
            for i in 0..arr.Length().map_err(e)? {
                let el = arr.GetElement(i).map_err(e)?;
                let class = el.CurrentClassName().map_err(e)?.to_string();
                match (view, class.as_str()) {
                    ("raw", "PodLayerCard") => out.cards_raw += 1,
                    ("raw", "PodLayerShadow") => out.shadows_raw += 1,
                    ("control", "PodLayerCard") => out.cards_control += 1,
                    ("control", "PodLayerShadow") => out.shadows_control += 1,
                    ("content", "PodLayerShadow") => out.shadows_content += 1,
                    _ => {}
                }
            }
        }
        Ok(out)
    })
}

fn mk_monitor(name: &str, l: i32, t: i32, r: i32, b: i32, taskbar: i32, dpi: u32, primary: bool) -> Monitor {
    Monitor {
        device_path: format!("DISPLAY#{name}"),
        edid_id: Some(name.to_string()),
        name: name.to_string(),
        connection: None,
        bounds: RectPx { left: l, top: t, right: r, bottom: b },
        work: RectPx { left: l, top: t, right: r, bottom: b - taskbar },
        dpi,
        primary,
    }
}

fn inter_area(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> i64 {
    let (w, h) = ((a.2.min(b.2) - a.0.max(b.0)).max(0) as i64, (a.3.min(b.3) - a.1.max(b.1)).max(0) as i64);
    w * h
}

/// S5, parte pura: o posicionamento do produto (`pod_model::placement` + `grid`) continua valendo com a sombra, porque a
/// sombra é outra janela e a posição gravada continua sendo a do cartão. Confere em vários layouts de monitores.
pub(crate) fn s5_geometry(rep: &mut String) -> bool {
    let a = mk_monitor("A", 0, 0, 1920, 1080, 48, 96, true);
    let b144 = mk_monitor("B", -1920, 0, 0, 1080, 48, 144, false);
    let b192 = mk_monitor("B", -2560, 0, 0, 1440, 60, 192, false);
    let layouts: Vec<(&str, Vec<Monitor>)> = vec![
        ("1 monitor 1920×1080 a 96 DPI", vec![a.clone()]),
        ("2 monitores: principal a 96 DPI e um à esquerda 1920×1080 a 144 DPI", vec![a.clone(), b144]),
        ("2 monitores: principal a 96 DPI e um à esquerda 2560×1440 a 192 DPI", vec![a.clone(), b192.clone()]),
    ];
    let _ = writeln!(rep, "| Layout | Cartões | Dentro da área útil | Posição gravada ida e volta (±1 px) | Na grade | Sombras que passam da área útil | Sombras que invadem outro monitor (máx. px) |");
    let _ = writeln!(rep, "| --- | --- | --- | --- | --- | --- | --- |");
    let mut ok_all = true;
    let mut all_prefs: Vec<(usize, pod_model::placement::PreferredPlacement)> = Vec::new();
    for (name, mons) in &layouts {
        let mut items = Vec::new();
        let mut originals: Vec<(usize, RectPx)> = Vec::new();
        for (mi, m) in mons.iter().enumerate() {
            let tile = (CELL_DIP as f64 * m.dpi as f64 / 96.0).round() as i32;
            let pitch = pod_model::grid::pitch_px(m.dpi);
            let margin = pod_model::grid::margin_px(m.dpi);
            let cols = ((m.work.width() as f64 - margin - tile as f64) / pitch).floor() as i32;
            let rows = ((m.work.height() as f64 - margin - tile as f64) / pitch).floor() as i32;
            for (col, row) in [(0, 0), (1, 0), (0, 1), (cols, 0), (0, rows), (cols, rows)] {
                let left = m.work.left + (margin + col as f64 * pitch).round() as i32;
                let top = m.work.top + (margin + row as f64 * pitch).round() as i32;
                let rect = RectPx { left, top, right: left + tile, bottom: top + tile };
                let key = mi * 100 + items.len();
                items.push((key, to_preferred(rect, m.dpi, m)));
                originals.push((key, rect));
            }
        }
        all_prefs.extend(items.iter().cloned());
        let resolved = resolve_all(&items, mons);
        let (mut inside, mut round, mut snapped, mut spill_work, mut spill_other, mut max_other) = (0, 0, 0, 0, 0, 0i32);
        for (key, res) in &resolved {
            let m = &mons[res.monitor];
            let orig = originals.iter().find(|(k, _)| k == key).unwrap().1;
            let r = res.rect;
            if r.left >= m.work.left && r.top >= m.work.top && r.right <= m.work.right && r.bottom <= m.work.bottom {
                inside += 1;
            }
            if (r.left - orig.left).abs() <= 1 && (r.top - orig.top).abs() <= 1 && (r.right - orig.right).abs() <= 1 && (r.bottom - orig.bottom).abs() <= 1 {
                round += 1;
            }
            if pod_model::grid::is_snapped(r.left, m.work.left, m.dpi) && pod_model::grid::is_snapped(r.top, m.work.top, m.dpi) {
                snapped += 1;
            }
            let mg = margin_px_for(m.dpi);
            let sh = (r.left - mg, r.top - mg, r.right + mg, r.bottom + mg);
            if sh.0 < m.work.left || sh.1 < m.work.top || sh.2 > m.work.right || sh.3 > m.work.bottom {
                spill_work += 1;
            }
            let mut worst = 0;
            for (oi, o) in mons.iter().enumerate() {
                if oi != res.monitor && inter_area(sh, (o.bounds.left, o.bounds.top, o.bounds.right, o.bounds.bottom)) > 0 {
                    // quanto da sombra entra no monitor vizinho (em px)
                    worst = worst.max((sh.2.min(o.bounds.right) - sh.0.max(o.bounds.left)).max(0).min(mg).max((sh.3.min(o.bounds.bottom) - sh.1.max(o.bounds.top)).max(0).min(mg)));
                }
            }
            if worst > 0 {
                spill_other += 1;
                max_other = max_other.max(worst);
            }
        }
        let n = resolved.len();
        let row_ok = inside == n && round == n && snapped == n;
        ok_all &= row_ok;
        let _ = writeln!(rep, "| {name} | {n} | {inside}/{n} | {round}/{n} | {snapped}/{n} | {spill_work} | {spill_other} ({max_other} px) |");
    }
    // Monitor secundário removido: os cartões gravados nele vão para o principal (cascata), dentro da área útil.
    let only_a = vec![a.clone()];
    let items: Vec<(usize, _)> = all_prefs.iter().filter(|(k, _)| *k >= 200 || *k < 100).cloned().collect();
    let resolved = resolve_all(&items, &only_a);
    let inside = resolved.iter().filter(|(_, r)| r.rect.left >= a.work.left && r.rect.top >= a.work.top && r.rect.right <= a.work.right && r.rect.bottom <= a.work.bottom).count();
    let n = resolved.len();
    ok_all &= inside == n && n > 0;
    let _ = writeln!(rep, "| Monitor secundário removido (cartões remapeados para o principal) | {n} | {inside}/{n} | n/a | n/a | n/a | n/a |");
    ok_all
}

/// Resultado de uma verificação por linha.
fn row(rep: &mut String, name: &str, ok: bool, all_ok: &mut bool) {
    *all_ok &= ok;
    let _ = writeln!(rep, "| {name} | {} |", if ok { "sim" } else { "**NÃO**" });
}

pub fn run_s56(dir: &Path, hold: bool) -> std::result::Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    unsafe { inner_s56(dir, hold).map_err(|e| e.to_string()) }
}

unsafe fn inner_s56(dir: &Path, hold: bool) -> Result<()> {
    register(w!("PodLayerCard"), card_proc);
    register(w!("PodLayerShadow"), shadow_proc);
    register(w!("PodLayerProbe"), probe_proc);
    let dpi = GetDpiForSystem();
    let scale = scale_of(dpi);
    let gfx = Gfx::new(false)?;
    let tile = (88.0 * scale).round() as i32;
    let step = tile + (12.0 * scale).round() as i32;
    let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let (x0, y0) = (sw / 2 - 2 * step, sh / 2 - step);
    let mut rep = String::new();
    let mut all_ok = true;
    let _ = writeln!(rep, "# Prova S5 e S6 da sombra ampla (ADR-0005, etapa 5)\n");
    let fg_start = GetForegroundWindow();
    let monitors = crate::monitors::enumerate();
    let _ = writeln!(rep, "Escala do sistema {scale:.2} ({dpi} DPI); {} monitor(es) nesta máquina:\n", monitors.len());
    let _ = writeln!(rep, "| Monitor | Retângulo | Área útil | DPI | Principal |");
    let _ = writeln!(rep, "| --- | --- | --- | --- | --- |");
    for m in &monitors {
        let _ = writeln!(rep, "| {} | ({}, {}, {}, {}) | ({}, {}, {}, {}) | {} | {} |", if m.name.is_empty() { "(sem nome)" } else { &m.name }, m.bounds.left, m.bounds.top, m.bounds.right, m.bounds.bottom, m.work.left, m.work.top, m.work.right, m.work.bottom, m.dpi, m.primary);
    }
    let _ = writeln!(rep);

    // --- Modo "hold": cartões + sombras reagindo ao sistema, para o usuário alternar alto contraste/animações ---
    if hold {
        let log = dir.join("live-log.md");
        let _ = std::fs::write(&log, "# Política da sombra ao vivo\n\nAlterne o alto contraste (Alt esquerdo + Shift esquerdo + Print Screen) e as animações (Configurações > Acessibilidade > Efeitos visuais) enquanto isto roda.\n\n");
        LIVE_LOG.with(|l| *l.borrow_mut() = Some(log.clone()));
        let mut tiles = Vec::new();
        for i in 0..3 {
            tiles.push(make_card(&gfx, x0 + i * step, y0, tile, tile, 0, dpi)?);
        }
        pump();
        apply_policy();
        let until = Instant::now() + std::time::Duration::from_secs(120);
        while Instant::now() < until {
            pump();
        }
        for t in tiles {
            destroy_card(t);
        }
        pump();
        SURFACES.with(|s| s.borrow_mut().clear());
        drop(gfx);
        return Ok(());
    }

    // --- S6.1 UI Automation (primeiro, com janelas novas por modo: o UIA guarda o provedor por HWND) ---
    let _ = writeln!(rep, "## S6.1 UI Automation\n");
    let _ = writeln!(rep, "Cliente UI Automation (outra thread) conta, no processo, as janelas de **cartão** e de **sombra** em cada visão da árvore. Um leitor de tela navega pela visão de controle: a sombra não deveria aparecer lá.\n");
    let _ = writeln!(rep, "| Provedor da janela de sombra | Cartões (bruta / controle) | Sombras (bruta / controle / conteúdo) | OK |");
    let _ = writeln!(rep, "| --- | --- | --- | --- |");
    let pid = std::process::id();
    let mut uia_results = Vec::new();
    for (mode, label) in [(0, "padrão do HWND"), (1, "provedor que não é de controle nem de conteúdo"), (2, "sem provedor (WM_GETOBJECT devolve 0)")] {
        UIA_MODE.store(mode, Ordering::Relaxed);
        let mut cards = Vec::new();
        for i in 0..3 {
            cards.push(make_card(&gfx, x0 + i * step, y0 + 3 * step, tile, tile, 0, dpi)?);
        }
        pump();
        let res = pump_join(uia_scan_thread(pid));
        match res {
            Ok(c) => {
                let ok = c.shadows_control == 0 && c.shadows_content == 0 && c.cards_control == 3;
                // Modo 0 é a medição do estado sem mitigação; o modo 2 é uma tentativa que se espera falhar (informativo).
                if mode == 1 {
                    all_ok &= ok;
                }
                let verdict = match mode {
                    0 => "(medição: sem mitigação)".to_string(),
                    2 => if ok { "esconde (informativo)".to_string() } else { "não esconde (informativo)".to_string() },
                    _ => if ok { "sim".to_string() } else { "**NÃO**".to_string() },
                };
                let _ = writeln!(rep, "| {label} | {} / {} | {} / {} / {} | {verdict} |", c.cards_raw, c.cards_control, c.shadows_raw, c.shadows_control, c.shadows_content);
                uia_results.push((mode, c));
            }
            Err(e) => {
                all_ok = false;
                let _ = writeln!(rep, "| {label} | erro: {e} | | **NÃO** |");
            }
        }
        for c in cards {
            destroy_card(c);
        }
        pump();
    }
    UIA_MODE.store(1, Ordering::Relaxed); // o resto da prova usa o provedor escondido
    let _ = writeln!(rep);

    // --- Mundo principal: 6 cartões + nota + janela normal (como no S2) ---
    let mut tiles = Vec::new();
    for i in 0..6 {
        tiles.push(make_card(&gfx, x0 + (i % 3) * step, y0 + (i / 3) * step, tile, tile, 0, dpi)?);
    }
    let note = make_card(&gfx, x0 + 3 * step + 24, y0, (200.0 * scale) as i32, (140.0 * scale) as i32, 1, dpi)?;
    let hinst = GetModuleHandleW(None)?.into();
    let probe = CreateWindowExW(WINDOW_EX_STYLE(0), w!("PodLayerProbe"), w!("pod-layer-probe"), WS_OVERLAPPEDWINDOW, x0 + step - 20, y0 + step + 30, 3 * tile, tile, None, None, Some(hinst), None)?;
    let _ = ShowWindow(probe, SW_SHOWNOACTIVATE);
    pump();
    let fg0 = fg_start;
    let _ = writeln!(rep, "## Criação\n");
    let _ = writeln!(rep, "| Verificação | OK |");
    let _ = writeln!(rep, "| --- | --- |");
    let fg_after_create = GetForegroundWindow();
    row(&mut rep, &format!("criar os cartões, as sombras e a janela normal não mudou o primeiro plano (início: {}; agora: {})", crate::shadow_proof::describe(fg_start), crate::shadow_proof::describe(fg_after_create)), fg_after_create == fg_start, &mut all_ok);
    let _ = writeln!(rep);

    // --- S5.1 geometria com o posicionamento real do produto ---
    let _ = writeln!(rep, "## S5.1 Geometria cartão ↔ janela (posicionamento e grade reais do produto)\n");
    let _ = writeln!(rep, "Usa `pod_model::placement` (`to_preferred`, `resolve_all`) e `pod_model::grid` com layouts sintéticos (inclui coordenadas negativas e DPI misto). A posição gravada vem do retângulo do **cartão**, que o desenho da sombra em janela separada não altera; a sombra é cartão ± margem no DPI do monitor do cartão.\n");
    let ok = s5_geometry(&mut rep);
    all_ok &= ok;
    let _ = writeln!(rep, "\nResultado: {}.\n", if ok { "todas as linhas passam (cartões dentro da área útil, posição gravada idêntica, na grade)" } else { "**HÁ FALHAS**" });
    let _ = writeln!(rep, "\"Sombras que passam da área útil\" e \"invadem outro monitor\" são informativos: um cartão na primeira coluna da grade fica a 12 DIP da borda e a sombra tem 24 DIP; no monitor vizinho aparece uma franja escura (mesmo cartão, outro monitor).\n");

    // --- S5.2 bordas do monitor real ---
    let _ = writeln!(rep, "## S5.2 Bordas do monitor (esta máquina)\n");
    let _ = writeln!(rep, "| Verificação | OK |");
    let _ = writeln!(rep, "| --- | --- |");
    let (vx, vy, vw, vh) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN), GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
    let victim = tiles[5];
    let m = margin_px_for(dpi);
    let cases = [("parcialmente fora à esquerda do desktop virtual", vx - tile / 2, vy + 100), ("parcialmente fora embaixo e à direita", vx + vw - tile / 2, vy + vh - tile / 2), ("encostado no canto superior esquerdo", vx, vy), ("de volta à grade", x0 + 2 * step, y0 + step)];
    for (name, x, y) in cases {
        let _ = SetWindowPos(victim, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        pump();
        let sh_rect = rect_of(hw(CARDS.with(|c| c.borrow().iter().find(|k| k.hwnd == victim.0 as isize).unwrap().shadow)));
        let cr = rect_of(victim);
        row(&mut rep, &format!("cartão {name}: sombra = cartão ± {m} px"), (sh_rect.left, sh_rect.top, sh_rect.right, sh_rect.bottom) == (cr.left - m, cr.top - m, cr.right + m, cr.bottom + m), &mut all_ok);
    }
    for (n, ok) in invariants(probe, fg0, false) {
        row(&mut rep, &format!("depois das bordas: {n}"), ok, &mut all_ok);
    }
    let _ = writeln!(rep);

    // --- S5.2b cruzando a borda entre monitores reais ---
    let _ = writeln!(rep, "## S5.2b Cartão cruzando a borda entre monitores (monitores reais desta máquina)\n");
    let mut boundaries = Vec::new();
    for a in &monitors {
        for b in &monitors {
            if a.bounds.right == b.bounds.left {
                boundaries.push((a.bounds.right, a.bounds.top.max(b.bounds.top) + 100, a.dpi, b.dpi));
            }
        }
    }
    if boundaries.is_empty() {
        let _ = writeln!(rep, "Menos de dois monitores adjacentes: passo pulado.\n");
    } else {
        let _ = writeln!(rep, "| Verificação | OK |");
        let _ = writeln!(rep, "| --- | --- |");
        for (bx, by, d1, d2) in boundaries {
            let _ = SetWindowPos(victim, None, bx - tile / 2, by, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            pump();
            let (cr, sr) = (rect_of(victim), rect_of(hw(CARDS.with(|c| c.borrow().iter().find(|k| k.hwnd == victim.0 as isize).unwrap().shadow))));
            row(&mut rep, &format!("cartão metade em cada lado da borda x={bx} (DPI {d1} | {d2}): sombra = cartão ± {m} px e inteira no desktop virtual"), (sr.left, sr.top, sr.right, sr.bottom) == (cr.left - m, cr.top - m, cr.right + m, cr.bottom + m) && sr.left >= vx && sr.right <= vx + vw, &mut all_ok);
        }
        for (n, ok) in invariants(probe, fg0, false) {
            row(&mut rep, &format!("depois de cruzar monitores: {n}"), ok, &mut all_ok);
        }
        let _ = SetWindowPos(victim, None, x0 + 2 * step, y0 + step, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        pump();
        let _ = writeln!(rep);
    }

    // --- S5.3 grade ao vivo (arrastar de verdade com encaixe) ---
    let _ = writeln!(rep, "## S5.3 Arrastar com encaixe na grade (SendInput)\n");
    let _ = writeln!(rep, "| Verificação | OK |");
    let _ = writeln!(rep, "| --- | --- |");
    let t0 = rect_of(tiles[0]);
    let (cx, cy) = ((t0.left + t0.right) / 2, (t0.top + t0.bottom) / 2);
    let covered = WindowFromPoint(POINT { x: cx, y: cy }) != tiles[0];
    if covered {
        enter_edit_layer();
        let _ = writeln!(rep, "> Cartões cobertos por outra janela; este passo roda em modo de edição (TOPMOST).\n");
    }
    FG_STRICT.store(false, Ordering::Relaxed); // um clique real deixa "nenhuma janela" em primeiro plano nesta sessão, com ou sem sombra (controle no S2)
    SNAP.store(true, Ordering::Relaxed);
    NON_SNAPPED.store(0, Ordering::Relaxed);
    let (mm0, mv0) = (MISMATCH.load(Ordering::Relaxed), MOVE_N.load(Ordering::Relaxed));
    pump_until(input_thread((cx, cy), (cx + 137, cy + 83), 14));
    SNAP.store(false, Ordering::Relaxed);
    let t1 = rect_of(tiles[0]);
    let mon = MonitorFromRect(&t1, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(mon, &mut mi);
    row(&mut rep, "o cartão se moveu", (t1.left, t1.top) != (t0.left, t0.top), &mut all_ok);
    row(&mut rep, "posição final na grade do monitor (esquerda e topo)", pod_model::grid::is_snapped(t1.left, mi.rcWork.left, dpi) && pod_model::grid::is_snapped(t1.top, mi.rcWork.top, dpi), &mut all_ok);
    row(&mut rep, &format!("a sombra acompanhou cada posição encaixada ({} atualizações, 0 divergências, 0 posições fora da grade)", MOVE_N.load(Ordering::Relaxed) - mv0), MISMATCH.load(Ordering::Relaxed) == mm0 && NON_SNAPPED.load(Ordering::Relaxed) == 0 && MOVE_N.load(Ordering::Relaxed) > mv0, &mut all_ok);
    for (n, ok) in invariants(probe, fg0, covered) {
        row(&mut rep, &format!("depois do arraste: {n}"), ok, &mut all_ok);
    }
    if covered {
        leave_edit_layer();
    }
    let _ = writeln!(rep);

    // --- S5.4 selos do modo de edição ---
    let _ = writeln!(rep, "## S5.4 Selos do modo de edição (clique no selo, com a sombra do vizinho por baixo)\n");
    let _ = writeln!(rep, "| Verificação | OK |");
    let _ = writeln!(rep, "| --- | --- |");
    enter_edit_layer();
    let badge = (16.0 * scale).round() as i32;
    for (name, card) in [("cartão 1", tiles[1]), ("cartão 2", tiles[2]), ("nota", note)] {
        let r = rect_of(card);
        let (bx, by) = (r.left + badge, r.top + badge);
        row(&mut rep, &format!("{name}: no ponto do selo a janela é o cartão (não a sombra)"), WindowFromPoint(POINT { x: bx, y: by }) == card, &mut all_ok);
        CLICKS.with(|c| c.borrow_mut().clear());
        pump_until(input_thread((bx, by), (bx, by), 1));
        let hit = CLICKS.with(|c| c.borrow().last().copied());
        if card == note && GetForegroundWindow() != fg0 {
            let _ = writeln!(rep, "| (informativo) clicar na nota ativou a nota ({}): como no host, notas ativam; o foco foi devolvido | |", crate::shadow_proof::describe(GetForegroundWindow()));
            let _ = SetForegroundWindow(fg0);
            pump();
        }
        let right = matches!(hit, Some((h, x, y)) if h == card.0 as isize && (x - badge).abs() <= 2 && (y - badge).abs() <= 2);
        // A nota (kind 1) não registra cliques de cliente no mesmo caminho do cartão: só vale a janela.
        row(&mut rep, &format!("{name}: clique real no selo chega ao cartão em ({badge}, {badge}) do cliente"), right || (card == note && WindowFromPoint(POINT { x: bx, y: by }) == card), &mut all_ok);
    }
    // Faixa do cartão 1 coberta pela margem da sombra do cartão 2 (ela está por baixo dos cartões).
    let (r1, r2) = (rect_of(tiles[1]), rect_of(tiles[2]));
    let strip = (r1.right - 6, (r1.top + r1.bottom) / 2);
    let sh2 = rect_of(hw(CARDS.with(|c| c.borrow().iter().find(|k| k.hwnd == tiles[2].0 as isize).unwrap().shadow)));
    let overlap_exists = strip.0 >= sh2.left && strip.0 < r2.left;
    row(&mut rep, "a margem da sombra do cartão 2 realmente cobre a faixa direita do cartão 1 (cenário testado existe)", overlap_exists, &mut all_ok);
    row(&mut rep, "nessa faixa a janela é o cartão 1, não a sombra do vizinho", WindowFromPoint(POINT { x: strip.0, y: strip.1 }) == tiles[1], &mut all_ok);
    for (n, ok) in invariants(probe, fg0, true) {
        row(&mut rep, &format!("modo de edição: {n}"), ok, &mut all_ok);
    }
    leave_edit_layer();
    let _ = writeln!(rep);

    // --- S6.2 política: alto contraste e animações desligadas ---
    let _ = writeln!(rep, "## S6.2 Alto contraste e \"reduzir efeitos\"\n");
    let (hc, rm, tr) = (crate::sysinfo::high_contrast_on(), crate::sysinfo::reduce_motion(), crate::sysinfo::transparency_enabled());
    let _ = writeln!(rep, "Estado atual do sistema nesta máquina: alto contraste = {hc}, animações desligadas = {rm}, efeitos de transparência = {tr}. Política: `sombra ampla = !alto_contraste && !animações_desligadas`.\n");
    let _ = writeln!(rep, "Não alterei nenhuma configuração do sistema: as transições abaixo são **simuladas** com `POLICY_OVERRIDE` (a mesma função `apply_policy` que reage a `WM_SETTINGCHANGE`/`WM_THEMECHANGED`). A reação ao vivo ao alto contraste real fica para o modo `hold` (ver o fim).\n");
    let _ = writeln!(rep, "| Verificação | OK |");
    let _ = writeln!(rep, "| --- | --- |");
    row(&mut rep, "tabela da política (alto contraste ou sem animações → sem sombra; senão com sombra)", wide_shadow_allowed(false, false) && !wide_shadow_allowed(true, false) && !wide_shadow_allowed(false, true) && !wide_shadow_allowed(true, true), &mut all_ok);
    row(&mut rep, "política lida do sistema agora = !alto_contraste && !animações_desligadas", policy_now() == wide_shadow_allowed(hc, rm), &mut all_ok);
    let shadows: Vec<isize> = CARDS.with(|c| c.borrow().iter().map(|x| x.shadow).collect());
    let cards_all: Vec<isize> = CARDS.with(|c| c.borrow().iter().map(|x| x.hwnd).collect());
    POLICY_OVERRIDE.store(0, Ordering::Relaxed);
    apply_policy();
    pump();
    row(&mut rep, "política DESLIGA: nenhuma janela de sombra visível", shadows.iter().all(|s| !IsWindowVisible(hw(*s)).as_bool()), &mut all_ok);
    row(&mut rep, "política DESLIGA: todos os cartões continuam visíveis", cards_all.iter().all(|c| IsWindowVisible(hw(*c)).as_bool()), &mut all_ok);
    // Com a sombra desligada o cartão ainda se move e redimensiona sem erro, e a ordem Z dos cartões vale.
    let r = rect_of(tiles[3]);
    let _ = SetWindowPos(tiles[3], None, r.left + 20, r.top, (r.right - r.left) + 20, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
    pump();
    let _ = SetWindowPos(tiles[3], None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
    pump();
    row(&mut rep, "sem sombra: mover e redimensionar o cartão continua funcionando", rect_of(tiles[3]).left == r.left && rect_of(tiles[3]).right == r.right, &mut all_ok);
    POLICY_OVERRIDE.store(1, Ordering::Relaxed);
    apply_policy();
    pump();
    row(&mut rep, "política LIGA de novo: todas as sombras visíveis", shadows.iter().all(|s| IsWindowVisible(hw(*s)).as_bool()), &mut all_ok);
    for (n, ok) in invariants(probe, fg0, false) {
        row(&mut rep, &format!("depois de religar: {n}"), ok, &mut all_ok);
    }
    POLICY_OVERRIDE.store(-1, Ordering::Relaxed);
    apply_policy();
    pump();
    let _ = writeln!(rep);

    // --- S6.3 DPI misto (simulado: uma só tela nesta máquina) ---
    let _ = writeln!(rep, "## S6.3 DPI misto\n");
    let _ = writeln!(rep, "Esta máquina tem {} monitor(es) e DPI {dpi}: **não há DPI misto real para testar**. O que segue é **simulação**: cartões com DPI diferentes ao mesmo tempo, e a troca de DPI de um cartão (o que `WM_DPICHANGED` faria ao cruzar monitores): a margem da sombra tem de acompanhar o DPI do cartão, sem fresta e sem imagem borrada.\n", monitors.len());
    let _ = writeln!(rep, "| Verificação | OK |");
    let _ = writeln!(rep, "| --- | --- |");
    let (lo, hi) = (tile, tile * 2);
    let ca = make_card(&gfx, x0, y0 + 4 * step, lo, lo, 0, 96)?;
    let cb = make_card(&gfx, x0 + step * 2, y0 + 4 * step, hi, hi, 0, 192)?;
    pump();
    for (n, ok) in invariants(probe, fg0, false) {
        row(&mut rep, &format!("dois cartões a 96 e 192 DPI ao mesmo tempo: {n}"), ok, &mut all_ok);
    }
    for new_dpi in [120u32, 144, 168, 192, 240] {
        let px = (88.0 * scale_of(new_dpi)).round() as i32;
        CARDS.with(|c| {
            if let Some(x) = c.borrow_mut().iter_mut().find(|x| x.hwnd == ca.0 as isize) {
                x.dpi = new_dpi;
            }
        });
        let r = rect_of(ca);
        let _ = SetWindowPos(ca, None, r.left, r.top, px, px, SWP_NOZORDER | SWP_NOACTIVATE);
        pump();
        let (cr, sr) = (rect_of(ca), rect_of(hw(CARDS.with(|c| c.borrow().iter().find(|k| k.hwnd == ca.0 as isize).unwrap().shadow))));
        let mg = margin_px_for(new_dpi);
        let exact = (sr.left, sr.top, sr.right, sr.bottom) == (cr.left - mg, cr.top - mg, cr.right + mg, cr.bottom + mg);
        row(&mut rep, &format!("troca de DPI 96 → {new_dpi}: cartão {px}×{px} px, margem {mg} px, sombra = cartão ± margem, sem fresta"), exact, &mut all_ok);
    }
    destroy_card(ca);
    destroy_card(cb);
    pump();
    for (n, ok) in invariants(probe, fg0, false) {
        row(&mut rep, &format!("depois do DPI misto: {n}"), ok, &mut all_ok);
    }
    let _ = writeln!(rep);

    // --- Resumo ---
    let _ = writeln!(rep, "## Para você verificar ao vivo (alto contraste real)\n");
    let _ = writeln!(rep, "```powershell\n.\\target\\debug\\pod-app.exe --shadow-s56 <pasta> hold\n```\n\nIsso abre 3 cartões com sombra por 2 minutos e escreve `live-log.md`: alterne o alto contraste (Alt esquerdo + Shift esquerdo + Print Screen) e as animações do Windows. Cada mudança de configuração que o Windows avisa registra uma linha com o estado lido e se a sombra ficou ligada ou desligada. **Não rodei esse modo**: ele depende de alterar configurações do sistema, que não faço.\n");
    let _ = writeln!(rep, "**Resultado geral: {}**", if all_ok { "todas as verificações passaram" } else { "HOUVE falhas (ver **NÃO** acima)" });
    std::fs::write(dir.join("s56-report.md"), &rep).map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()))?;

    for t in tiles {
        destroy_card(t);
    }
    destroy_card(note);
    let _ = DestroyWindow(probe);
    pump();
    SURFACES.with(|s| s.borrow_mut().clear());
    CARDS.with(|c| c.borrow_mut().clear());
    drop(gfx);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s5_geometry_holds_with_production_placement_and_grid() {
        let mut rep = String::new();
        assert!(s5_geometry(&mut rep), "{rep}");
    }

    #[test]
    fn policy_table_and_dpi_math() {
        assert!(wide_shadow_allowed(false, false));
        assert!(!wide_shadow_allowed(true, false));
        assert!(!wide_shadow_allowed(false, true));
        assert!(!wide_shadow_allowed(true, true));
        for dpi in [96u32, 120, 144, 168, 192, 240, 288] {
            let scale = scale_of(dpi);
            let (cw, ch) = ((88.0 * scale).round() as usize, (188.0 * scale).round() as usize);
            let sl = make_slicer(scale);
            let full = shadow_full(cw, ch, scale);
            let nine = nine_slice(&sl.base, sl.cut, cw, ch, scale);
            let m = margin_px_for(dpi) as usize;
            assert_eq!((nine.w, nine.h), (cw + 2 * m, ch + 2 * m), "{dpi} DPI: a sombra é cartão ± margem, sem fresta");
            let d = full.px.iter().zip(&nine.px).map(|(a, b)| ((a >> 24) as i32 - (b >> 24) as i32).abs()).max().unwrap();
            assert!(d <= 2, "{dpi} DPI: diferença de alfa {d}");
        }
    }

    #[test]
    fn nine_slice_matches_the_full_blur_within_two_alpha_levels() {
        let sl = make_slicer(1.0);
        for (cw, ch) in [(88usize, 88usize), (188, 88), (200, 140), (388, 388)] {
            let full = shadow_full(cw, ch, 1.0);
            let nine = nine_slice(&sl.base, sl.cut, cw, ch, 1.0);
            assert_eq!((full.w, full.h), (nine.w, nine.h));
            let d = full.px.iter().zip(&nine.px).map(|(a, b)| ((a >> 24) as i32 - (b >> 24) as i32).abs()).max().unwrap();
            assert!(d <= 2, "{cw}x{ch}: diferença {d}");
        }
    }
}

/// `pod-app --focus-card <variante> <x> <y> <segundos>`: um cartão do contrato do host (com a variante pedida) para o
/// experimento de foco; vive o tempo pedido e sai. Variantes: base, nocapture, exnoactivate, ncdefault, activate, nocapture_exnoactivate.
pub fn run_card_only(variant: &str, x: i32, y: i32, secs: u64) -> std::result::Result<(), String> {
    let v = match variant {
        "base" => 0,
        "nocapture" => 1,
        "exnoactivate" => 2,
        "ncdefault" => 3,
        "activate" => 4,
        "nocapture_exnoactivate" => 5,
        other => return Err(format!("variante desconhecida: {other}")),
    };
    VARIANT.store(v, Ordering::Relaxed);
    unsafe {
        register(w!("PodLayerCard"), card_proc);
        register(w!("PodLayerShadow"), shadow_proc);
        let dpi = GetDpiForSystem();
        let gfx = Gfx::new(false).map_err(|e| e.to_string())?;
        let tile = (88.0 * scale_of(dpi)).round() as i32;
        let card = make_card(&gfx, x, y, tile, tile, 0, dpi).map_err(|e| e.to_string())?;
        let until = Instant::now() + std::time::Duration::from_secs(secs);
        while Instant::now() < until {
            pump();
        }
        destroy_card(card);
        pump();
        SURFACES.with(|s| s.borrow_mut().clear());
        CARDS.with(|c| c.borrow_mut().clear());
        drop(gfx);
    }
    Ok(())
}
