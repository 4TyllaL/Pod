//! Ícone da bandeja e seu menu nativo: "Nova nota", lista de notas (recuperação), "Ocultar/mostrar
//! todas" e "Sair". Pertence à janela de controle oculta, que recebe as mensagens do ícone e o
//! broadcast `TaskbarCreated` (por isso essa janela não é message-only).

use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::shell::Shell;

/// Mensagem de callback do ícone (privada da janela de controle).
pub const WM_TRAY: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;
const CMD_NEW: usize = 1;
const CMD_TOGGLE_ALL: usize = 2;
const CMD_QUIT: usize = 3;
const CMD_GLASS: usize = 4;
const CMD_EDIT: usize = 5;
const CMD_INTERNET: usize = 6;
const CMD_AUTOSTART: usize = 7;
const CMD_PERF: usize = 8;
const CMD_NOTE_BASE: usize = 100;
const CMD_WIDGET_BASE: usize = 1000;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// Id da mensagem `TaskbarCreated` (0 até `register`).
pub fn taskbar_created_message() -> u32 {
    TASKBAR_CREATED.load(Ordering::Relaxed)
}

/// Registra `TaskbarCreated` e adiciona o ícone. Chamar uma vez na thread de UI.
pub fn register(control: HWND) {
    let msg = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    TASKBAR_CREATED.store(msg, Ordering::Relaxed);
    add(control);
}

/// Ícone do próprio executável (recurso 1, compilado pelo build.rs), no tamanho pequeno do sistema;
/// cai no ícone genérico do Windows se o recurso não existir.
fn app_icon() -> HICON {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        LoadImageW(
            Some(hinst.into()),
            PCWSTR(1usize as *const u16),
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_DEFAULTCOLOR,
        )
        .map(|h| HICON(h.0))
        .or_else(|_| LoadIconW(None, IDI_APPLICATION))
        .unwrap_or_default()
    }
}

/// Adiciona (ou recria, após reinício do Explorer) o ícone.
pub fn add(control: HWND) {
    unsafe {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: control,
            uID: ICON_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: app_icon(),
            ..Default::default()
        };
        for (d, s) in nid.szTip.iter_mut().zip(concat!("Pod ", env!("CARGO_PKG_VERSION")).encode_utf16()) {
            *d = s;
        }
        let ok = Shell_NotifyIconW(NIM_ADD, &nid).as_bool();
        crate::host::log(&format!("tray: NIM_ADD {}", if ok { "aceito" } else { "RECUSADO" }));
    }
}

/// Diagnóstico: onde o shell colocou o ícone (retângulo em tela). Falha se ele não está
/// registrado ou o Explorer ainda não o posicionou.
pub fn log_icon_rect(control: HWND) {
    unsafe {
        let id = NOTIFYICONIDENTIFIER {
            cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: control,
            uID: ICON_ID,
            ..Default::default()
        };
        match Shell_NotifyIconGetRect(&id) {
            Ok(r) => crate::host::log(&format!(
                "tray: icone em ({},{},{},{}) {}x{}",
                r.left, r.top, r.right, r.bottom, r.right - r.left, r.bottom - r.top
            )),
            Err(e) => crate::host::log(&format!("tray: posicao do icone indisponivel: {e}")),
        }
        // `NIM_MODIFY` só tem sucesso se o shell conhece o ícone: prova que ele está registrado.
        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: control,
            uID: ICON_ID,
            uFlags: NIF_TIP,
            ..Default::default()
        };
        let known = Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool();
        crate::host::log(&format!("tray: shell conhece o icone (NIM_MODIFY): {known}"));
    }
}

pub fn remove(control: HWND) {
    unsafe {
        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: control,
            uID: ICON_ID,
            ..Default::default()
        };
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

/// Trata o clique no ícone: abre o menu para os botões principal e secundário.
pub fn on_callback(shell: &Rc<Shell>, control: HWND, lparam: LPARAM) {
    let event = (lparam.0 & 0xffff) as u32;
    if event == WM_RBUTTONUP || event == WM_LBUTTONUP || event == WM_CONTEXTMENU {
        show_menu(shell, control);
    }
}

fn show_menu(shell: &Rc<Shell>, control: HWND) {
    use crate::menu::Item;
    let notes = shell.note_labels();
    let catalog = crate::widgets::catalog();
    let new_widget: Vec<Item> = catalog.iter().enumerate().map(|(i, (_, name))| Item::action(CMD_WIDGET_BASE + i, *name)).collect();
    let mut items = vec![
        Item::action(CMD_NEW, "Nova nota").glyph('\u{E70B}'),
        Item::sub("Novo widget", new_widget).glyph('\u{E710}'),
        Item::action(CMD_EDIT, "Editar widgets\u{2026}").glyph('\u{E70F}').checked(shell.editing()),
    ];
    // A lista cresce com o uso: fica num submenu (e some quando vazia).
    if !notes.is_empty() {
        let list = notes
            .iter()
            .enumerate()
            .map(|(i, (_, label, visible))| Item::action(CMD_NOTE_BASE + i, if *visible { label.clone() } else { format!("{label} (oculto)") }))
            .collect();
        items.push(Item::sub("Meus widgets e notas", list).glyph('\u{E8FD}'));
    }
    items.push(Item::separator());
    items.push(
        Item::action(CMD_TOGGLE_ALL, if shell.hidden_all() { "Mostrar todos" } else { "Ocultar todos" })
            .glyph(if shell.hidden_all() { '\u{E7B3}' } else { '\u{ED1A}' }),
    );
    items.push(
        Item::sub(
            "Ajustes",
            vec![
                Item::action(CMD_AUTOSTART, "Iniciar com o Windows").checked(crate::autostart::enabled()),
                Item::action(CMD_PERF, "Modo desempenho (esconder em tela cheia)").checked(shell.perf_mode()),
                Item::separator(),
                Item::action(CMD_GLASS, "Vidro (transl\u{fa}cido)").checked(shell.glass()),
                Item::action(CMD_INTERNET, "Widgets de internet").checked(shell.internet()),
            ],
        )
        .glyph('\u{E713}'),
    );
    items.push(Item::separator());
    items.push(Item::action(CMD_QUIT, "Sair").glyph('\u{E7E8}'));

    let (shell2, catalog2) = (shell.clone(), catalog);
    let _ = control;
    crate::menu::open(shell, items, Rc::new(move |cmd| run_command(&shell2, &notes, &catalog2, cmd)));
}

fn run_command(shell: &Rc<Shell>, notes: &[(pod_model::ids::WidgetId, String, bool)], catalog: &[(pod_model::ids::WidgetTypeId, &'static str)], cmd: usize) {
    crate::host::log(&format!("bandeja: comando {cmd}"));
    match cmd {
        CMD_NEW => shell.new_note(),
        CMD_TOGGLE_ALL => shell.toggle_hidden_all(),
        CMD_GLASS => shell.toggle_glass(),
        CMD_EDIT => shell.set_editing(!shell.editing()),
        CMD_INTERNET => shell.toggle_internet(),
        CMD_PERF => shell.toggle_perf_mode(),
        CMD_AUTOSTART => {
            let ok = crate::autostart::set(!crate::autostart::enabled());
            crate::host::log(&format!("iniciar com o Windows: {} (ok={ok})", crate::autostart::enabled()));
        }
        CMD_QUIT => unsafe { PostQuitMessage(0) },
        n if n >= CMD_WIDGET_BASE => {
            if let Some((type_id, _)) = catalog.get(n - CMD_WIDGET_BASE) {
                shell.new_widget(type_id);
            }
        }
        n if n >= CMD_NOTE_BASE => {
            if let Some((id, _, _)) = notes.get(n - CMD_NOTE_BASE) {
                shell.reveal(*id);
            }
        }
        _ => {}
    }
}
