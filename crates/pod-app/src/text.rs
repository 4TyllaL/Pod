//! Editor de texto nativo: Rich Edit windowless (`CreateTextServices`) desenhado via
//! `ITextServices2::TxDrawD2D`. Thread de UI (STA) apenas.
//!
//! `ITextHost` existe nos bindings do crate `windows`; `ITextServices2` e
//! `CreateTextServices` não, então a vtable é declarada à mão (layout de textserv.h).
//!
//! Reentrância: o serviço de texto chama de volta o host durante `TxSendMessage`. Os campos
//! do host são `Cell` (sem `&mut`), e o WndProc clona o `TextEditor` (refcount COM) antes de
//! chamar, de modo que nenhum empréstimo do estado da janela atravessa essas chamadas.

use std::cell::Cell;
use std::ffi::c_void;

use windows::core::{implement, ComObject, Interface, Result, BOOL, BSTR, GUID, HRESULT, PCSTR, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Controls::RichEdit::*;
use windows::Win32::UI::Input::Ime::{ImmGetContext, ImmReleaseContext, HIMC};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const CARET_TIMER: usize = 0xCA7;
const EN_CHANGE: u32 = 0x0300;
const EM_SETEVENTMASK: u32 = 0x0445;
const ENM_CHANGE: isize = 1;
const EM_SETLANGOPTIONS: u32 = 0x0478;
const EM_SETTYPOGRAPHYOPTIONS: u32 = 0x04CA;
const TO_ADVANCEDTYPOGRAPHY: usize = 1;
/// IMF_AUTOFONT | IMF_DUALFONT | IMF_AUTOFONTSIZEADJUST: troca de fonte por script (CJK, emoji).
const LANG_OPTIONS: isize = 0x02 | 0x80 | 0x10;
const ES_AUTOVSCROLL: u32 = 0x40;
const MAX_CHARS: u32 = 256 * 1024;
/// IID_ITextServices2 (textserv.h).
const IID_ITEXTSERVICES2: GUID = GUID::from_u128(0x8d33f741_cf58_11ce_a89d_00aa006cadc5);

type Hr = HRESULT;

/// Vtable de ITextServices2 (IUnknown + 18 métodos de ITextServices + 2 próprios).
#[repr(C)]
struct ServicesVtbl {
    _query_interface: *const c_void,
    _add_ref: *const c_void,
    _release: *const c_void,
    tx_send_message: unsafe extern "system" fn(*mut c_void, u32, WPARAM, LPARAM, *mut LRESULT) -> Hr,
    _tx_draw: *const c_void,
    _tx_get_hscroll: *const c_void,
    _tx_get_vscroll: *const c_void,
    _on_tx_set_cursor: *const c_void,
    _tx_query_hit_point: *const c_void,
    on_tx_inplace_activate: unsafe extern "system" fn(*mut c_void, *const RECT) -> Hr,
    on_tx_inplace_deactivate: unsafe extern "system" fn(*mut c_void) -> Hr,
    _on_tx_ui_activate: *const c_void,
    _on_tx_ui_deactivate: *const c_void,
    tx_get_text: unsafe extern "system" fn(*mut c_void, *mut BSTR) -> Hr,
    tx_set_text: unsafe extern "system" fn(*mut c_void, PCWSTR) -> Hr,
    _tx_get_cur_target_x: *const c_void,
    _tx_get_base_line_pos: *const c_void,
    _tx_get_natural_size: *const c_void,
    _tx_get_drop_target: *const c_void,
    on_tx_property_bits_change: unsafe extern "system" fn(*mut c_void, u32, u32) -> Hr,
    _tx_get_cached_size: *const c_void,
    _tx_get_natural_size2: *const c_void,
    tx_draw_d2d: unsafe extern "system" fn(*mut c_void, *mut c_void, *const RECT, *mut RECT, i32) -> Hr,
}

type CreateTextServicesFn =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> Hr;

/// Host de texto: responde aos callbacks do serviço sobre uma janela.
#[implement(ITextHost)]
pub struct TextHost {
    hwnd: HWND,
    client: Cell<RECT>,
    dpi: Cell<u32>,
    caret_pos: Cell<(i32, i32)>,
    caret_size: Cell<(i32, i32)>,
    caret_shown: Cell<bool>,
    /// O editor tem o foco: sem foco não há caret visível nem timer de piscar (sem quadros em repouso).
    focused: Cell<bool>,
    caret_phase: Cell<bool>,
    revision: Cell<u32>,
    /// Revisão cujo texto já foi entregue ao modelo, e o último texto entregue (LF).
    delivered_revision: Cell<u32>,
    last_text: std::cell::RefCell<String>,
    /// Mutável só na thread de UI, fora de chamadas do serviço (o tema muda a cor do texto).
    cf: std::cell::UnsafeCell<CHARFORMATW>,
    pf: PARAFORMAT2,
}

impl TextHost {
    /// Liga o timer de piscar só enquanto o caret está visível E o editor tem o foco.
    fn update_blink_timer(&self) {
        unsafe {
            if self.caret_shown.get() && self.focused.get() {
                SetTimer(Some(self.hwnd), CARET_TIMER, 530, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), CARET_TIMER);
            }
        }
    }

    fn invalidate(&self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }
}

#[allow(non_snake_case)]
impl ITextHost_Impl for TextHost_Impl {
    fn TxGetDC(&self) -> HDC {
        unsafe { GetDC(Some(self.hwnd)) }
    }
    fn TxReleaseDC(&self, hdc: HDC) -> i32 {
        unsafe { ReleaseDC(Some(self.hwnd), hdc) }
    }
    fn TxShowScrollBar(&self, _: i32, _: BOOL) -> BOOL {
        BOOL(1)
    }
    fn TxEnableScrollBar(&self, _: SCROLLBAR_CONSTANTS, _: i32) -> BOOL {
        BOOL(1)
    }
    fn TxSetScrollRange(&self, _: i32, _: i32, _: i32, _: BOOL) -> BOOL {
        BOOL(1)
    }
    fn TxSetScrollPos(&self, _: i32, _: i32, _: BOOL) -> BOOL {
        BOOL(1)
    }
    fn TxInvalidateRect(&self, _: *mut RECT, _: BOOL) {
        self.invalidate();
    }
    fn TxViewChange(&self, _: BOOL) {
        self.invalidate();
    }
    fn TxCreateCaret(&self, _: HBITMAP, w: i32, h: i32) -> BOOL {
        self.caret_size.set((w, h));
        BOOL(1)
    }
    fn TxShowCaret(&self, show: BOOL) -> BOOL {
        // O serviço pede o caret na ativação mesmo sem foco; só piscamos (e redesenhamos) com foco.
        self.caret_shown.set(show.as_bool());
        self.caret_phase.set(true);
        self.update_blink_timer();
        self.invalidate();
        BOOL(1)
    }
    fn TxSetCaretPos(&self, x: i32, y: i32) -> BOOL {
        self.caret_pos.set((x, y));
        self.caret_phase.set(true);
        self.invalidate();
        BOOL(1)
    }
    fn TxSetTimer(&self, id: u32, ms: u32) -> BOOL {
        BOOL((unsafe { SetTimer(Some(self.hwnd), id as usize, ms, None) } != 0) as i32)
    }
    fn TxKillTimer(&self, id: u32) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), id as usize);
        }
    }
    fn TxScrollWindowEx(&self, _: i32, _: i32, _: *mut RECT, _: *mut RECT, _: HRGN, _: *mut RECT, _: SCROLL_WINDOW_FLAGS) {
        self.invalidate();
    }
    fn TxSetCapture(&self, capture: BOOL) {
        unsafe {
            if capture.as_bool() {
                SetCapture(self.hwnd);
            } else {
                let _ = ReleaseCapture();
            }
        }
    }
    fn TxSetFocus(&self) {
        unsafe {
            let _ = SetFocus(Some(self.hwnd));
        }
    }
    fn TxSetCursor(&self, hcur: HCURSOR, _: BOOL) {
        unsafe {
            SetCursor(Some(hcur));
        }
    }
    fn TxScreenToClient(&self, pt: *mut POINT) -> BOOL {
        unsafe { ScreenToClient(self.hwnd, pt) }
    }
    fn TxClientToScreen(&self, pt: *mut POINT) -> BOOL {
        unsafe { ClientToScreen(self.hwnd, pt) }
    }
    fn TxActivate(&self, old: *mut i32) -> Result<()> {
        unsafe { *old = 0 };
        Ok(())
    }
    fn TxDeactivate(&self, _: i32) -> Result<()> {
        Ok(())
    }
    fn TxGetClientRect(&self, prc: *mut RECT) -> Result<()> {
        unsafe { *prc = self.client.get() };
        Ok(())
    }
    fn TxGetViewInset(&self, prc: *mut RECT) -> Result<()> {
        unsafe { *prc = RECT::default() };
        Ok(())
    }
    fn TxGetCharFormat(&self, ppcf: *const *const CHARFORMATW) -> Result<()> {
        unsafe { *(ppcf as *mut *const CHARFORMATW) = self.cf.get() };
        Ok(())
    }
    fn TxGetParaFormat(&self, pppf: *const *const PARAFORMAT) -> Result<()> {
        // PARAFORMAT2 começa com PARAFORMAT; cbSize informa a extensão ao Rich Edit.
        unsafe { *(pppf as *mut *const PARAFORMAT) = &self.pf as *const PARAFORMAT2 as *const PARAFORMAT };
        Ok(())
    }
    fn TxGetSysColor(&self, index: SYS_COLOR_INDEX) -> COLORREF {
        COLORREF(unsafe { GetSysColor(index) })
    }
    fn TxGetBackStyle(&self, style: *mut TXTBACKSTYLE) -> Result<()> {
        unsafe { *style = TXTBACK_TRANSPARENT };
        Ok(())
    }
    fn TxGetMaxLength(&self, len: *mut u32) -> Result<()> {
        unsafe { *len = MAX_CHARS };
        Ok(())
    }
    fn TxGetScrollBars(&self, bars: *mut u32) -> Result<()> {
        unsafe { *bars = ES_AUTOVSCROLL };
        Ok(())
    }
    fn TxGetPasswordChar(&self) -> Result<i8> {
        Ok(0)
    }
    fn TxGetAcceleratorPos(&self, pcp: *mut i32) -> Result<()> {
        unsafe { *pcp = -1 };
        Ok(())
    }
    fn TxGetExtent(&self, extent: *mut SIZE) -> Result<()> {
        // HIMETRIC (0,01 mm) do retângulo cliente, no DPI da janela.
        let (r, dpi) = (self.client.get(), self.dpi.get().max(1) as i64);
        let himetric = |px: i32| (px as i64 * 2540 / dpi) as i32;
        unsafe {
            *extent = SIZE { cx: himetric(r.right - r.left), cy: himetric(r.bottom - r.top) };
        }
        Ok(())
    }
    fn OnTxCharFormatChange(&self, _: *const CHARFORMATW) -> Result<()> {
        Ok(())
    }
    fn OnTxParaFormatChange(&self, _: *const PARAFORMAT) -> Result<()> {
        Ok(())
    }
    fn TxGetPropertyBits(&self, mask: u32, bits: *mut u32) -> Result<()> {
        // Texto simples, multilinha, com quebra; sem rich text, OLE nem drag and drop.
        let on = TXTBIT_MULTILINE
            | TXTBIT_WORDWRAP
            | TXTBIT_SAVESELECTION
            | TXTBIT_DISABLEDRAG
            | TXTBIT_D2DDWRITE
            | TXTBIT_D2DPIXELSNAPPED;
        unsafe { *bits = on & mask };
        Ok(())
    }
    fn TxNotify(&self, code: u32, _: *mut c_void) -> Result<()> {
        if code == EN_CHANGE {
            self.revision.set(self.revision.get().wrapping_add(1));
        }
        Ok(())
    }
    fn TxImmGetContext(&self) -> HIMC {
        unsafe { ImmGetContext(self.hwnd) }
    }
    fn TxImmReleaseContext(&self, himc: HIMC) {
        unsafe {
            let _ = ImmReleaseContext(self.hwnd, himc);
        }
    }
    fn TxGetSelectionBarWidth(&self, w: *mut i32) -> Result<()> {
        unsafe { *w = 0 };
        Ok(())
    }
}

/// Editor de uma janela: serviço de texto + host. Clone = novo AddRef nos dois.
#[derive(Clone)]
pub struct TextEditor {
    services: windows::core::IUnknown,
    host: ComObject<TextHost>,
    _host_if: ITextHost,
}

impl TextEditor {
    pub fn new(hwnd: HWND, client: RECT, dpi: u32, initial: &str, color: [u8; 3], font_dip: f32) -> Result<Self> {
        let mut cf = CHARFORMATW {
            cbSize: std::mem::size_of::<CHARFORMATW>() as u32,
            dwMask: CFM_COLOR | CFM_FACE | CFM_SIZE,
            yHeight: dip_to_twips(font_dip),
            crTextColor: colorref(color),
            ..Default::default()
        };
        for (d, s) in cf.szFaceName.iter_mut().zip(crate::gfx::ui_font_family().encode_utf16()) {
            *d = s;
        }
        let pf = PARAFORMAT2 {
            Base: PARAFORMAT {
                cbSize: std::mem::size_of::<PARAFORMAT2>() as u32,
                dwMask: PFM_ALIGNMENT | PFM_LINESPACING,
                wAlignment: PFA_LEFT,
                ..Default::default()
            },
            // 24/20 = 1,2 linhas nativas. Escala com a fonte sem reformatar parágrafos,
            // operação recusada pelo Rich Edit em texto simples. Não corta glifos de fallback.
            dyLineSpacing: 24,
            bLineSpacingRule: 5,
            ..Default::default()
        };
        let host = ComObject::new(TextHost {
            hwnd,
            client: Cell::new(client),
            dpi: Cell::new(dpi),
            caret_pos: Cell::new((0, 0)),
            caret_size: Cell::new((1, 16)),
            caret_shown: Cell::new(false),
            focused: Cell::new(false),
            caret_phase: Cell::new(true),
            revision: Cell::new(0),
            delivered_revision: Cell::new(0),
            last_text: std::cell::RefCell::new(String::new()),
            cf: std::cell::UnsafeCell::new(cf),
            pf,
        });
        let host_if: ITextHost = host.to_interface();
        unsafe {
            // Carrega do diretório de sistema, nunca do diretório do aplicativo.
            let lib = LoadLibraryExW(windows::core::w!("msftedit.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32)?;
            let create = GetProcAddress(lib, PCSTR(b"CreateTextServices\0".as_ptr()))
                .ok_or_else(windows::core::Error::from_thread)?;
            let create: CreateTextServicesFn = std::mem::transmute(create);
            let mut unk: *mut c_void = std::ptr::null_mut();
            create(std::ptr::null_mut(), host_if.as_raw(), &mut unk).ok()?;
            let unk = windows::core::IUnknown::from_raw(unk);
            let mut ts2: *mut c_void = std::ptr::null_mut();
            unk.query(&IID_ITEXTSERVICES2, &mut ts2).ok()?;
            let services = windows::core::IUnknown::from_raw(ts2);
            let ed = Self { services, host, _host_if: host_if };
            ed.send(EM_SETEVENTMASK, WPARAM(0), LPARAM(ENM_CHANGE));
            ed.send(EM_SETLANGOPTIONS, WPARAM(0), LPARAM(LANG_OPTIONS));
            ed.send(EM_SETTYPOGRAPHYOPTIONS, WPARAM(TO_ADVANCEDTYPOGRAPHY), LPARAM(TO_ADVANCEDTYPOGRAPHY as isize));
            ed.property_changed(TXTBIT_CLIENTRECTCHANGE | TXTBIT_EXTENTCHANGE | TXTBIT_VIEWINSETCHANGE);
            (ed.vtbl().on_tx_inplace_activate)(ed.raw(), &client).ok()?;
            ed.set_text_silent(initial)?;
            Ok(ed)
        }
    }

    fn raw(&self) -> *mut c_void {
        self.services.as_raw()
    }
    unsafe fn vtbl(&self) -> &ServicesVtbl {
        &**(self.raw() as *const *const ServicesVtbl)
    }

    /// Encaminha uma mensagem ao serviço. `None` = não tratada (use DefWindowProc).
    pub fn send(&self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        let mut res = LRESULT(0);
        let hr = unsafe { (self.vtbl().tx_send_message)(self.raw(), msg, wp, lp, &mut res) };
        (hr == S_OK).then_some(res)
    }

    fn property_changed(&self, bits: u32) {
        unsafe {
            let _ = (self.vtbl().on_tx_property_bits_change)(self.raw(), bits, bits);
        }
    }

    /// Substitui todo o texto, em bloco (`EM_SETTEXTEX`, Unicode). Limitação conhecida: emoji e
    /// símbolos inseridos assim aparecem como caixas; CJK e acentos renderizam. Digitados pelo
    /// teclado, os emoji renderizam. Ver docs/validation/m4-quick-notes.md.
    pub fn replace_text(&self, text: &str) -> Result<()> {
        const EM_SETTEXTEX: u32 = 0x0461;
        const CP_UTF16: u32 = 1200;
        #[repr(C)]
        struct SetTextEx {
            flags: u32,
            codepage: u32,
        }
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let st = SetTextEx { flags: 0, codepage: CP_UTF16 };
        match self.send(EM_SETTEXTEX, WPARAM(&st as *const _ as usize), LPARAM(wide.as_ptr() as isize)) {
            Some(r) if r.0 != 0 => Ok(()),
            _ => Err(windows::core::Error::from(E_FAIL)),
        }
    }

    /// Define o texto sem que isso conte como edição do usuário (restauração, recusa de limite) e
    /// sem entrar no histórico de desfazer.
    pub fn set_text_silent(&self, text: &str) -> Result<()> {
        const EM_EMPTYUNDOBUFFER: u32 = 0x00CD;
        self.replace_text(text)?;
        self.send(EM_EMPTYUNDOBUFFER, WPARAM(0), LPARAM(0));
        self.mark_delivered();
        Ok(())
    }

    pub fn text(&self) -> String {
        let mut b = BSTR::new();
        unsafe {
            let _ = (self.vtbl().tx_get_text)(self.raw(), &mut b);
        }
        b.to_string()
    }

    /// Texto com quebras de linha LF (o serviço de texto usa CR internamente).
    pub fn text_lf(&self) -> String {
        normalize_newlines(&self.text())
    }

    /// Marca o texto atual como já conhecido pelo modelo.
    pub fn mark_delivered(&self) {
        self.host.delivered_revision.set(self.host.revision.get());
        *self.host.last_text.borrow_mut() = self.text_lf();
    }

    /// Texto confirmado se mudou desde a última entrega. Mudanças só de formatação (troca de fonte
    /// para emoji) incrementam a revisão mas não o texto, então não viram edição.
    pub fn take_text_change(&self) -> Option<String> {
        if self.host.revision.get() == self.host.delivered_revision.get() {
            return None;
        }
        self.host.delivered_revision.set(self.host.revision.get());
        let text = self.text_lf();
        if *self.host.last_text.borrow() == text {
            return None;
        }
        *self.host.last_text.borrow_mut() = text.clone();
        Some(text)
    }

    /// Troca a cor do texto (mudança de tema): novo texto e o já existente.
    pub fn set_text_color(&self, color: [u8; 3]) {
        const EM_SETCHARFORMAT: u32 = 0x0444;
        const SCF_ALL: usize = 4;
        let c = colorref(color);
        // SAFETY: thread de UI, fora de qualquer chamada do serviço de texto.
        unsafe { (*self.host.cf.get()).crTextColor = c };
        // Só a cor: tocar na fonte quebra o fallback de emoji (ver docs/validation/m1-text-fallback.md).
        let mut cf = CHARFORMATW {
            cbSize: std::mem::size_of::<CHARFORMATW>() as u32,
            dwMask: CFM_COLOR,
            crTextColor: c,
            ..Default::default()
        };
        self.send(EM_SETCHARFORMAT, WPARAM(SCF_ALL), LPARAM(&mut cf as *mut _ as isize));
        self.host.invalidate();
    }

    pub fn client(&self) -> RECT {
        self.host.client.get()
    }

    /// Altera apenas tamanho do host de texto simples. A entrelinha é proporcional à fonte.
    /// Não troca texto, seleção,
    /// face dos runs de fallback nem limpa undo. Só na thread de UI, fora de composição IME.
    pub fn set_font_size(&self, font_dip: f32) {
        let font = dip_to_twips(font_dip);
        // SAFETY: ponteiros estáveis do host COM; a mutação termina antes de reentrar no serviço.
        let changed = unsafe {
            let cf = &mut *self.host.cf.get();
            if cf.yHeight == font { false } else {
                cf.yHeight = font;
                true
            }
        };
        if changed {
            // Somente CFM_SIZE: a face de cada run (inclusive fallback de emoji/CJK) fica intacta.
            const EM_SETCHARFORMAT: u32 = 0x0444;
            let cf = CHARFORMATW {
                cbSize: std::mem::size_of::<CHARFORMATW>() as u32,
                dwMask: CFM_SIZE,
                yHeight: font,
                ..Default::default()
            };
            self.send(EM_SETCHARFORMAT, WPARAM(4 /* SCF_ALL */), LPARAM(&cf as *const _ as isize));
            self.host.invalidate();
        }
    }

    pub fn set_client(&self, client: RECT, dpi: u32) {
        let dpi_changed = self.host.dpi.get() != dpi;
        self.host.client.set(client);
        self.host.dpi.set(dpi);
        self.property_changed(
            TXTBIT_CLIENTRECTCHANGE | TXTBIT_EXTENTCHANGE | TXTBIT_VIEWINSETCHANGE,
        );
        if dpi_changed {
            // Reavalia métricas no DC do novo monitor sem reaplicar CFM_FACE aos runs.
            let cf = CHARFORMATW {
                cbSize: std::mem::size_of::<CHARFORMATW>() as u32,
                dwMask: CFM_SIZE,
                // SAFETY: leitura na thread de UI; nenhuma mutação atravessa a chamada abaixo.
                yHeight: unsafe { (*self.host.cf.get()).yHeight },
                ..Default::default()
            };
            self.send(0x0444 /* EM_SETCHARFORMAT */, WPARAM(4 /* SCF_ALL */), LPARAM(&cf as *const _ as isize));
        }
    }

    pub fn revision(&self) -> u32 {
        self.host.revision.get()
    }

    /// Foco do editor: liga/desliga o caret e o timer de piscar.
    pub fn set_focused(&self, focused: bool) {
        self.host.focused.set(focused);
        self.host.caret_phase.set(true);
        self.host.update_blink_timer();
        self.host.invalidate();
    }

    pub fn toggle_caret_phase(&self) {
        self.host.caret_phase.set(!self.host.caret_phase.get());
        self.host.invalidate();
    }

    /// Desenha texto no alvo D2D (pixels físicos: o chamador deve ter DPI 96 no contexto).
    pub fn draw(&self, ctx: &ID2D1DeviceContext) -> Result<()> {
        let bounds = self.host.client.get();
        unsafe {
            let rt: windows::Win32::Graphics::Direct2D::ID2D1RenderTarget = ctx.cast()?;
            let mut upd = bounds;
            (self.vtbl().tx_draw_d2d)(self.raw(), rt.as_raw(), &bounds, &mut upd, 0).ok()
        }
    }

    /// Retângulo do caret em pixels do cliente, se visível neste instante do piscar.
    pub fn caret_rect(&self) -> Option<RECT> {
        let h = &self.host;
        if !(h.caret_shown.get() && h.focused.get() && h.caret_phase.get()) {
            return None;
        }
        let ((x, y), (w, ch)) = (h.caret_pos.get(), h.caret_size.get());
        Some(RECT { left: x, top: y, right: x + w.max(1), bottom: y + ch })
    }

    pub fn deactivate(&self) {
        unsafe {
            let _ = (self.vtbl().on_tx_inplace_deactivate)(self.raw());
        }
    }
}

fn colorref(c: [u8; 3]) -> COLORREF {
    COLORREF(c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16)
}

fn dip_to_twips(dip: f32) -> i32 {
    // 1440 twips/polegada ÷ 96 DIPs/polegada. O DPI físico é aplicado pelo serviço de texto.
    (dip * 15.0).round() as i32
}

pub fn normalize_newlines(raw: &str) -> String {
    raw.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::normalize_newlines;

    #[test]
    fn editorial_resize_preserves_text_selection_and_undo_in_native_richedit() {
        use super::*;
        struct NativeHost(HWND);
        impl Drop for NativeHost {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                    windows::Win32::System::Ole::OleUninitialize();
                }
            }
        }
        unsafe {
            windows::Win32::System::Ole::OleInitialize(None).unwrap();
            let host = NativeHost(CreateWindowExW(
                WINDOW_EX_STYLE::default(), windows::core::w!("STATIC"), windows::core::w!("Pod text test"),
                WS_POPUP, 0, 0, 600, 600, None, None, Some(GetModuleHandleW(None).unwrap().into()), None,
            ).unwrap());
            let bounds = RECT { left: 28, top: 78, right: 360, bottom: 352 };
            let ed = TextEditor::new(host.0, bounds, 96, "ação\n日本語", [37, 43, 40], 28.0).unwrap();
            let mut end = CHARRANGE { cpMin: -1, cpMax: -1 };
            ed.send(0x0437 /* EM_EXSETSEL */, WPARAM(0), LPARAM(&mut end as *mut _ as isize));
            let inserted = windows::core::HSTRING::from(" café");
            ed.send(0x00C2 /* EM_REPLACESEL */, WPARAM(1), LPARAM(inserted.as_ptr() as isize));
            let expected = ed.text_lf();
            assert!(expected.ends_with(" café"));
            let mut selection = CHARRANGE { cpMin: 1, cpMax: 4 };
            ed.send(0x0437, WPARAM(0), LPARAM(&mut selection as *mut _ as isize));
            ed.set_client(RECT { right: 164, bottom: 140, ..bounds }, 144);
            ed.set_font_size(16.0);
            ed.set_text_color([241, 241, 233]);
            let mut after = CHARRANGE::default();
            ed.send(0x0434 /* EM_EXGETSEL */, WPARAM(0), LPARAM(&mut after as *mut _ as isize));
            assert_eq!((after.cpMin, after.cpMax), (1, 4));
            assert_eq!(ed.text_lf(), expected);
            let mut format = CHARFORMATW { cbSize: std::mem::size_of::<CHARFORMATW>() as u32, ..Default::default() };
            ed.send(0x043A /* EM_GETCHARFORMAT */, WPARAM(1), LPARAM(&mut format as *mut _ as isize));
            assert_eq!(format.yHeight, 240, "16 DIPs = 12 pt, independente do DPI físico");
            let mut paragraph = PARAFORMAT2 {
                Base: PARAFORMAT { cbSize: std::mem::size_of::<PARAFORMAT2>() as u32, ..Default::default() },
                ..Default::default()
            };
            ed.send(0x043D /* EM_GETPARAFORMAT */, WPARAM(0), LPARAM(&mut paragraph as *mut _ as isize));
            assert_eq!(paragraph.dyLineSpacing, 24, "entrelinha proporcional de 1,2 linhas");
            assert_eq!(paragraph.bLineSpacingRule, 5);
            ed.send(0x00C7 /* EM_UNDO */, WPARAM(0), LPARAM(0));
            assert_eq!(ed.text_lf(), "ação\n日本語", "resize não descarta nem acrescenta uma ação ao undo");
            ed.deactivate();
        }
    }

    #[test]
    fn newlines_are_normalized_to_lf() {
        assert_eq!(normalize_newlines("a\rb\r\nc\nd"), "a\nb\nc\nd");
        assert_eq!(normalize_newlines("sem quebra"), "sem quebra");
    }

    #[test]
    fn editorial_long_note_scrolls_without_losing_text_or_caret_on_resize() {
        use super::*;
        use windows::Win32::UI::Controls::EM_SCROLLCARET;
        struct NativeHost(HWND);
        impl Drop for NativeHost {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                    windows::Win32::System::Ole::OleUninitialize();
                }
            }
        }
        unsafe {
            windows::Win32::System::Ole::OleInitialize(None).unwrap();
            let host = NativeHost(CreateWindowExW(
                WINDOW_EX_STYLE::default(), windows::core::w!("STATIC"), windows::core::w!("Pod scroll test"),
                WS_POPUP, 0, 0, 600, 600, None, None, Some(GetModuleHandleW(None).unwrap().into()), None,
            ).unwrap());
            let text = (1..=100).map(|n| format!("Linha {n}: café e ação")).collect::<Vec<_>>().join("\n");
            let compact = RECT { left: 16, top: 44, right: 172, bottom: 148 };
            let ed = TextEditor::new(host.0, compact, 96, &text, [37, 43, 40], 16.0).unwrap();
            let mut end = CHARRANGE { cpMin: -1, cpMax: -1 };
            ed.send(EM_EXSETSEL, WPARAM(0), LPARAM(&mut end as *mut _ as isize));
            ed.send(EM_SCROLLCARET, WPARAM(0), LPARAM(0));
            let mut position = POINT::default();
            ed.send(EM_GETSCROLLPOS, WPARAM(0), LPARAM(&mut position as *mut _ as isize));
            assert!(position.y > 0, "a nota longa precisa rolar até o caret");
            let mut before = CHARRANGE::default();
            ed.send(EM_EXGETSEL, WPARAM(0), LPARAM(&mut before as *mut _ as isize));
            ed.set_client(RECT { left: 28, top: 78, right: 360, bottom: 352 }, 96);
            ed.set_font_size(28.0);
            ed.send(EM_SCROLLCARET, WPARAM(0), LPARAM(0));
            let mut after = CHARRANGE::default();
            ed.send(EM_EXGETSEL, WPARAM(0), LPARAM(&mut after as *mut _ as isize));
            assert_eq!((after.cpMin, after.cpMax), (before.cpMin, before.cpMax));
            assert_eq!(ed.text_lf(), text);
            ed.deactivate();
        }
    }

}
