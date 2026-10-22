//! Painéis de edição com controles nativos do Windows (lista, campos de texto, botões): Tab, Enter
//! (Adicionar/OK), Esc, IME e fonte do sistema vêm do próprio sistema.
//!
//! - Painel de dados compartilhados (eventos, tarefas, links, encomendas): uma janela por tipo, guiada
//!   por uma tabela (`spec`); edições passam por `Shell::edit_shared`.
//! - Janelinha de entrada de texto (`open_input`): um campo e OK/Cancelar, para um widget pedir um valor.
//!   A resposta vai ao widget como `Event::TextChanged`; ele pode recusar e a janela mostra o motivo.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::core::{w, Result, BOOL, HSTRING};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::host;
use crate::shell::Shell;
use pod_model::ids::WidgetId;
use pod_model::shared::{Date, SharedError, MAX_TITLE_CHARS, MAX_URL_CHARS};
use pod_model::widget::{Event, InputSpec, LocalId, PanelKind};

const ID_ADD: i32 = 1; // IDOK: Enter nos campos aciona "Adicionar"/"OK"
const ID_CLOSE: i32 = 2; // IDCANCEL: Esc fecha
const ID_REMOVE: i32 = 102;
const ID_TOGGLE: i32 = 103;
const ID_LIST: i32 = 104;
const ID_FIELD0: i32 = 110;
const WM_PANEL_REFRESH: u32 = WM_APP + 21;
const EM_SETLIMITTEXT: u32 = 0x00C5;
const EM_SETCUEBANNER: u32 = 0x1501;
const EM_SETSEL: u32 = 0x00B1;

struct Field {
    label: &'static str,
    cue: &'static str,
    limit: usize,
    /// Largura em DIPs; 0 = o que sobrar da linha.
    width: i32,
}

struct Spec {
    title: &'static str,
    fields: &'static [Field],
    toggle: Option<&'static str>,
    remove: &'static str,
}

fn spec(kind: PanelKind) -> Spec {
    match kind {
        PanelKind::Events => Spec {
            title: "Eventos do calend\u{e1}rio",
            fields: &[
                Field { label: "Data (dia/m\u{ea}s/ano)", cue: "18/10/2026", limit: 10, width: 132 },
                Field { label: "T\u{ed}tulo", cue: "Ex.: Reuni\u{e3}o", limit: MAX_TITLE_CHARS, width: 0 },
            ],
            toggle: None,
            remove: "Remover selecionado",
        },
        PanelKind::Tasks => Spec {
            title: "Tarefas",
            fields: &[Field { label: "Nova tarefa", cue: "", limit: MAX_TITLE_CHARS, width: 0 }],
            toggle: Some("Alternar conclu\u{ed}da"),
            remove: "Remover",
        },
        PanelKind::Links => Spec {
            title: "Favoritos",
            fields: &[
                Field { label: "T\u{ed}tulo", cue: "Meu site", limit: MAX_TITLE_CHARS, width: 150 },
                Field { label: "Endere\u{e7}o (http ou https)", cue: "exemplo.com", limit: MAX_URL_CHARS, width: 0 },
            ],
            toggle: None,
            remove: "Remover selecionado",
        },
        PanelKind::Parcels => Spec {
            title: "Encomendas",
            fields: &[
                Field { label: "Descri\u{e7}\u{e3}o", cue: "Ex.: Fone novo", limit: MAX_TITLE_CHARS, width: 190 },
                Field { label: "C\u{f3}digo de rastreio", cue: "AA123456789BR", limit: 30, width: 0 },
            ],
            toggle: Some("Marcar entregue"),
            remove: "Remover",
        },
    }
}

thread_local! {
    /// Painéis abertos (um por tipo) e janelinhas de entrada (uma por instância).
    static OPEN: RefCell<Vec<(PanelKind, HWND)>> = const { RefCell::new(Vec::new()) };
    static INPUTS: RefCell<Vec<(WidgetId, HWND)>> = const { RefCell::new(Vec::new()) };
}

struct Panel {
    shell: Rc<Shell>,
    kind: PanelKind,
    list: HWND,
    edits: Vec<HWND>,
    status: HWND,
    /// Id (de `SharedData`) de cada linha da lista.
    keys: Vec<u64>,
    font: HFONT,
}

fn send(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { SendMessageW(h, m, Some(w), Some(l)) }
}

fn text_of(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h).max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        let got = GetWindowTextW(h, &mut buf).max(0) as usize;
        String::from_utf16_lossy(&buf[..got])
    }
}

/// Abre (ou foca) o painel do tipo pedido.
pub fn open(shell: &Rc<Shell>, kind: PanelKind) {
    if let Some(h) = OPEN.with(|o| o.borrow().iter().find(|(k, _)| *k == kind).map(|(_, h)| *h)) {
        unsafe {
            let _ = ShowWindow(h, SW_RESTORE);
            let _ = SetForegroundWindow(h);
        }
        return;
    }
    if let Err(e) = unsafe { create(shell, kind) } {
        host::log(&format!("painel n\u{e3}o aberto: {e}"));
    }
}

/// Abre (ou foca) a janelinha de entrada de texto de uma instância.
pub fn open_input(shell: &Rc<Shell>, id: WidgetId, spec: InputSpec) {
    if let Some(h) = INPUTS.with(|o| o.borrow().iter().find(|(i, _)| *i == id).map(|(_, h)| *h)) {
        unsafe {
            let _ = SetForegroundWindow(h);
        }
        return;
    }
    if let Err(e) = unsafe { create_input(shell, id, spec) } {
        host::log(&format!("entrada n\u{e3}o aberta: {e}"));
    }
}

/// Repassa a mensagem ao diálogo que a deve tratar (Tab, Enter, Esc).
pub fn is_dialog_message(msg: &MSG) -> bool {
    let mut handles: Vec<HWND> = OPEN.with(|o| o.borrow().iter().map(|(_, h)| *h).collect());
    handles.extend(INPUTS.with(|o| o.borrow().iter().map(|(_, h)| *h).collect::<Vec<_>>()));
    handles.into_iter().any(|h| unsafe { IsDialogMessageW(h, msg).as_bool() })
}

/// Os dados mudaram (por um widget ou por este painel): atualiza as listas abertas.
pub fn refresh_all() {
    let handles: Vec<HWND> = OPEN.with(|o| o.borrow().iter().map(|(_, h)| *h).collect());
    for h in handles {
        unsafe {
            let _ = PostMessageW(Some(h), WM_PANEL_REFRESH, WPARAM(0), LPARAM(0));
        }
    }
}

struct Layout {
    dpi: u32,
}

impl Layout {
    fn px(&self, dip: i32) -> i32 {
        (dip as i64 * self.dpi as i64 / 96) as i32
    }
}

unsafe fn monitor_dpi_and_work() -> (u32, RECT) {
    let mut cur = POINT::default();
    let _ = GetCursorPos(&mut cur);
    let mon = MonitorFromPoint(cur, MONITOR_DEFAULTTOPRIMARY);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(mon, &mut mi);
    let (mut dx, mut dy) = (0u32, 0u32);
    let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
    (dx.max(96), mi.rcWork)
}

unsafe fn register(class: windows::core::PCWSTR) -> Result<HINSTANCE> {
    let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as isize as *mut _),
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassExW(&wc); // 0 se já registrada: tudo bem
    Ok(hinst)
}

unsafe fn make_font(l: &Layout) -> HFONT {
    CreateFontW(
        -l.px(15),
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        FONT_CHARSET(1),
        FONT_OUTPUT_PRECISION(0),
        FONT_CLIP_PRECISION(0),
        FONT_QUALITY(5),
        0,
        &HSTRING::from(crate::gfx::ui_font_family()),
    )
}

unsafe fn center_window(l: &Layout, work: RECT, style: WINDOW_STYLE, cw: i32, ch: i32) -> (i32, i32, i32, i32) {
    let mut rc = RECT { left: 0, top: 0, right: l.px(cw), bottom: l.px(ch) };
    let _ = AdjustWindowRectExForDpi(&mut rc, style, false, WS_EX_APPWINDOW, l.dpi);
    let (ww, wh) = (rc.right - rc.left, rc.bottom - rc.top);
    (work.left + (work.right - work.left - ww) / 2, work.top + (work.bottom - work.top - wh) / 2, ww, wh)
}

unsafe fn create(shell: &Rc<Shell>, kind: PanelKind) -> Result<()> {
    let sp = spec(kind);
    let hinst = register(w!("PodPanel"))?;
    let (dpi, work) = monitor_dpi_and_work();
    let l = Layout { dpi };
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let (x, y, ww, wh) = center_window(&l, work, style, 460, 392);
    let hwnd = CreateWindowExW(WS_EX_APPWINDOW, w!("PodPanel"), &HSTRING::from(sp.title), style, x, y, ww, wh, None, None, Some(hinst), None)?;

    let child = |cls: windows::core::PCWSTR, text: &str, st: WINDOW_STYLE, ex: WINDOW_EX_STYLE, r: (i32, i32, i32, i32), id: i32| {
        CreateWindowExW(
            ex,
            cls,
            &HSTRING::from(text),
            WS_CHILD | WS_VISIBLE | st,
            l.px(r.0),
            l.px(r.1),
            l.px(r.2),
            l.px(r.3),
            Some(hwnd),
            Some(HMENU(id as isize as *mut _)),
            Some(hinst),
            None,
        )
    };
    let none = WINDOW_EX_STYLE(0);
    let tab = WS_TABSTOP;
    let list = child(
        w!("LISTBOX"),
        "",
        WS_VSCROLL | tab | WINDOW_STYLE(LBS_NOTIFY as u32) | WINDOW_STYLE(LBS_NOINTEGRALHEIGHT as u32),
        WS_EX_CLIENTEDGE,
        (12, 12, 436, 188),
        ID_LIST,
    )?;

    // Campos lado a lado: os de largura fixa primeiro, o último ocupa o resto da linha.
    let mut edits = Vec::new();
    let mut fx = 12;
    for (i, f) in sp.fields.iter().enumerate() {
        let wdt = if f.width > 0 { f.width } else { 448 - fx };
        let _ = child(w!("STATIC"), f.label, WINDOW_STYLE(0), none, (fx, 212, wdt, 18), 0)?;
        let e = child(w!("EDIT"), "", tab | WINDOW_STYLE(ES_AUTOHSCROLL as u32), WS_EX_CLIENTEDGE, (fx, 232, wdt, 24), ID_FIELD0 + i as i32)?;
        send(e, EM_SETLIMITTEXT, WPARAM(f.limit), LPARAM(0));
        if !f.cue.is_empty() {
            let cue = HSTRING::from(f.cue);
            send(e, EM_SETCUEBANNER, WPARAM(1), LPARAM(cue.as_ptr() as isize));
        }
        edits.push(e);
        fx += wdt + 12;
    }

    let by = 272;
    let _ = child(w!("BUTTON"), "Adicionar", tab | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32), none, (12, by, 100, 28), ID_ADD)?;
    let mut bx = 120;
    if let Some(t) = sp.toggle {
        let _ = child(w!("BUTTON"), t, tab | WINDOW_STYLE(BS_PUSHBUTTON as u32), none, (bx, by, 136, 28), ID_TOGGLE)?;
        bx += 144;
    }
    let rw = if sp.toggle.is_some() { 88 } else { 150 };
    let _ = child(w!("BUTTON"), sp.remove, tab | WINDOW_STYLE(BS_PUSHBUTTON as u32), none, (bx, by, rw, 28), ID_REMOVE)?;
    let _ = child(w!("BUTTON"), "Fechar", tab | WINDOW_STYLE(BS_PUSHBUTTON as u32), none, (358, by, 90, 28), ID_CLOSE)?;
    let status = child(w!("STATIC"), "", WINDOW_STYLE(0), none, (12, by + 36, 436, 36), 0)?;

    let font = make_font(&l);
    let _ = EnumChildWindows(Some(hwnd), Some(set_font_cb), LPARAM(font.0 as isize));
    let panel = Box::new(Panel { shell: shell.clone(), kind, list, edits: edits.clone(), status, keys: Vec::new(), font });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(panel) as isize);
    OPEN.with(|o| o.borrow_mut().push((kind, hwnd)));
    refresh(hwnd);
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetFocus(Some(edits[0]));
    Ok(())
}

unsafe extern "system" fn set_font_cb(h: HWND, lp: LPARAM) -> BOOL {
    send(h, WM_SETFONT, WPARAM(lp.0 as usize), LPARAM(1));
    BOOL(1)
}

unsafe fn panel<'a>(hwnd: HWND) -> Option<&'a mut Panel> {
    if is_input(hwnd) {
        return None;
    }
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Panel).as_mut()
}

fn is_input(hwnd: HWND) -> bool {
    INPUTS.with(|o| o.borrow().iter().any(|(_, h)| *h == hwnd))
}

fn set_status(p: &Panel, msg: &str) {
    unsafe {
        let _ = SetWindowTextW(p.status, &HSTRING::from(msg));
    }
}

fn row_text(kind: PanelKind, shared: &pod_model::shared::SharedData) -> Vec<(u64, String)> {
    match kind {
        PanelKind::Events => shared.events().iter().map(|e| (e.id, format!("{}    {}", e.date.format(), e.title))).collect(),
        PanelKind::Tasks => shared
            .tasks()
            .iter()
            .map(|t| (t.id, format!("{} {}", if t.done { "[x]" } else { "[ ]" }, t.title)))
            .collect(),
        PanelKind::Links => shared.links().iter().map(|l| (l.id, format!("{}    {}", l.title, l.url))).collect(),
        PanelKind::Parcels => shared
            .parcels()
            .iter()
            .map(|p| (p.id, format!("{} {}    {}", if p.delivered { "[x]" } else { "[ ]" }, p.label, p.code)))
            .collect(),
    }
}

/// Recarrega a lista a partir dos dados atuais, mantendo a seleção pelo id.
unsafe fn refresh(hwnd: HWND) {
    let Some(p) = panel(hwnd) else { return };
    let selected = selected_id(p);
    let rows = row_text(p.kind, p.shell.runtime.borrow().shared());
    send(p.list, WM_SETREDRAW, WPARAM(0), LPARAM(0));
    send(p.list, LB_RESETCONTENT, WPARAM(0), LPARAM(0));
    p.keys.clear();
    for (id, text) in &rows {
        let h = HSTRING::from(text.as_str());
        send(p.list, LB_ADDSTRING, WPARAM(0), LPARAM(h.as_ptr() as isize));
        p.keys.push(*id);
    }
    if let Some(sel) = selected.and_then(|id| p.keys.iter().position(|k| *k == id)) {
        send(p.list, LB_SETCURSEL, WPARAM(sel), LPARAM(0));
    }
    send(p.list, WM_SETREDRAW, WPARAM(1), LPARAM(0));
    let _ = InvalidateRect(Some(p.list), None, true);
}

unsafe fn selected_id(p: &Panel) -> Option<u64> {
    let i = send(p.list, LB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
    (i >= 0).then(|| p.keys.get(i as usize).copied()).flatten()
}

/// Índice do campo a destacar para cada tipo de erro.
fn field_for_error(kind: PanelKind, e: &SharedError) -> usize {
    match (kind, e) {
        (PanelKind::Events, SharedError::BadDate) => 0,
        (PanelKind::Events, _) => 1,
        (PanelKind::Links, SharedError::BadUrl) | (PanelKind::Parcels, SharedError::BadCode) => 1,
        _ => 0,
    }
}

unsafe fn command(hwnd: HWND, id: i32) {
    let Some(p) = panel(hwnd) else { return };
    let shell = p.shell.clone();
    let kind = p.kind;
    match id {
        ID_ADD => {
            let t: Vec<String> = p.edits.iter().map(|e| text_of(*e)).collect();
            let result: std::result::Result<(), SharedError> = match kind {
                PanelKind::Events => match Date::parse(&t[0]) {
                    None => Err(SharedError::BadDate),
                    Some(d) => shell.edit_shared(|s| s.add_event(d, &t[1])).map(|_| ()),
                },
                PanelKind::Tasks => shell.edit_shared(|s| s.add_task(&t[0])).map(|_| ()),
                PanelKind::Links => shell.edit_shared(|s| s.add_link(&t[0], &t[1])).map(|_| ()),
                PanelKind::Parcels => shell.edit_shared(|s| s.add_parcel(&t[0], &t[1])).map(|_| ()),
            };
            match result {
                Ok(()) => {
                    for e in &p.edits {
                        let _ = SetWindowTextW(*e, w!(""));
                    }
                    set_status(p, "");
                    let _ = SetFocus(Some(p.edits[0]));
                }
                Err(e) => {
                    set_status(p, &e.to_string());
                    let _ = SetFocus(Some(p.edits[field_for_error(kind, &e).min(p.edits.len() - 1)]));
                }
            }
        }
        ID_REMOVE => match selected_id(p) {
            Some(sel) => {
                shell.edit_shared(|s| match kind {
                    PanelKind::Events => s.remove_event(sel),
                    PanelKind::Tasks => s.remove_task(sel),
                    PanelKind::Links => s.remove_link(sel),
                    PanelKind::Parcels => s.remove_parcel(sel),
                });
                set_status(p, "");
            }
            None => set_status(p, "Selecione um item da lista."),
        },
        ID_TOGGLE => match selected_id(p) {
            Some(sel) => {
                shell.edit_shared(|s| match kind {
                    PanelKind::Parcels => s.toggle_parcel(sel),
                    _ => s.toggle_task(sel),
                });
                set_status(p, "");
            }
            None => set_status(p, "Selecione um item da lista."),
        },
        ID_CLOSE => {
            let _ = DestroyWindow(hwnd);
        }
        _ => {}
    }
}

// ----------------------------------------------------------------------------- entrada de texto

struct InputPanel {
    shell: Rc<Shell>,
    id: WidgetId,
    edit: HWND,
    status: HWND,
    font: HFONT,
    /// O widget recusou a última entrada (a janela fica aberta).
    refused: Cell<bool>,
    /// Campo de segredo: o texto vai ao cofre do Windows, nunca ao widget.
    secret: bool,
}

unsafe fn create_input(shell: &Rc<Shell>, id: WidgetId, spec: InputSpec) -> Result<()> {
    let hinst = register(w!("PodInput"))?;
    let (dpi, work) = monitor_dpi_and_work();
    let l = Layout { dpi };
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
    let (x, y, ww, wh) = center_window(&l, work, style, 340, 150);
    let hwnd = CreateWindowExW(WS_EX_APPWINDOW, w!("PodInput"), &HSTRING::from(spec.title.as_str()), style, x, y, ww, wh, None, None, Some(hinst), None)?;
    let child = |cls: windows::core::PCWSTR, text: &str, st: WINDOW_STYLE, ex: WINDOW_EX_STYLE, r: (i32, i32, i32, i32), cid: i32| {
        CreateWindowExW(ex, cls, &HSTRING::from(text), WS_CHILD | WS_VISIBLE | st, l.px(r.0), l.px(r.1), l.px(r.2), l.px(r.3), Some(hwnd), Some(HMENU(cid as isize as *mut _)), Some(hinst), None)
    };
    let none = WINDOW_EX_STYLE(0);
    let _ = child(w!("STATIC"), &spec.label, WINDOW_STYLE(0), none, (12, 12, 316, 18), 0)?;
    let edit = child(w!("EDIT"), &spec.initial, WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32 | if spec.secret { ES_PASSWORD as u32 } else { 0 }), WS_EX_CLIENTEDGE, (12, 32, 316, 24), ID_FIELD0)?;
    send(edit, EM_SETLIMITTEXT, WPARAM(if spec.secret { 255 } else { 40 }), LPARAM(0));
    let _ = child(w!("BUTTON"), "OK", WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32), none, (156, 66, 80, 28), ID_ADD)?;
    let _ = child(w!("BUTTON"), "Cancelar", WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32), none, (244, 66, 84, 28), ID_CLOSE)?;
    let status = child(w!("STATIC"), "", WINDOW_STYLE(0), none, (12, 100, 316, 36), 0)?;
    let font = make_font(&l);
    let _ = EnumChildWindows(Some(hwnd), Some(set_font_cb), LPARAM(font.0 as isize));
    let ip = Box::new(InputPanel { shell: shell.clone(), id, edit, status, font, refused: Cell::new(false), secret: spec.secret });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(ip) as isize);
    INPUTS.with(|o| o.borrow_mut().push((id, hwnd)));
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetFocus(Some(edit));
    send(edit, EM_SETSEL, WPARAM(0), LPARAM(-1));
    Ok(())
}

/// O widget recusou a entrada: a janelinha mostra o motivo e continua aberta.
pub fn input_error(id: WidgetId, msg: &str) {
    let Some(h) = INPUTS.with(|o| o.borrow().iter().find(|(i, _)| *i == id).map(|(_, h)| *h)) else { return };
    unsafe {
        if let Some(ip) = (GetWindowLongPtrW(h, GWLP_USERDATA) as *const InputPanel).as_ref() {
            ip.refused.set(true);
            let _ = SetWindowTextW(ip.status, &HSTRING::from(msg));
            let _ = SetFocus(Some(ip.edit));
        }
    }
}

unsafe fn input_command(hwnd: HWND, id: i32) {
    let Some(ip) = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const InputPanel).as_ref() else { return };
    match id {
        ID_ADD => {
            ip.refused.set(false);
            let text = text_of(ip.edit);
            let (shell, wid) = (ip.shell.clone(), ip.id);
            if ip.secret {
                // Segredo: valida o formato, grava no cofre e apaga o campo; o widget nao recebe o texto.
                match pod_model::github::clean_token(&text) {
                    Some(t) if crate::vault::store(crate::vault::GITHUB, &t) => {
                        let _ = SetWindowTextW(ip.edit, &HSTRING::new());
                        let _ = DestroyWindow(hwnd);
                        shell.github_token_changed();
                    }
                    _ => {
                        let _ = SetWindowTextW(ip.status, &HSTRING::from("Token inv\u{e1}lido (ghp_\u{2026} ou github_pat_\u{2026})."));
                        let _ = SetFocus(Some(ip.edit));
                    }
                }
                return;
            }
            shell.dispatch(wid, Event::TextChanged { field: LocalId(0), text });
            // Aceita: a janelinha fecha; recusada: `input_error` já marcou e mostrou o motivo.
            if !ip.refused.get() {
                let _ = DestroyWindow(hwnd);
            }
        }
        ID_CLOSE => {
            let _ = DestroyWindow(hwnd);
        }
        _ => {}
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let (id, notify) = ((wp.0 & 0xffff) as i32, ((wp.0 >> 16) & 0xffff) as u32);
            if is_input(hwnd) {
                if notify == 0 || notify == 1 {
                    input_command(hwnd, id);
                }
                return LRESULT(0);
            }
            // Duplo clique em uma tarefa ou encomenda alterna a conclusão.
            if id == ID_LIST && notify == LBN_DBLCLK {
                if panel(hwnd).is_some_and(|p| matches!(p.kind, PanelKind::Tasks | PanelKind::Parcels)) {
                    command(hwnd, ID_TOGGLE);
                }
                return LRESULT(0);
            }
            if notify == 0 || notify == 1 {
                command(hwnd, id);
            }
            LRESULT(0)
        }
        WM_PANEL_REFRESH => {
            refresh(hwnd);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            let was_input = INPUTS.with(|o| {
                let mut o = o.borrow_mut();
                let n = o.len();
                o.retain(|(_, h)| *h != hwnd);
                o.len() != n
            });
            OPEN.with(|o| o.borrow_mut().retain(|(_, h)| *h != hwnd));
            if ptr != 0 {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                if was_input {
                    let ip = Box::from_raw(ptr as *mut InputPanel);
                    let _ = DeleteObject(ip.font.into());
                } else {
                    let p = Box::from_raw(ptr as *mut Panel);
                    let _ = DeleteObject(p.font.into());
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
