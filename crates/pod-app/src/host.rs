//! Window host Win32: cria HWNDs, traduz mensagens e aciona a superfície. Não conhece o conteúdo
//! dos widgets: pede a view ao runtime (via shell) e devolve eventos tipados.
//!
//! Invariante de `unsafe`: o ponteiro em GWLP_USERDATA aponta para um `Box<Window>` criado em
//! `create_window`, usado só na thread de UI e liberado em WM_NCDESTROY. O WndProc solta qualquer
//! `&mut Window` antes de chamar algo que possa reentrar (shell, SetWindowPos, DestroyWindow).

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::rc::Rc;
use std::sync::Arc;

use windows::core::{w, Result, HSTRING};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VIRTUAL_KEY, VK_CONTROL, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::gfx::{self, DrawParams, Gfx, Surface};
use crate::storage::files::{self, LoadOutcome, NoFaults, Source, WorkspaceLock};
use crate::storage::paths;
use crate::storage::session::{Carried, Session};
use crate::shell::{
    Shell, FADE_TIMER, FULLSCREEN_TIMER, SCHEDULER_TIMER, TOPOLOGY_TIMER, WM_DEVICE_LOST, WM_FOREGROUND_CHANGED,
};
use crate::text::{TextEditor, CARET_TIMER};
use crate::tray;
use crate::sysinfo;
use crate::ui::anim::Tween;
use crate::ui::keymode::{self, Arrow, KeyMode};
use crate::ui::theme::Theme;
use crate::ui::note::NoteLayout;
use crate::ui::{self, Placed, PlacedKind, RectDip};
use pod_model::geometry::{dip_to_px, RectPx};
use pod_model::placement::Monitor;
use pod_model::schema::Workspace;
use pod_model::ids::WidgetId;
use pod_model::layout::Rect as LRect;
use pod_model::shape::Shape;
use pod_model::widget::{Chrome, Element, Event, LocalId, SizeDip, ViewCtx};

const BORDER_DIP: i32 = 6;
const MIN_W_DIP: i32 = 160;
const MIN_H_DIP: i32 = 120;

/// Revisão interativa sem alterar notas reais: somente debug e com diretório explícito.
/// Torna as janelas selecionáveis por ferramentas que omitem tool windows. Release mantém
/// os estilos e a política de desktop usuais, mesmo se receber o argumento.
fn review_windows() -> bool {
    #[cfg(debug_assertions)]
    {
        static REVIEW: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *REVIEW.get_or_init(|| {
            let args: Vec<String> = std::env::args().skip(1).collect();
            review_requested(args)
        })
    }
    #[cfg(not(debug_assertions))]
    { false }
}

#[cfg(debug_assertions)]
fn review_requested(args: Vec<String>) -> bool {
    args.iter().any(|a| a == "--review-notes") && paths::parse_data_dir(args).is_some()
}

#[cfg(all(test, debug_assertions))]
mod review_tests {
    #[test]
    fn diagnostic_windows_require_an_explicit_workspace() {
        let args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect();
        assert!(!super::review_requested(args(&["--review-notes"])));
        assert!(!super::review_requested(args(&["--review-notes", "--data-dir="])));
        assert!(!super::review_requested(args(&["--data-dir=review"])));
        assert!(super::review_requested(args(&["--review-notes", "--data-dir=review"])));
        assert!(super::review_requested(args(&["--review-notes", "--data-dir", "review"])));
    }
}

#[cfg(test)]
mod ex_style_tests {
    use super::*;

    #[test]
    fn only_fixed_format_tiles_are_created_without_activation() {
        let noact = WS_EX_NOACTIVATE.0;
        assert_ne!(window_ex_style(Chrome::Tile, false).0 & noact, 0, "cartão: nunca ativa");
        assert_eq!(window_ex_style(Chrome::Note, false).0 & noact, 0, "nota: ativa para digitar");
        assert_eq!(window_ex_style(Chrome::Tile, true).0 & noact, 0, "janela de revisão: inalterada");
        // O resto do estilo não muda: sem barra de tarefas (exceto revisão) e sem bitmap de redirecionamento.
        let tile = window_ex_style(Chrome::Tile, false).0;
        assert_ne!(tile & WS_EX_TOOLWINDOW.0, 0);
        assert_ne!(tile & WS_EX_NOREDIRECTIONBITMAP.0, 0);
        assert_eq!(tile & WS_EX_APPWINDOW.0, 0);
        assert_ne!(window_ex_style(Chrome::Tile, true).0 & WS_EX_APPWINDOW.0, 0);
    }
}

/// Arrasto de cartão: ponto de partida do cursor e da janela (px de tela). `active` só depois de
/// passar do limite de clique (no modo de edição começa na hora).
struct Drag {
    cursor: POINT,
    origin: RECT,
    active: bool,
    /// Bordas puxadas (EDGE_*): 0 = mover; senão redimensiona pela borda, ancorando a oposta.
    edges: u8,
}

const EDGE_L: u8 = 1;
const EDGE_T: u8 = 2;
const EDGE_R: u8 = 4;
const EDGE_B: u8 = 8;
/// Faixa das bordas (DIP) que redimensiona o cartão.
const EDGE_DIP: f32 = 8.0;

/// Bordas sob o ponto `lp` (coordenadas cliente) de um cartão com mais de um formato.
unsafe fn edge_at(hwnd: HWND, x: i32, y: i32) -> u8 {
    let Some(w) = state(hwnd) else { return 0 };
    if w.shell.allowed_shapes(w.id).len() < 2 {
        return 0;
    }
    let mut r = RECT::default();
    if GetClientRect(hwnd, &mut r).is_err() {
        return 0;
    }
    let e = (EDGE_DIP * w.dpi as f32 / 96.0).round() as i32;
    let mut m = 0;
    if x < e {
        m |= EDGE_L;
    }
    if x >= r.right - e {
        m |= EDGE_R;
    }
    if y < e {
        m |= EDGE_T;
    }
    if y >= r.bottom - e {
        m |= EDGE_B;
    }
    m
}

fn edge_cursor(m: u8) -> windows::core::PCWSTR {
    match m {
        x if x == EDGE_L | EDGE_T || x == EDGE_R | EDGE_B => IDC_SIZENWSE,
        x if x == EDGE_R | EDGE_T || x == EDGE_L | EDGE_B => IDC_SIZENESW,
        x if x & (EDGE_T | EDGE_B) != 0 => IDC_SIZENS,
        _ => IDC_SIZEWE,
    }
}

/// Limite em DIPs para um clique virar arrasto fora do modo de edição.
const DRAG_THRESHOLD_DIP: i32 = 4;

/// Encaixa um retângulo de cartão na grade do monitor onde está e o mantém dentro da área útil.
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

struct Window {
    shell: Rc<Shell>,
    id: WidgetId,
    /// Nota (tamanho livre, legenda) ou cartão de grade (formato fixo, sem legenda).
    chrome: Chrome,
    shape: Option<Shape>,
    /// Vira `true` em WM_NCCREATE: a partir daí o Box pertence ao ciclo de vida da janela.
    adopted: Rc<Cell<bool>>,
    surface: Option<Surface>,
    dpi: u32,
    accent_index: usize,
    /// Botões do cartão no último desenho (DIPs): hit test, cliques e ação principal.
    hits: Vec<(LocalId, pod_model::layout::Rect)>,
    /// Última view de cartão desenhada: base do diff que evita quadros sem mudança.
    last_view: Option<pod_model::view::View>,
    /// Posições dos elementos no último desenho, para hit testing de cliques.
    placed: Vec<Placed>,
    /// Só existe para widgets cuja view tem campo de texto (Quick Notes, M4).
    editor: Option<TextEditor>,
    /// Campo de texto ao qual o editor está ligado.
    field: Option<LocalId>,
    /// Composição IME em andamento: nada é entregue ao modelo até confirmar.
    composing: bool,
    /// Dentro do loop modal de mover/redimensionar do usuário.
    interacting: bool,
    /// Arrasto de cartão em andamento (tratado aqui, não pelo loop modal do sistema).
    drag: Option<Drag>,
    /// O shell está posicionando a janela: ignorar o retângulo sugerido por `WM_DPICHANGED`.
    placing: bool,
    /// Foco lógico dentro da nota (a janela tem um único foco do Win32).
    focus: Focus,
    /// A janela tem o foco do teclado (para o anel de foco e o editor).
    has_focus: bool,
    /// Transições curtas de hover e foco.
    menu_hover: Tween,
    focus_ring: Tween,
    button_hover: Option<(LocalId, Tween)>,
    /// `TrackMouseEvent(TME_LEAVE)` pendente.
    tracking: bool,
    /// Tecla que abrirá o menu da nota quando for solta.
    menu_key: Option<usize>,
    /// Mover/redimensionar pelo teclado em andamento, com o retângulo de partida (Esc restaura).
    key_mode: Option<(KeyMode, RectPx)>,
}

/// Para onde vão as teclas dentro de uma nota.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
    Editor,
    MenuButton,
}

/// `WM_MOUSELEAVE` (fica em `Win32::UI::Controls`, que não importamos): sem a constante, o nome
/// virava uma variável de padrão e casava com qualquer mensagem.
const WM_MOUSELEAVE: u32 = 0x02A3;

/// Pedido de despejo das métricas de desenho (só medição).
pub const WM_METRICS_DUMP: u32 = WM_APP + 9;

/// Timer de quadros de animação (só existe enquanto há transição em curso).
const ANIM_TIMER: usize = 0xA17;
/// Pedido (vindo do UI Automation) para abrir o menu da nota fora da chamada do provedor.
pub const WM_OPEN_NOTE_MENU: u32 = WM_APP + 10;
/// `Invoke` de UI Automation em um botão de cartão (wParam = id local do botão).
pub const WM_TILE_INVOKE: u32 = WM_APP + 11;

unsafe fn editor(hwnd: HWND) -> Option<TextEditor> {
    state(hwnd).and_then(|w| w.editor.clone())
}

/// Editor da janela, para provedores de acessibilidade (chamados na thread de UI).
pub fn editor_of(hwnd: HWND) -> Option<TextEditor> {
    unsafe { editor(hwnd) }
}

pub fn run() -> Result<()> {
    unsafe {
        // OLE (clipboard do Rich Edit) na thread de UI, que é STA.
        windows::Win32::System::Ole::OleInitialize(None)?;
        // PMv2 vem do manifesto (ARCHITECTURE §10); a chamada abaixo só reforça em builds sem ele.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        log(&format!(
            "dpi: Per-Monitor V2 = {}",
            AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).as_bool()
        ));
        let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: w!("PodWidget"),
            style: CS_DROPSHADOW,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        let control_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(control_proc),
            hInstance: hinst,
            lpszClassName: w!("PodControl"),
            ..Default::default()
        };
        if RegisterClassExW(&control_class) == 0 {
            return Err(windows::core::Error::from_thread());
        }

        // Instância única por usuário/sessão. Segunda execução informa e sai sem tocar nos dados.
        let _single = CreateMutexW(None, false, w!("Local\\Pod.SingleInstance"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            message("O Pod já está em execução.", MB_OK | MB_ICONINFORMATION);
            return Ok(());
        }
        let Some(startup) = open_workspace() else {
            return Ok(());
        };

        let shell = Shell::new(Rc::new(Gfx::new(false)?), hinst);
        // Janela de controle: top-level oculta (não message-only, para receber broadcasts como
        // TaskbarCreated no futuro). Hospeda o timer único e recebe o aviso do worker de storage.
        let boxed = Box::into_raw(Box::new(shell.clone()));
        let control = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("PodControl"),
            w!("Pod control"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            Some(boxed as *const _),
        )?;
        shell.control.set(control);
        tray::register(control);
        // Bloqueio/desbloqueio da sessão e primeiro plano (detecção de tela cheia).
        let _ = WTSRegisterSessionNotification(control, NOTIFY_FOR_THIS_SESSION);
        CONTROL_FOR_HOOK.store(control.0 as isize, Ordering::Relaxed);
        let hook = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(foreground_hook), 0, 0, WINEVENT_OUTOFCONTEXT);
        shell.refresh_monitors();
        shell.log_theme_state("inicio");

        let Startup { dir, lock, workspace, fresh } = startup;
        let control_raw = control.0 as usize;
        let wake = Box::new(move || {
            // O worker só sinaliza que há acks; os dados vão pela fila.
            let _ = PostMessageW(Some(HWND(control_raw as *mut _)), WM_APP, WPARAM(0), LPARAM(0));
        });
        shell.attach_session(Session::open(
            dir,
            lock,
            workspace.generation,
            Carried { preferences: workspace.preferences.clone(), extra: workspace.extra.clone() },
            Arc::new(NoFaults),
            wake,
        ));
        shell.load_shared(workspace.extra.get("shared").cloned());
        shell.restore(workspace.instances);
        if fresh {
            shell.seed_first_run();
        }
        // Só em depuração: abre direto no modo de edição (testes ao vivo sem o menu da bandeja).
        #[cfg(debug_assertions)]
        if std::env::var_os("POD_START_EDIT").is_some() {
            shell.set_editing(true);
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if crate::panel::is_dialog_message(&msg) {
                continue; // Tab/Enter/Esc de um painel de edição
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if let Err(e) = shell.shutdown() {
            message(
                &format!("As últimas alterações não puderam ser confirmadas no disco:\n{e}"),
                MB_OK | MB_ICONWARNING,
            );
        }
        tray::remove(control);
        let _ = UnhookWinEvent(hook);
        let _ = WTSUnRegisterSessionNotification(control);
        let _ = DestroyWindow(control);
    }
    Ok(())
}

struct Startup {
    dir: PathBuf,
    lock: WorkspaceLock,
    workspace: Workspace,
    /// Não havia workspace gravado: primeira execução deste usuário.
    fresh: bool,
}

fn message(text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    unsafe { MessageBoxW(None, &HSTRING::from(text), w!("Pod"), style | MB_SETFOREGROUND) }
}

/// Trava o diretório, carrega e aplica a política de recuperação. `None` = encerrar sem escrever.
fn open_workspace() -> Option<Startup> {
    let dir = match paths::data_dir() {
        Ok(d) => d,
        Err(e) => {
            message(&format!("Não foi possível localizar a pasta de dados:\n{e}"), MB_OK | MB_ICONERROR);
            return None;
        }
    };
    let lock = match WorkspaceLock::acquire(&dir) {
        Ok(l) => l,
        Err(e) if files::is_sharing_violation(&e) => {
            message(
                "Outra sessão do Pod já está usando os dados deste usuário. Esta cópia será fechada.",
                MB_OK | MB_ICONINFORMATION,
            );
            return None;
        }
        Err(e) => {
            message(&format!("Não foi possível abrir a pasta de dados:\n{e}"), MB_OK | MB_ICONERROR);
            return None;
        }
    };
    let mut fresh = false;
    let workspace = match files::load(&dir, &NoFaults) {
        Err(e) => {
            message(&format!("Não foi possível ler os dados:\n{e}"), MB_OK | MB_ICONERROR);
            return None;
        }
        Ok(LoadOutcome::Fresh) => {
            fresh = true;
            Workspace::empty()
        }
        Ok(LoadOutcome::Loaded { workspace, source, .. }) => {
            if source == Source::PreviousRecovered {
                message(
                    "O arquivo principal estava ausente ou inválido. Os dados foram recuperados da \
                     geração anterior; o arquivo problemático foi preservado na pasta recovery.",
                    MB_OK | MB_ICONWARNING,
                );
            }
            workspace
        }
        Ok(LoadOutcome::NewerVersion { found, supported }) => {
            message(
                &format!(
                    "Os dados foram gravados por uma versão mais nova do Pod (formato {found}; \
                     esta versão entende até o {supported}). Nada foi alterado. Use a versão compatível."
                ),
                MB_OK | MB_ICONERROR,
            );
            return None;
        }
        Ok(LoadOutcome::Unusable { .. }) => {
            let answer = message(
                "Não foi possível ler os dados nem a geração anterior. Os arquivos foram \
                 preservados na pasta recovery.\n\nIniciar com um espaço de trabalho novo?",
                MB_YESNO | MB_ICONQUESTION,
            );
            if answer != IDYES || files::quarantine_all(&dir).is_err() {
                return None;
            }
            Workspace::empty()
        }
    };
    Some(Startup { dir, lock, workspace, fresh })
}

/// `WM_SETTINGCHANGE` de tema: lParam "ImmersiveColorSet" ou SPI_SETHIGHCONTRAST (0x43).
unsafe fn is_theme_change(wp: WPARAM, lp: LPARAM) -> bool {
    if wp.0 == 0x43 {
        return true;
    }
    lp.0 != 0 && windows::core::PCWSTR(lp.0 as *const u16).to_string().is_ok_and(|s| s == "ImmersiveColorSet")
}

unsafe extern "system" fn control_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_TIMER if wp.0 == SCHEDULER_TIMER => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                shell.on_timer();
            }
            LRESULT(0)
        }
        // Pedido externo de encerramento (atualização manual/instalador): sai pelo caminho normal,
        // com snapshot final. Não é IPC: é a mensagem padrão de fechar uma janela.
        WM_CLOSE => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == TOPOLOGY_TIMER => {
            let _ = KillTimer(Some(hwnd), TOPOLOGY_TIMER);
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                shell.apply_topology();
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == FADE_TIMER => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                shell.on_fade_tick();
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == crate::shell::SLOTS_TIMER => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                shell.refresh_slots();
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == FULLSCREEN_TIMER => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                let _ = KillTimer(Some(hwnd), FULLSCREEN_TIMER);
                shell.evaluate_fullscreen();
            }
            LRESULT(0)
        }
        WM_FOREGROUND_CHANGED => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                shell.update_activation_for(HWND(wp.0 as *mut _));
                shell.schedule_fullscreen_check();
            }
            LRESULT(0)
        }
        WM_DEVICE_LOST => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                shell.recover_devices();
            }
            LRESULT(0)
        }
        crate::shell::WM_OPEN_PANEL => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                use pod_model::widget::PanelKind as K;
                let kind = [K::Events, K::Tasks, K::Links, K::Parcels]
                    .into_iter()
                    .find(|k| *k as usize == wp.0)
                    .unwrap_or(K::Events);
                crate::panel::open(&shell, kind);
            }
            LRESULT(0)
        }
        // Relógio do sistema acertado, fuso ou horário de verão: relê a hora.
        0x001E /* WM_TIMECHANGE */ => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                shell.on_time_changed();
            }
            LRESULT(0)
        }
        // Energia e sessão: gravar pendências ao suspender/bloquear, revalidar ao voltar.
        WM_POWERBROADCAST => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                match wp.0 as u32 {
                    PBT_APMSUSPEND => shell.on_suspend(),
                    PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => shell.on_resume(),
                    _ => {}
                }
            }
            LRESULT(1)
        }
        WM_WTSSESSION_CHANGE => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                match wp.0 as u32 {
                    WTS_SESSION_LOCK => shell.on_suspend(),
                    WTS_SESSION_UNLOCK => shell.on_resume(),
                    _ => {}
                }
            }
            LRESULT(0)
        }
        // Monitores, resolução e área útil: reavalia depois de a rajada de mensagens acabar.
        WM_DISPLAYCHANGE => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                shell.schedule_topology();
            }
            LRESULT(0)
        }
        // Tema do sistema (claro/escuro) ou alto contraste mudou: reavalia o visual.
        WM_SETTINGCHANGE if is_theme_change(wp, lp) => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                shell.on_system_theme_message();
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE if wp.0 == SPI_SETWORKAREA.0 as usize => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                shell.schedule_topology();
            }
            LRESULT(0)
        }
        // Só para medição: despeja os contadores de desenho no log (wParam 1 zera as amostras).
        WM_METRICS_DUMP => {
            let s = crate::metrics::snapshot();
            log(&format!(
                "metrics: presents={} samples={} work_p50={:.3}ms work_p95={:.3}ms work_max={:.3}ms total_p95={:.3}ms",
                s.presents, s.samples, s.work_p50_ms, s.work_p95_ms, s.work_max_ms, s.total_p95_ms
            ));
            let r = crate::sysmetrics::reads();
            log(&format!("sysmetrics: reads_cpu={} reads_mem={} reads_disk={} reads_battery={} reads_net={}", r[0], r[1], r[2], r[3], r[4]));
            log(&format!("media: reads={} notificacoes: reads={}", crate::media::reads(), crate::notif::reads()));
            log(&format!("claude usage: ciclos={}", crate::claude_usage::cycles()));
            log(&format!("rede: pedidos={} noticias={}", crate::weather::requests(), crate::news::requests()));
        log(&format!("rede: github={} cotacoes={}", crate::github::requests(), crate::quotes::requests()));
            let l = crate::localdata::reads();
            log(&format!("localdata: reads_downloads={} reads_screenshots={} reads_clipboard={}", l[0], l[1], l[2]));
            crate::tray::log_icon_rect(hwnd);
            if wp.0 == 1 {
                crate::metrics::reset_samples();
            }
            LRESULT(0)
        }
        WM_APP => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                shell.on_acks();
            }
            LRESULT(0)
        }
        tray::WM_TRAY => {
            let shell = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<Shell>).as_ref().cloned();
            if let Some(shell) = shell {
                tray::on_callback(&shell, hwnd, lp);
            }
            LRESULT(0)
        }
        m if m != 0 && m == tray::taskbar_created_message() => {
            // Explorer reiniciou: recria o ícone.
            tray::add(hwnd);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Rc<Shell>;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !p.is_null() {
                drop(Box::from_raw(p));
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Cria (oculta) a janela de uma instância. O shell mostra depois de montar a instância.
pub unsafe fn create_window(
    shell: &Rc<Shell>,
    hinst: HINSTANCE,
    id: WidgetId,
    pos: (i32, i32),
    accent_index: usize,
    chrome: Chrome,
    shape: Option<Shape>,
) -> Result<HWND> {
    let adopted = Rc::new(Cell::new(false));
    let ptr = Box::into_raw(Box::new(Window {
        shell: shell.clone(),
        id,
        chrome,
        shape,
        adopted: adopted.clone(),
        surface: None,
        dpi: 96,
        accent_index,
        hits: Vec::new(),
        last_view: None,
        placed: Vec::new(),
        editor: None,
        field: None,
        composing: false,
        interacting: false,
        drag: None,
        placing: false,
        focus: Focus::MenuButton,
        has_focus: false,
        menu_hover: Tween::at(0.0),
        focus_ring: Tween::at(0.0),
        button_hover: None,
        tracking: false,
        menu_key: None,
        key_mode: None,
    }));
    let result = CreateWindowExW(
        window_ex_style(chrome, review_windows()),
        w!("PodWidget"),
        w!("Pod"),
        WS_POPUP | WS_THICKFRAME,
        pos.0,
        pos.1,
        320,
        240,
        None,
        None,
        Some(hinst),
        Some(ptr as *const _),
    );
    if result.is_err() && !adopted.get() {
        // Falhou antes de WM_NCCREATE: nenhuma janela assumiu o Box, então liberamos aqui.
        drop(Box::from_raw(ptr));
    }
    result
}

/// Estilos estendidos da janela de um widget. Cartão de formato fixo nunca é ativado: `MA_NOACTIVATE` sozinho evita que ele
/// se ative, mas o Windows ainda desativa o aplicativo em uso ao clicar (o primeiro plano vira "nenhuma janela"; ver
/// docs/validation/focus-click.md). Notas ativam (são para digitar). Em janelas de revisão (aparecem na barra de tarefas)
/// nada muda.
fn window_ex_style(chrome: Chrome, review: bool) -> WINDOW_EX_STYLE {
    let base = if review { WS_EX_APPWINDOW } else { WS_EX_TOOLWINDOW } | WS_EX_NOREDIRECTIONBITMAP;
    if chrome == Chrome::Tile && !review { base | WS_EX_NOACTIVATE } else { base }
}

/// Janela de controle para o hook de eventos (callback sem contexto).
static CONTROL_FOR_HOOK: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn foreground_hook(_: HWINEVENTHOOK, _: u32, fg: HWND, _: i32, _: i32, _: u32, _: u32) {
    let control = CONTROL_FOR_HOOK.load(Ordering::Relaxed);
    if control != 0 {
        let _ = PostMessageW(Some(HWND(control as *mut _)), WM_FOREGROUND_CHANGED, WPARAM(fg.0 as usize), LPARAM(0));
    }
}

/// Há um aplicativo normal em primeiro plano (não o desktop, a barra de tarefas nem o Pod)?
/// É o que decide o esmaecimento dos cartões.
pub fn app_in_foreground() -> bool {
    app_in_foreground_for(unsafe { GetForegroundWindow() })
}

/// Como `app_in_foreground`, para a janela que o evento de primeiro plano informou.
pub fn app_in_foreground_for(fg: HWND) -> bool {
    unsafe {
        if fg.0.is_null() || IsIconic(fg).as_bool() || !IsWindowVisible(fg).as_bool() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == std::process::id() {
            return false;
        }
        let mut class = [0u16; 64];
        let n = GetClassNameW(fg, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        !matches!(
            class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "NotifyIconOverflowWindow" | "XamlExplorerHostIslandWindow"
        )
    }
}

/// Monitor inteiramente coberto pela janela em primeiro plano, se houver. Exclui o Pod e o shell
/// (área de trabalho e barras). Heurística, não um identificador de jogo.
pub fn fullscreen_monitor(monitors: &[Monitor]) -> Option<usize> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == std::process::id() {
            return None;
        }
        let mut class = [0u16; 64];
        let n = GetClassNameW(fg, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
            return None;
        }
        // Limites visíveis (sem a sombra invisível); cai para o retângulo da janela se falhar.
        let mut r = RECT::default();
        let ok = DwmGetWindowAttribute(
            fg,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok();
        if !ok {
            let _ = GetWindowRect(fg, &mut r);
        }
        const TOL: i32 = 2;
        monitors.iter().position(|m| {
            let b = m.bounds;
            (r.left - b.left).abs() <= TOL
                && (r.top - b.top).abs() <= TOL
                && (r.right - b.right).abs() <= TOL
                && (r.bottom - b.bottom).abs() <= TOL
        })
    }
}

/// Recria a superfície da janela depois de uma troca de dispositivo gráfico.
pub fn rebuild_surface(hwnd: HWND) {
    unsafe {
        let Some(shell) = state(hwnd).map(|w| {
            w.surface = None; // libera o alvo de composição antigo antes de criar o novo
            w.shell.clone()
        }) else {
            return;
        };
        let gfx = shell.gfx();
        let mut c = RECT::default();
        let _ = GetClientRect(hwnd, &mut c);
        match Surface::new(&gfx, hwnd, (c.right - c.left).max(1) as u32, (c.bottom - c.top).max(1) as u32) {
            Ok(s) => {
                if let Some(w) = state(hwnd) {
                    w.surface = Some(s);
                }
            }
            Err(e) => log(&format!("surface não recriada: {e}")),
        }
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// Troca o formato do cartão e ajusta a janela ao tamanho exato do novo formato.
pub fn set_tile_shape(hwnd: HWND, shape: Shape) {
    unsafe {
        if let Some(w) = state(hwnd) {
            w.shape = Some(shape);
        }
    }
    fit_tile(hwnd, shape);
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// Itens de acessibilidade do cartão, gerados da view atual (None se a janela não é um cartão).
/// Os retângulos vêm em pixels de tela.
pub fn tile_a11y_items(hwnd: HWND) -> Option<Vec<(pod_model::a11y::A11yItem, RECT)>> {
    unsafe {
        let (shell, id, dpi, shape) =
            state(hwnd).filter(|w| w.chrome == Chrome::Tile).and_then(|w| w.shape.map(|s| (w.shell.clone(), w.id, w.dpi, s)))?;
        let ctx = ViewCtx { shape, time: sysinfo::local_time() };
        let view = shell.runtime.borrow().tile_view(id, &ctx).ok().flatten()?;
        let sz = shape.size_dip();
        let gfx = shell.gfx();
        let laid = pod_model::layout::layout(&view, LRect::new(0.0, 0.0, sz.w, sz.h), &crate::render::DwMeasure(&gfx));
        let mut origin = POINT::default();
        let _ = ClientToScreen(hwnd, &mut origin);
        let s = dpi as f32 / 96.0;
        let items = pod_model::a11y::items(&view, &laid)
            .into_iter()
            .map(|it| {
                let r = RECT {
                    left: origin.x + (it.rect.x * s).round() as i32,
                    top: origin.y + (it.rect.y * s).round() as i32,
                    right: origin.x + (it.rect.right() * s).round() as i32,
                    bottom: origin.y + (it.rect.bottom() * s).round() as i32,
                };
                (it, r)
            })
            .collect();
        Some(items)
    }
}

pub fn set_title(hwnd: HWND, title: &str) {
    unsafe {
        let _ = SetWindowTextW(hwnd, &HSTRING::from(title));
    }
}

/// Aviso sonoro do sistema (asterisco), sem janela nem arquivo próprio.
pub fn chime() {
    unsafe {
        let _ = windows::Win32::System::Diagnostics::Debug::MessageBeep(MB_ICONASTERISK);
    }
}

/// Botão do cartão sob o ponto (coordenadas de cliente em `lp`), com shell e id da instância.
unsafe fn tile_button_at(hwnd: HWND, lp: LPARAM) -> Option<(Rc<Shell>, WidgetId, LocalId)> {
    let (x, y) = ((lp.0 & 0xffff) as i16 as f32, ((lp.0 >> 16) & 0xffff) as i16 as f32);
    state(hwnd).and_then(|w| {
        let s = w.dpi as f32 / 96.0;
        let (px, py) = (x / s, y / s);
        w.hits
            .iter()
            .find(|(_, b)| px >= b.x && px < b.right() && py >= b.y && py < b.bottom())
            .map(|(b, _)| (w.shell.clone(), w.id, *b))
    })
}

pub fn is_tile(hwnd: HWND) -> bool {
    unsafe { state(hwnd).is_some_and(|w| w.chrome == Chrome::Tile) }
}

/// Pede um quadro só se a view do cartão difere da última desenhada.
pub fn invalidate_tile_if_changed(hwnd: HWND) {
    unsafe {
        let Some((shell, id, shape)) = state(hwnd).and_then(|w| w.shape.map(|s| (w.shell.clone(), w.id, s))) else {
            return;
        };
        let ctx = ViewCtx { shape, time: sysinfo::local_time() };
        let now = shell.runtime.borrow().tile_view(id, &ctx).ok().flatten();
        let changed = state(hwnd).is_some_and(|w| w.last_view != now);
        if changed {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

/// Ajusta a janela de um cartão ao tamanho exato do formato no DPI em que ela está.
pub fn fit_tile(hwnd: HWND, shape: Shape) {
    let dpi = dpi_of(hwnd);
    let sz = shape.size_dip();
    let r = rect_of(hwnd);
    set_rect(
        hwnd,
        RectPx {
            left: r.left,
            top: r.top,
            right: r.left + dip_to_px(sz.w as f64, dpi),
            bottom: r.top + dip_to_px(sz.h as f64, dpi),
        },
    );
}

/// Retângulo atual da janela em pixels do desktop virtual.
pub fn rect_of(hwnd: HWND) -> RectPx {
    unsafe {
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd, &mut r);
        RectPx { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
    }
}

pub fn dpi_of(hwnd: HWND) -> u32 {
    unsafe { GetDpiForWindow(hwnd).max(1) }
}

/// Posiciona a janela exatamente em `rect` (pixels físicos), sem deixar o `WM_DPICHANGED` que a
/// mudança de monitor provoca sobrescrever o resultado com o retângulo sugerido.
pub fn set_rect(hwnd: HWND, rect: RectPx) {
    unsafe {
        if let Some(w) = state(hwnd) {
            w.placing = true;
        }
        let _ = SetWindowPos(
            hwnd,
            None,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        if let Some(w) = state(hwnd) {
            w.placing = false;
        }
    }
}

/// Reposiciona só se mudou e se o usuário não está movendo a janela agora.
pub fn move_to(hwnd: HWND, rect: RectPx) {
    let interacting = unsafe { state(hwnd).is_some_and(|w| w.interacting) };
    if !interacting && rect_of(hwnd) != rect {
        set_rect(hwnd, rect);
    }
}

/// Campo de texto da view e seu texto atual, se houver.
fn find_field(e: &Element) -> Option<(LocalId, String)> {
    match e {
        Element::TextField { id, text } => Some((*id, text.clone())),
        Element::Surface(children) => children.iter().find_map(find_field),
        _ => None,
    }
}

/// Mantém o editor nativo de acordo com a view: cria quando há campo de texto, reposiciona pelo
/// layout atual e descarta quando o campo some. Chamar fora de empréstimos do estado da janela.
pub fn sync_editor(hwnd: HWND) {
    unsafe {
        if state(hwnd).is_some_and(|w| w.chrome == Chrome::Tile) {
            return; // cartões de grade não têm campo de texto no W1
        }
        let Some((shell, id, dpi, size)) = state(hwnd)
            .and_then(|w| w.surface.as_ref().map(|s| (w.shell.clone(), w.id, w.dpi, (s.width, s.height))))
        else {
            return;
        };
        let scale = dpi as f32 / 96.0;
        let (w_dip, h_dip) = (size.0 as f32 / scale, size.1 as f32 / scale);
        let Ok(view) = shell.runtime.borrow().view(id, SizeDip { w: w_dip, h: h_dip }) else {
            return;
        };
        let placed = ui::layout(&view, content_area(w_dip, h_dip));
        let field = placed.iter().find_map(|p| match p.kind {
            PlacedKind::TextField { id } => Some((id, p.rect)),
            _ => None,
        });
        let Some((field_id, r)) = field else {
            // O widget não tem campo de texto (ou ele sumiu): sem editor.
            let old = state(hwnd).and_then(|w| {
                w.field = None;
                w.editor.take()
            });
            if let Some(ed) = old {
                ed.deactivate();
            }
            return;
        };
        let px = |v: f32| (v * scale).round() as i32;
        let rc = RECT { left: px(r.x), top: px(r.y), right: px(r.x + r.w), bottom: px(r.y + r.h) };
        match editor(hwnd) {
            Some(ed) => {
                ed.set_client(rc, dpi);
                if !state(hwnd).is_some_and(|w| w.composing) {
                    let note = NoteLayout::new(w_dip, h_dip);
                    ed.set_font_size(note.font_dip);
                }
            }
            None => {
                let initial = find_field(&view).map(|(_, t)| t).unwrap_or_default();
                let color = shell.theme().editorial().text;
                let note = NoteLayout::new(w_dip, h_dip);
                match TextEditor::new(hwnd, rc, dpi, &initial, color, note.font_dip) {
                    Ok(ed) => {
                        ed.mark_delivered();
                        if let Some(w) = state(hwnd) {
                            w.editor = Some(ed);
                            w.field = Some(field_id);
                            w.focus = Focus::Editor; // digitar logo ao ativar a nota
                        }
                    }
                    Err(e) => log(&format!("text editor: {e}")),
                }
            }
        }
    }
}

/// Se o texto do editor mudou (e não há composição em andamento), entrega ao modelo.
pub fn deliver_text(hwnd: HWND) {
    unsafe {
        let Some((ed, field, shell, id)) = state(hwnd).and_then(|w| {
            if w.composing {
                return None;
            }
            Some((w.editor.clone()?, w.field?, w.shell.clone(), w.id))
        }) else {
            return;
        };
        if let Some(text) = ed.take_text_change() {
            shell.dispatch(id, Event::TextChanged { field, text });
        }
    }
}

/// Efeito `SetText` do widget: troca o texto do editor sem contá-lo como edição do usuário.
pub fn apply_set_text(hwnd: HWND, field: LocalId, text: &str) {
    unsafe {
        let ed = state(hwnd).and_then(|w| if w.field == Some(field) { w.editor.clone() } else { None });
        if let Some(ed) = ed {
            let _ = ed.set_text_silent(text);
        }
    }
}

/// Confirmação de "Restaurar padrão": diz o que some e o que fica. Padrão do botão: Não.
pub fn confirm_restore(owner: Option<HWND>, tiles: usize) -> bool {
    let text = format!(
        "Restaurar o layout padrão?\n\nOs {tiles} cartões atuais serão removidos e voltam os 3 iniciais (Relógio, Calendário e Sistema). Suas notas não são apagadas.\n\nNão dá para desfazer."
    );
    unsafe {
        MessageBoxW(owner, &HSTRING::from(text), w!("Pod"), MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2 | MB_SETFOREGROUND) == IDYES
    }
}

pub fn confirm_delete(owner: Option<HWND>) -> bool {
    unsafe {
        MessageBoxW(
            owner,
            w!("Excluir esta nota? O conteúdo será removido."),
            w!("Pod"),
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2 | MB_SETFOREGROUND,
        ) == IDYES
    }
}

/// Traz a janela para frente e a ativa (recuperação pela bandeja).
/// Só durante `raise_note`: a nota que o usuário pediu para escrever pode vir à frente.
static RAISE_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Traz uma **nota** à frente para digitar (ela volta ao fundo ao perder o foco). Cartões nunca
/// sobem: não há equivalente para eles.
pub fn raise_note(hwnd: HWND) {
    if is_tile(hwnd) {
        return;
    }
    unsafe {
        RAISE_OK.store(true, Ordering::Relaxed);
        let _ = SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        let _ = SetForegroundWindow(hwnd);
        RAISE_OK.store(false, Ordering::Relaxed);
    }
}

/// Camada "desktop": cartões e notas vivem atrás de todas as janelas normais, no fundo da área
/// de trabalho, e nunca sobem sobre os aplicativos (nem ao criar, nem ao clicar). Só operações
/// documentadas: `HWND_BOTTOM` e a regravação do `hwndInsertAfter` no `WM_WINDOWPOSCHANGING`
/// das próprias janelas; nenhum laço disputa a ordem com outros processos.
/// Modo de edição: os cartões ficam acima dos aplicativos para dar para ver e editar a grade; ao
/// sair voltam ao fundo (`send_to_bottom`, que também tira o estado de "sempre no topo").
pub fn set_edit_layer(hwnd: HWND, on: bool) {
    if on {
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    } else {
        send_to_bottom(hwnd);
    }
}

pub fn send_to_bottom(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Começa o arrasto de um cartão: captura o mouse (o cartão não ativa, então não sobe sobre apps).
unsafe fn begin_drag(hwnd: HWND, immediate: bool, edges: u8) {
    let mut cursor = POINT::default();
    let mut origin = RECT::default();
    if GetCursorPos(&mut cursor).is_err() || GetWindowRect(hwnd, &mut origin).is_err() {
        return;
    }
    if let Some(w) = state(hwnd) {
        w.drag = Some(Drag { cursor, origin, active: immediate, edges });
        if immediate {
            w.interacting = true;
        }
    }
    SetCapture(hwnd);
}

/// Segue o cursor: a janela acompanha livremente e só encaixa na grade ao soltar.
unsafe fn drag_to(hwnd: HWND) {
    let mut now = POINT::default();
    if GetCursorPos(&mut now).is_err() {
        return;
    }
    let Some(w) = state(hwnd) else { return };
    let dpi = w.dpi;
    let Some(d) = w.drag.as_mut() else { return };
    let (dx, dy) = (now.x - d.cursor.x, now.y - d.cursor.y);
    if !d.active {
        let limit = dip_to_px(DRAG_THRESHOLD_DIP as f64, dpi);
        if dx.abs() <= limit && dy.abs() <= limit {
            return;
        }
        d.active = true;
        w.interacting = true;
    }
    if d.edges != 0 {
        // Borda puxada: o formato acompanha o cursor (o mais próximo entre os que o tipo declara);
        // a borda oposta fica parada (puxar o topo estende para cima).
        let s = dpi as f32 / 96.0;
        let (edges, o) = (d.edges, d.origin);
        let (ow, oh) = ((o.right - o.left) as f32 / s, (o.bottom - o.top) as f32 / s);
        let (fx, fy) = (dx as f32 / s, dy as f32 / s);
        let want_w = if edges & EDGE_R != 0 { ow + fx } else if edges & EDGE_L != 0 { ow - fx } else { ow };
        let want_h = if edges & EDGE_B != 0 { oh + fy } else if edges & EDGE_T != 0 { oh - fy } else { oh };
        let (shell, id) = (w.shell.clone(), w.id);
        let best = shell.allowed_shapes(id).into_iter().min_by(|a, b| {
            let dist = |sh: &Shape| {
                let z = sh.size_dip();
                (z.w - want_w).powi(2) + (z.h - want_h).powi(2)
            };
            dist(a).partial_cmp(&dist(b)).unwrap_or(std::cmp::Ordering::Equal)
        });
        if let Some(shape) = best {
            shell.set_shape(id, shape);
            let z = shape.size_dip();
            let (wpx, hpx) = (dip_to_px(z.w as f64, dpi), dip_to_px(z.h as f64, dpi));
            let x = if edges & EDGE_L != 0 { o.right - wpx } else { o.left };
            let y = if edges & EDGE_T != 0 { o.bottom - hpx } else { o.top };
            let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        return;
    }
    let (x, y) = (d.origin.left + dx, d.origin.top + dy);
    let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
}

/// Solta o cartão: encaixa na grade e só então a posição vira preferência a gravar.
unsafe fn end_drag(hwnd: HWND, commit: bool) {
    let Some(w) = state(hwnd) else { return };
    let Some(d) = w.drag.take() else { return };
    let dpi = w.dpi;
    let was_active = d.active;
    let who = (w.shell.clone(), w.id);
    if ReleaseCapture().is_err() {
        // a captura já tinha sido tirada (WM_CAPTURECHANGED)
    }
    // Mover encaixa na grade; redimensionar mantém a borda oposta onde estava.
    if was_active && commit && d.edges == 0 {
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_ok() {
            snap_tile_rect(&mut r, dpi);
            let _ = SetWindowPos(hwnd, None, r.left, r.top, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
    if let Some(w) = state(hwnd) {
        w.interacting = false;
    }
    if was_active {
        who.0.window_moved(who.1);
    }
}

/// Menu de contexto da faixa de movimentação: nova nota, ocultar, excluir.
unsafe fn note_menu(hwnd: HWND, anchor: Option<POINT>) {
    let Some((shell, id)) = state(hwnd).map(|w| (w.shell.clone(), w.id)) else {
        return;
    };
    let Ok(menu) = CreatePopupMenu() else { return };
    let tile = state(hwnd).is_some_and(|w| w.chrome == Chrome::Tile);
    let _ = AppendMenuW(menu, MF_STRING, 1, w!("Nova nota"));
    let _ = AppendMenuW(menu, MF_STRING, 2, if tile { w!("Ocultar widget") } else { w!("Ocultar nota") });
    if tile {
        let edit_label = if shell.editing() { w!("Concluir edi\u{e7}\u{e3}o") } else { w!("Editar widgets\u{2026}") };
        let _ = AppendMenuW(menu, MF_STRING, 6, edit_label);
    }
    let _ = AppendMenuW(menu, MF_STRING, 4, w!("Mover (Alt+F7)"));
    if !tile {
        let _ = AppendMenuW(menu, MF_STRING, 5, w!("Redimensionar (Alt+F8)"));
    }
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
    let _ = AppendMenuW(menu, MF_STRING, 3, if tile { w!("Excluir widget\u{2026}") } else { w!("Excluir nota\u{2026}") });
    // Ajustes do próprio widget (itens soltos e submenus por grupo) e escolha de formato.
    let extra = shell.runtime.borrow().menu_items(id);
    if !extra.is_empty() {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let mut groups: Vec<(&'static str, HMENU)> = Vec::new();
        for (i, item) in extra.iter().enumerate() {
            let flags = if item.checked { MF_STRING | MF_CHECKED } else { MF_STRING };
            let text = HSTRING::from(item.label.as_str());
            match item.group {
                None => {
                    let _ = AppendMenuW(menu, flags, 1000 + i, &text);
                }
                Some(name) => {
                    let sub = match groups.iter().find(|(n, _)| *n == name) {
                        Some((_, h)) => *h,
                        None => {
                            let Ok(h) = CreatePopupMenu() else { continue };
                            let _ = AppendMenuW(menu, MF_POPUP, h.0 as usize, &HSTRING::from(name));
                            groups.push((name, h));
                            h
                        }
                    };
                    let _ = AppendMenuW(sub, flags, 1000 + i, &text);
                }
            }
        }
    }
    let shapes: Vec<Shape> = if tile {
        shell.runtime.borrow().descriptor_of(id).map(|d| d.shapes).unwrap_or_default()
    } else {
        Vec::new()
    };
    if shapes.len() > 1 {
        if let Ok(sub) = CreatePopupMenu() {
            let current = state(hwnd).and_then(|w| w.shape);
            for (i, s) in shapes.iter().enumerate() {
                let flags = if Some(*s) == current { MF_STRING | MF_CHECKED } else { MF_STRING };
                let label = match s.size_name(&shapes) {
                    Some(name) => format!("{name} ({s})"),
                    None => s.to_string(),
                };
                let _ = AppendMenuW(sub, flags, 200 + i, &HSTRING::from(label));
            }
            let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, w!("Tamanho"));
        }
    }
    let mut pt = POINT::default();
    match anchor {
        Some(a) => pt = a,
        None => {
            let _ = GetCursorPos(&mut pt);
        }
    }
    let _ = SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, Some(0), hwnd, None);
    let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
    match cmd.0 {
        1 => shell.new_note(),
        2 => shell.hide(id),
        3 => shell.request_remove(id),
        4 => begin_key_mode(hwnd, KeyMode::Move),
        5 => begin_key_mode(hwnd, KeyMode::Resize),
        6 => shell.set_editing(!shell.editing()),
        c @ 1000..=1999 => {
            if let Some(item) = extra.get((c - 1000) as usize) {
                shell.menu_command(id, item.id);
            }
        }
        c @ 200..=299 => {
            if let Some(s) = shapes.get((c - 200) as usize) {
                shell.set_shape(id, *s);
            }
        }
        _ => {}
    }
}

fn now_ms_of(hwnd: HWND) -> u64 {
    unsafe { state(hwnd).map_or(0, |w| w.shell.now_ms()) }
}

/// Retângulo do botão de menu em pixels de tela para a janela em `r` (RECT de tela).
fn menu_rect_px(r: RECT, dpi: u32) -> RECT {
    let w_dip = (r.right - r.left) as f32 * 96.0 / dpi.max(1) as f32;
    let h_dip = (r.bottom - r.top) as f32 * 96.0 / dpi.max(1) as f32;
    let m = NoteLayout::new(w_dip, h_dip).menu;
    let px = |d: f32| dip_to_px(d as f64, dpi);
    RECT { left: r.left + px(m.x), top: r.top + px(m.y), right: r.left + px(m.x + m.w), bottom: r.top + px(m.y + m.h) }
}

/// O clique (coordenadas do cliente, lParam) caiu no botão de menu?
unsafe fn menu_hit(hwnd: HWND, lp: LPARAM) -> bool {
    let dpi = state(hwnd).map_or(96, |w| w.dpi);
    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    let m = menu_rect_px(r, dpi);
    let (x, y) = ((lp.0 & 0xffff) as i16 as i32, ((lp.0 >> 16) & 0xffff) as i16 as i32);
    x >= m.left && x < m.right && y >= m.top && y < m.bottom
}

/// Retângulo do botão de menu em coordenadas de tela (para âncora do menu e para o UI Automation).
pub fn menu_button_screen_rect(hwnd: HWND) -> RECT {
    unsafe {
        let dpi = state(hwnd).map_or(96, |w| w.dpi);
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd, &mut r);
        menu_rect_px(r, dpi)
    }
}

/// O botão de menu tem o foco lógico (e a janela tem o foco do teclado)?
pub fn menu_button_focused(hwnd: HWND) -> bool {
    unsafe { state(hwnd).is_some_and(|w| w.has_focus && w.focus == Focus::MenuButton) }
}

/// UIA SetFocus precisa escolher o filho lógico, não apenas focar o HWND compartilhado.
pub fn focus_note_control(hwnd: HWND, menu: bool) -> Result<()> {
    unsafe {
        if state(hwnd).is_none() || is_tile(hwnd) || (!menu && editor(hwnd).is_none()) {
            return Err(windows::core::Error::from(E_INVALIDARG));
        }
        set_focus_target(hwnd, if menu { Focus::MenuButton } else { Focus::Editor });
        windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(hwnd))?;
    }
    Ok(())
}

/// A tecla pedida abre o menu da nota? (Shift+F10, tecla de menu, Enter/Espaço com o botão focado.)
unsafe fn is_menu_key(hwnd: HWND, vk: usize) -> bool {
    (vk == 0x79 && GetKeyState(VK_SHIFT.0 as i32) < 0)
        || vk == 0x5D
        || ((vk == 0x0D || vk == 0x20) && focus_is(hwnd, Focus::MenuButton) && !is_tile(hwnd))
}

/// Há mover/redimensionar por teclado em andamento nesta janela?
unsafe fn key_mode_active(hwnd: HWND) -> bool {
    state(hwnd).is_some_and(|w| w.key_mode.is_some())
}

unsafe fn begin_key_mode(hwnd: HWND, mode: KeyMode) {
    let start = rect_of(hwnd);
    if let Some(w) = state(hwnd) {
        w.key_mode = Some((mode, start));
    }
    let _ = InvalidateRect(Some(hwnd), None, false);
}

/// Encerra o modo: confirmar grava a nova preferência; cancelar restaura onde estava.
unsafe fn end_key_mode(hwnd: HWND, confirm: bool) {
    let Some((start, shell, id)) = state(hwnd).and_then(|w| w.key_mode.take().map(|(_, s)| (s, w.shell.clone(), w.id))) else {
        return;
    };
    if confirm {
        shell.window_moved(id);
    } else {
        set_rect(hwnd, start);
    }
    let _ = InvalidateRect(Some(hwnd), None, false);
}

/// Tecla durante o modo: setas movem/redimensionam, Enter confirma, Esc cancela.
unsafe fn key_mode_key(hwnd: HWND, vk: usize) {
    let Some((mode, shell)) = state(hwnd).and_then(|w| w.key_mode.map(|(m, _)| (m, w.shell.clone()))) else {
        return;
    };
    match vk {
        0x0D => end_key_mode(hwnd, true),
        0x1B => end_key_mode(hwnd, false),
        _ => {
            if let Some(arrow) = Arrow::from_vk(vk) {
                let dpi = dpi_of(hwnd);
                let big = GetKeyState(VK_SHIFT.0 as i32) < 0;
                let (min_w, min_h) = (dip_to_px(MIN_W_DIP as f64, dpi), dip_to_px(MIN_H_DIP as f64, dpi));
                let next = keymode::step(rect_of(hwnd), mode, arrow, big, dpi, min_w, min_h);
                if keymode::reachable(next, &shell.monitor_bounds()) {
                    set_rect(hwnd, next);
                }
            }
        }
    }
}

unsafe fn focus_is(hwnd: HWND, f: Focus) -> bool {
    state(hwnd).is_some_and(|w| w.focus == f)
}

/// Abre o menu da nota ancorado no botão (posição estável para teclado e acessibilidade).
unsafe fn open_menu(hwnd: HWND) {
    let r = menu_button_screen_rect(hwnd);
    note_menu(hwnd, Some(POINT { x: r.left, y: r.bottom + 2 }));
}

/// Move o foco lógico entre editor e botão de menu, avisando o serviço de texto.
unsafe fn set_focus_target(hwnd: HWND, target: Focus) {
    let (changed, had_focus, ed) = match state(hwnd) {
        Some(w) => {
            let changed = w.focus != target;
            w.focus = target;
            (changed, w.has_focus, w.editor.clone())
        }
        None => return,
    };
    if !changed {
        return;
    }
    if had_focus {
        if let Some(ed) = ed {
            let msg = if target == Focus::Editor { WM_SETFOCUS } else { WM_KILLFOCUS };
            ed.set_focused(target == Focus::Editor);
            ed.send(msg, WPARAM(0), LPARAM(0));
        }
    }
    update_focus_visuals(hwnd);
    crate::a11y::notify_note_focus(hwnd);
}

/// Contorno migra para o acento só enquanto o editor está em foco (transição curta).
unsafe fn update_focus_visuals(hwnd: HWND) {
    let now = now_ms_of(hwnd);
    let reduce = sysinfo::reduce_motion();
    if let Some(w) = state(hwnd) {
        let target = if w.has_focus && w.focus == Focus::Editor && w.editor.is_some() { 1.0 } else { 0.0 };
        w.focus_ring.retarget(now, target, reduce);
    }
    start_animation(hwnd);
    let _ = InvalidateRect(Some(hwnd), None, false);
}

fn tween_running(w: &Window, now: u64) -> bool {
    w.menu_hover.is_running(now)
        || w.focus_ring.is_running(now)
        || w.button_hover.is_some_and(|(_, t)| t.is_running(now))
}

unsafe fn animations_running(hwnd: HWND) -> bool {
    let now = now_ms_of(hwnd);
    // Letreiro visível pede quadros contínuos (o timer para quando a janela some ou a view muda).
    state(hwnd).is_some_and(|w| tween_running(w, now) || (w.last_view.as_ref().is_some_and(|v| v.root.has_marquee()) && IsWindowVisible(hwnd).as_bool()))
}

/// Pede quadros (timer de ~60 Hz) apenas enquanto houver transição; parada, nenhum timer.
unsafe fn start_animation(hwnd: HWND) {
    if animations_running(hwnd) {
        SetTimer(Some(hwnd), ANIM_TIMER, 16, None);
    }
}

/// Atualiza o hover (`None` = o mouse saiu da janela) e inicia as transições.
unsafe fn update_hover(hwnd: HWND, lp: Option<LPARAM>) {
    let now = now_ms_of(hwnd);
    let reduce = sysinfo::reduce_motion();
    let mut track = false;
    if let Some(w) = state(hwnd) {
        let scale = w.dpi as f32 / 96.0;
        let (mut over_menu, mut over_button) = (false, None);
        if let Some(lp) = lp {
            let (x, y) = (((lp.0 & 0xffff) as i16 as f32) / scale, (((lp.0 >> 16) & 0xffff) as i16 as f32) / scale);
            let w_dip = w.surface.as_ref().map_or(0.0, |s| s.width as f32 / scale);
            let h_dip = w.surface.as_ref().map_or(0.0, |s| s.height as f32 / scale);
            over_menu = NoteLayout::new(w_dip, h_dip).menu.contains(x, y);
            over_button = ui::button_at(&w.placed, x, y);
            if !w.tracking {
                w.tracking = true;
                track = true;
            }
        }
        w.menu_hover.retarget(now, if over_menu { 1.0 } else { 0.0 }, reduce);
        match (&mut w.button_hover, over_button) {
            (Some((id, t)), Some(b)) if *id == b => t.retarget(now, 1.0, reduce),
            (Some((_, t)), _) => t.retarget(now, 0.0, reduce),
            (None, Some(b)) => {
                let mut t = Tween::at(0.0);
                t.retarget(now, 1.0, reduce);
                w.button_hover = Some((b, t));
            }
            (None, None) => {}
        }
    }
    if track {
        let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
        let _ = TrackMouseEvent(&mut tme);
    }
    start_animation(hwnd);
    let _ = InvalidateRect(Some(hwnd), None, false);
}

/// Aplica um tema novo a uma janela: cor do texto do editor e redesenho.
pub fn apply_theme(hwnd: HWND, theme: &Theme) {
    unsafe {
        if let Some(ed) = editor(hwnd) {
            ed.set_text_color(theme.clone().editorial().text);
        }
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut Window> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Window).as_mut()
}

unsafe fn apply_region(hwnd: HWND, dpi: u32) {
    let mut r = RECT::default();
    if GetWindowRect(hwnd, &mut r).is_err() {
        return;
    }
    let scale = dpi as f32 / 96.0;
    let radius = if state(hwnd).is_some_and(|w| w.chrome == Chrome::Tile) {
        crate::render::TILE_RADIUS_DIP
    } else {
        NoteLayout::new((r.right - r.left) as f32 / scale, (r.bottom - r.top) as f32 / scale).radius
    };
    let d = dip_to_px(radius as f64 * 2.0, dpi);
    let rgn = CreateRoundRectRgn(0, 0, r.right - r.left + 1, r.bottom - r.top + 1, d, d);
    // Em sucesso o sistema assume a posse da região.
    SetWindowRgn(hwnd, Some(rgn), true);
}

/// Área de conteúdo em DIPs, abaixo da faixa de movimentação.
fn content_area(w_dip: f32, h_dip: f32) -> RectDip {
    NoteLayout::new(w_dip, h_dip).content
}

unsafe fn paint(hwnd: HWND) {
    // 1) Copia o necessário do estado e solta o empréstimo antes de consultar o runtime.
    let (shell, id, dpi, size) = match state(hwnd) {
        Some(w) => match &w.surface {
            Some(s) => (w.shell.clone(), w.id, w.dpi, (s.width, s.height)),
            None => return,
        },
        None => return,
    };
    if state(hwnd).is_some_and(|w| w.chrome == Chrome::Tile) {
        return paint_tile(hwnd);
    }
    let scale = dpi as f32 / 96.0;
    let (w_dip, h_dip) = (size.0 as f32 / scale, size.1 as f32 / scale);
    let view = shell.runtime.borrow().view(id, SizeDip { w: w_dip, h: h_dip });
    let Ok(view) = view else { return };
    let placed = ui::layout(&view, content_area(w_dip, h_dip));

    // 2) Desenha. Perda de dispositivo só agenda a reconstrução (não recria dentro do WM_PAINT).
    let gfx = shell.gfx();
    let theme = shell.theme().editorial();
    let now = shell.now_ms();
    let mut status = shell.status_text();
    let mut lost = false;
    if let Some(w) = state(hwnd) {
        if let Some(s) = &w.surface {
            if let Some((mode, _)) = w.key_mode {
                status = keymode::status_text(mode);
            }
            let params = DrawParams {
                dpi,
                theme: &theme,
                accent: theme.accent(w.accent_index),
                placed: &placed,
                editor: w.editor.as_ref(),
                status,
                empty: find_field(&view).is_some_and(|(_, text)| text.is_empty()),
                menu_hover: w.menu_hover.value(now),
                focus_ring: if w.key_mode.is_some() { 1.0 } else { w.focus_ring.value(now) },
                menu_focus: w.has_focus && w.focus == Focus::MenuButton,
                button_hover: w.button_hover.map(|(id, t)| (id, t.value(now))),
            };
            match s.draw(&gfx, &params) {
                Ok(()) => shell.device_ok(),
                Err(e) if gfx::is_device_lost(&e) => lost = true,
                Err(e) => log(&format!("draw {}x{} dpi{}: {e}", s.width, s.height, dpi)),
            }
        }
        w.placed = placed;
    }
    if lost {
        shell.device_lost();
    }
}

/// Cartão de grade: pede a view ao widget, valida, posiciona e desenha. Uma view inválida não
/// derruba a janela: o cartão fica só com o fundo.
unsafe fn paint_tile(hwnd: HWND) {
    let Some((shell, id, dpi, size, shape)) = state(hwnd).and_then(|w| {
        w.surface.as_ref().map(|s| (w.shell.clone(), w.id, w.dpi, (s.width, s.height), w.shape))
    }) else {
        return;
    };
    let Some(shape) = shape else { return };
    let scale = dpi as f32 / 96.0;
    let (w_dip, h_dip) = (size.0 as f32 / scale, size.1 as f32 / scale);
    let ctx = ViewCtx { shape, time: sysinfo::local_time() };
    let view = match shell.runtime.borrow().tile_view(id, &ctx) {
        Ok(Some(v)) if v.validate().is_ok() => v,
        _ => pod_model::view::View::new(pod_model::view::Node::spacer()),
    };
    let gfx = shell.gfx();
    let theme = shell.theme();
    let laid = pod_model::layout::layout(&view, LRect::new(0.0, 0.0, w_dip, h_dip), &crate::render::DwMeasure(&gfx));
    let mut lost = false;
    if let Some(w) = state(hwnd) {
        w.last_view = Some(view.clone());
        if view.root.has_marquee() {
            SetTimer(Some(hwnd), ANIM_TIMER, 33, None);
        }
        w.hits = pod_model::layout::button_rects(&view, &laid);
        if let Some(s) = &w.surface {
            let params = gfx::TileDraw {
                dpi,
                theme: &theme,
                accent: theme.accent(w.accent_index),
                view: &view,
                laid: &laid,
                edit: shell.editing(),
                resizable: shell.allowed_shapes(w.id).len() > 1,
            };
            match s.draw_tile(&gfx, &params) {
                Ok(()) => shell.device_ok(),
                Err(e) if gfx::is_device_lost(&e) => lost = true,
                Err(e) => log(&format!("draw tile {}x{} dpi{}: {e}", s.width, s.height, dpi)),
            }
        }
    }
    if lost {
        shell.device_lost();
    }
}

/// Atalhos de teste do M2 até existir a bandeja: Ctrl+Shift+N nova, W remove, H oculta, A mostra todas.
unsafe fn shortcut(hwnd: HWND, vk: usize) -> bool {
    let down = |k: VIRTUAL_KEY| GetKeyState(k.0 as i32) < 0;
    if !(down(VK_CONTROL) && down(VK_SHIFT)) {
        return false;
    }
    let Some((shell, id)) = state(hwnd).map(|w| (w.shell.clone(), w.id)) else {
        return false;
    };
    match vk as u8 {
        b'N' => shell.new_note(),
        b'W' => shell.request_remove(id),
        b'H' => shell.hide(id),
        b'A' => shell.show_all(),
        #[cfg(debug_assertions)]
        b'R' => shell.toggle_simulated_removal(),
        #[cfg(debug_assertions)]
        b'L' => shell.simulate_device_loss(1),
        #[cfg(debug_assertions)]
        b'K' => shell.simulate_device_loss(3),
        #[cfg(debug_assertions)]
        b'T' => shell.cycle_theme_override(),
        _ => return false,
    }
    true
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            if let Some(w) = state(hwnd) {
                w.adopted.set(true);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CREATE => {
            let dpi = GetDpiForWindow(hwnd);
            let mut c = RECT::default();
            let _ = GetClientRect(hwnd, &mut c);
            if let Some(w) = state(hwnd) {
                w.dpi = dpi;
                let gfx = w.shell.gfx();
                match Surface::new(&gfx, hwnd, (c.right - c.left) as u32, (c.bottom - c.top) as u32) {
                    Ok(s) => w.surface = Some(s),
                    Err(_) => return LRESULT(-1),
                }
            }
            apply_region(hwnd, dpi);
            LRESULT(0)
        }
        // Sem moldura visível: toda a janela é área cliente.
        WM_NCCALCSIZE if wp.0 != 0 => LRESULT(0),
        WM_NCACTIVATE => LRESULT(1),
        // Cartão não ativa ao clicar: não rouba o foco do aplicativo do usuário e, sem ativação,
        // o Windows não o levanta sobre as janelas normais. (Notas ativam: são para digitar.)
        WM_MOUSEACTIVATE if is_tile(hwnd) => {
            LRESULT(MA_NOACTIVATE as isize)
        }
        // Arrastar pela "legenda": o tratamento padrão ativaria o cartão (e o levantaria sobre os
        // aplicativos). Inicia o movimento direto, sem ativar; WM_MOVING continua encaixando na grade.
        WM_NCLBUTTONDOWN if is_tile(hwnd) && wp.0 as u32 == HTCAPTION => {
            DefWindowProcW(hwnd, WM_SYSCOMMAND, WPARAM(0xF012), lp)
        }
        // Qualquer pedido de mudança de ordem Z (ativação por clique, mostrar, levar ao topo) vira
        // "para o fundo": os widgets ficam no desktop. Mover e redimensionar não passam por aqui.
        WM_WINDOWPOSCHANGING => {
            let pos = &mut *(lp.0 as *mut WINDOWPOS);
            let editing = state(hwnd).is_some_and(|w| w.shell.editing());
            if pos.flags.0 & SWP_NOZORDER.0 == 0 && !RAISE_OK.load(Ordering::Relaxed) && !editing && !review_windows() {
                pos.hwndInsertAfter = HWND_BOTTOM;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_SHOWWINDOW if wp.0 != 0 => {
            // Em edição os cartões novos também ficam à vista; fora dela, no fundo.
            set_edit_layer(hwnd, is_tile(hwnd) && state(hwnd).is_some_and(|w| w.shell.editing()));
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_NCHITTEST => {
            let dpi = state(hwnd).map(|w| w.dpi).unwrap_or(96);
            let mut r = RECT::default();
            let _ = GetWindowRect(hwnd, &mut r);
            let x = (lp.0 & 0xffff) as i16 as i32;
            let y = ((lp.0 >> 16) & 0xffff) as i16 as i32;
            let b = dip_to_px(BORDER_DIP as f64, dpi);
            let scale = dpi as f32 / 96.0;
            let note = NoteLayout::new((r.right - r.left) as f32 / scale, (r.bottom - r.top) as f32 / scale);
            let cap = dip_to_px(note.caption_bottom as f64, dpi);
            let (l, t, rr, bb) = (x < r.left + b, y < r.top + b, x >= r.right - b, y >= r.bottom - b);
            let mb = menu_rect_px(r, dpi);
            let on_menu = x >= mb.left && x < mb.right && y >= mb.top && y < mb.bottom;
            if state(hwnd).is_some_and(|w| w.chrome == Chrome::Tile) {
                // Formato fixo: arrasta em qualquer ponto, exceto sobre um botão (que recebe o clique).
                let on_button = state(hwnd).is_some_and(|w| {
                    let s = w.dpi as f32 / 96.0;
                    let (px, py) = ((x - r.left) as f32 / s, (y - r.top) as f32 / s);
                    w.hits.iter().any(|(_, b)| px >= b.x && px < b.right() && py >= b.y && py < b.bottom())
                });
                // Em edição: só o selo "−" é cliente; o resto do cartão (inclusive botões) arrasta.
                // O cartão inteiro é área cliente: o arrasto é tratado em WM_LBUTTONDOWN/MOUSEMOVE
                // (captura do mouse), não pelo loop modal do sistema.
                let _ = on_button;
                return LRESULT(HTCLIENT as isize);
            }
            let ht = match (l, t, rr, bb) {
                (true, true, _, _) => HTTOPLEFT,
                (_, true, true, _) => HTTOPRIGHT,
                (true, _, _, true) => HTBOTTOMLEFT,
                (_, _, true, true) => HTBOTTOMRIGHT,
                (true, ..) => HTLEFT,
                (_, true, ..) => HTTOP,
                (_, _, true, _) => HTRIGHT,
                (_, _, _, true) => HTBOTTOM,
                _ if on_menu => HTCLIENT,
                _ if y < r.top + cap => HTCAPTION,
                _ => HTCLIENT,
            };
            LRESULT(ht as isize)
        }
        WM_GETMINMAXINFO => {
            let dpi = state(hwnd).map(|w| w.dpi).unwrap_or(96);
            let mmi = &mut *(lp.0 as *mut MINMAXINFO);
            if let Some(shape) = state(hwnd).filter(|w| w.chrome == Chrome::Tile).and_then(|w| w.shape) {
                let sz = shape.size_dip();
                let (pw, ph) = (dip_to_px(sz.w as f64, dpi), dip_to_px(sz.h as f64, dpi));
                mmi.ptMinTrackSize.x = pw;
                mmi.ptMinTrackSize.y = ph;
                mmi.ptMaxTrackSize.x = pw;
                mmi.ptMaxTrackSize.y = ph;
                return LRESULT(0);
            }
            mmi.ptMinTrackSize.x = dip_to_px(MIN_W_DIP as f64, dpi);
            mmi.ptMinTrackSize.y = dip_to_px(MIN_H_DIP as f64, dpi);
            LRESULT(0)
        }
        WM_SIZE => {
            let (w_px, h_px) = ((lp.0 & 0xffff) as u32, ((lp.0 >> 16) & 0xffff) as u32);
            let dpi = match state(hwnd) {
                Some(w) => {
                    if let Some(s) = &mut w.surface {
                        if let Err(e) = s.resize(w_px, h_px) {
                            log(&format!("resize {w_px}x{h_px}: {e}"));
                        }
                    }
                    w.dpi
                }
                None => 96,
            };
            apply_region(hwnd, dpi);
            sync_editor(hwnd);
            // O loop modal de resize não entrega WM_PAINT com regularidade: desenha direto.
            paint(hwnd);
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let (placing, interacting, shell) = match state(hwnd) {
                Some(w) => {
                    w.dpi = (wp.0 & 0xffff) as u32;
                    (w.placing, w.interacting, Some(w.shell.clone()))
                }
                None => (false, false, None),
            };
            if placing {
                return LRESULT(0); // o shell calculou o retângulo exato em pixels
            }
            let r = *(lp.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                None,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if !interacting {
                // Mudança de escala feita pelo sistema (não por arrasto): refaz a partir do
                // preferido para manter o tamanho em DIPs sem acumular arredondamento.
                if let Some(shell) = shell {
                    shell.schedule_topology();
                }
            }
            LRESULT(0)
        }
        WM_PAINT | WM_DISPLAYCHANGE => {
            paint(hwnd);
            let _ = ValidateRect(Some(hwnd), None);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_GETOBJECT if lp.0 as i32 == windows::Win32::UI::Accessibility::UiaRootObjectId => {
            let root = crate::a11y::root_provider(hwnd);
            windows::Win32::UI::Accessibility::UiaReturnRawElementProvider(hwnd, wp, lp, &root)
        }
        // Mover/redimensionar por teclado: consome teclas e caracteres enquanto o modo está ativo.
        WM_KEYDOWN | WM_SYSKEYDOWN if key_mode_active(hwnd) => {
            key_mode_key(hwnd, wp.0);
            LRESULT(0)
        }
        WM_KEYUP | WM_SYSKEYUP | WM_CHAR | WM_SYSCHAR if key_mode_active(hwnd) => LRESULT(0),
        // Alt+F7 move, Alt+F8 redimensiona (teclas clássicas do Windows para janelas sem moldura).
        WM_SYSKEYDOWN if (wp.0 == 0x76 || wp.0 == 0x77) && GetKeyState(0x12) < 0 => {
            begin_key_mode(hwnd, if wp.0 == 0x76 { KeyMode::Move } else { KeyMode::Resize });
            LRESULT(0)
        }
        WM_KEYDOWN if shortcut(hwnd, wp.0) => LRESULT(0),
        // Esc sai do editor para o botão de menu (nunca no meio de uma composição IME).
        WM_KEYDOWN if wp.0 == 0x1B && focus_is(hwnd, Focus::Editor) && !state(hwnd).is_some_and(|w| w.composing) => {
            set_focus_target(hwnd, Focus::MenuButton);
            LRESULT(0)
        }
        // F6 alterna entre o editor e o botão de menu; Tab também, a partir do botão (no editor,
        // Tab é texto).
        WM_KEYDOWN if wp.0 == 0x75 && editor(hwnd).is_some() => {
            let to = if focus_is(hwnd, Focus::Editor) { Focus::MenuButton } else { Focus::Editor };
            set_focus_target(hwnd, to);
            LRESULT(0)
        }
        WM_KEYDOWN if wp.0 == 0x09 && focus_is(hwnd, Focus::MenuButton) && editor(hwnd).is_some() => {
            set_focus_target(hwnd, Focus::Editor);
            LRESULT(0)
        }
        // Menu da nota pelo teclado: Shift+F10, tecla de menu, ou Enter/Espaço no botão focado.
        // Abre ao SOLTAR a tecla: um menu aberto no pressionar fecha quando o soltar chega. F10 é
        // tecla de sistema (WM_SYSKEYDOWN/WM_SYSKEYUP).
        WM_KEYDOWN | WM_SYSKEYDOWN if is_menu_key(hwnd, wp.0) => {
            if let Some(w) = state(hwnd) {
                w.menu_key = Some(wp.0);
            }
            LRESULT(0)
        }
        WM_KEYUP | WM_SYSKEYUP if state(hwnd).is_some_and(|w| w.menu_key == Some(wp.0)) => {
            if let Some(w) = state(hwnd) {
                w.menu_key = None;
            }
            open_menu(hwnd);
            LRESULT(0)
        }
        WM_OPEN_NOTE_MENU => {
            open_menu(hwnd);
            LRESULT(0)
        }
        WM_SETFOCUS => {
            let ed = state(hwnd).and_then(|w| {
                w.has_focus = true;
                if w.focus == Focus::Editor { w.editor.clone() } else { None }
            });
            if let Some(ed) = ed {
                ed.set_focused(true);
                ed.send(WM_SETFOCUS, wp, lp);
            }
            update_focus_visuals(hwnd);
            crate::a11y::notify_note_focus(hwnd);
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            end_key_mode(hwnd, false);
            let ed = state(hwnd).and_then(|w| {
                w.has_focus = false;
                if w.focus == Focus::Editor { w.editor.clone() } else { None }
            });
            if let Some(ed) = ed {
                ed.send(WM_KILLFOCUS, wp, lp);
                ed.set_focused(false);
            }
            update_focus_visuals(hwnd);
            // Nota que perdeu o foco para outra janela (não para o próprio editor) volta ao desktop.
            let to_own_child = wp.0 != 0 && IsChild(hwnd, HWND(wp.0 as *mut _)).as_bool();
            if !is_tile(hwnd) && !to_own_child {
                send_to_bottom(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            if let Some(w) = state(hwnd) {
                w.tracking = false;
            }
            update_hover(hwnd, None);
            LRESULT(0)
        }
        WM_LBUTTONDOWN if menu_hit(hwnd, lp) => {
            open_menu(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == ANIM_TIMER => {
            let running = animations_running(hwnd);
            if !running {
                let _ = KillTimer(Some(hwnd), ANIM_TIMER);
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_NCRBUTTONUP if wp.0 as u32 == HTCAPTION => {
            note_menu(hwnd, None);
            LRESULT(0)
        }
        WM_MOUSEMOVE if is_tile(hwnd) && state(hwnd).is_some_and(|w| w.drag.is_some()) => {
            drag_to(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE if editor(hwnd).is_none() => {
            update_hover(hwnd, Some(lp));
            LRESULT(0)
        }
        WM_LBUTTONDOWN if is_tile(hwnd) => {
            // Bordas redimensionam (em qualquer modo); no modo de edição o cartão inteiro arrasta;
            // fora dele, botões recebem o clique e o resto arrasta depois de um pequeno limite.
            let (x, y) = ((lp.0 & 0xffff) as i16 as i32, ((lp.0 >> 16) & 0xffff) as i16 as i32);
            let edges = edge_at(hwnd, x, y);
            let editing = state(hwnd).is_some_and(|w| w.shell.editing());
            if edges != 0 {
                begin_drag(hwnd, true, edges);
            } else if let Some((shell, id, button)) = tile_button_at(hwnd, lp).filter(|_| !editing) {
                shell.dispatch(id, Event::Clicked { button });
            } else {
                begin_drag(hwnd, editing, 0);
            }
            LRESULT(0)
        }
        WM_SETCURSOR if is_tile(hwnd) && (lp.0 & 0xffff) as u32 == HTCLIENT => {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = ScreenToClient(hwnd, &mut pt);
            let edges = edge_at(hwnd, pt.x, pt.y);
            if let Ok(c) = LoadCursorW(None, if edges != 0 { edge_cursor(edges) } else { IDC_ARROW }) {
                SetCursor(Some(c));
            }
            LRESULT(1)
        }
        WM_LBUTTONUP if is_tile(hwnd) && state(hwnd).is_some_and(|w| w.drag.is_some()) => {
            end_drag(hwnd, true);
            LRESULT(0)
        }
        WM_CAPTURECHANGED if is_tile(hwnd) && state(hwnd).is_some_and(|w| w.drag.is_some()) => {
            end_drag(hwnd, true);
            LRESULT(0)
        }
        WM_RBUTTONUP if is_tile(hwnd) => {
            note_menu(hwnd, None);
            LRESULT(0)
        }
        // Ação principal do cartão pelo teclado: Espaço ou Enter aciona o primeiro botão.
        WM_KEYDOWN if is_tile(hwnd) && (wp.0 == 0x20 || wp.0 == 0x0D) => {
            if let Some((shell, id, button)) = state(hwnd).and_then(|w| w.hits.first().map(|(b, _)| (w.shell.clone(), w.id, *b))) {
                shell.dispatch(id, Event::Clicked { button });
            }
            LRESULT(0)
        }
        // `Invoke` do UI Automation em um botão do cartão (postado para sair da chamada do provedor).
        WM_TILE_INVOKE => {
            if let Some((shell, id)) = state(hwnd).map(|w| (w.shell.clone(), w.id)) {
                shell.dispatch(id, Event::Clicked { button: LocalId(wp.0 as u32) });
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN if editor(hwnd).is_none() => {
            // Clique em botão da view: vira evento tipado para o widget.
            let hit = state(hwnd).and_then(|w| {
                let (x, y) = ((lp.0 & 0xffff) as i16 as f32, ((lp.0 >> 16) & 0xffff) as i16 as f32);
                let s = w.dpi as f32 / 96.0;
                ui::button_at(&w.placed, x / s, y / s).map(|b| (w.shell.clone(), w.id, b))
            });
            if let Some((shell, id, button)) = hit {
                shell.dispatch(id, Event::Clicked { button });
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == CARET_TIMER => {
            if let Some(ed) = editor(hwnd) {
                ed.toggle_caret_phase();
            }
            LRESULT(0)
        }
        WM_SETCURSOR if (lp.0 & 0xffff) as u32 == HTCLIENT && editor(hwnd).is_some() => {
            if let Ok(c) = LoadCursorW(None, IDC_IBEAM) {
                SetCursor(Some(c));
            }
            LRESULT(1)
        }
        // Entrada de texto: o serviço nativo trata teclado, mouse, foco, timers e IME.
        WM_CHAR | WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP | WM_SYSCHAR | WM_LBUTTONDOWN
        | WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MOUSEMOVE
        | WM_MOUSEWHEEL | WM_TIMER | WM_IME_STARTCOMPOSITION
        | WM_IME_COMPOSITION | WM_IME_ENDCOMPOSITION | WM_IME_NOTIFY | WM_IME_CHAR | WM_CAPTURECHANGED
        | WM_UNICHAR | WM_INPUTLANGCHANGE => {
            // Mouse: atualiza o hover do botão de menu e das views; clique no editor o foca.
            if msg == WM_MOUSEMOVE {
                update_hover(hwnd, Some(lp));
            }
            if msg == WM_LBUTTONDOWN {
                set_focus_target(hwnd, Focus::Editor);
            }
            // Teclas e IME só vão ao editor quando ele tem o foco lógico.
            let is_key = matches!(
                msg,
                WM_CHAR | WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP | WM_SYSCHAR | WM_UNICHAR
                    | WM_IME_STARTCOMPOSITION | WM_IME_COMPOSITION | WM_IME_ENDCOMPOSITION | WM_IME_CHAR
            );
            if is_key && !focus_is(hwnd, Focus::Editor) {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            // Clone do editor: nenhum empréstimo do estado atravessa a chamada reentrante.
            if msg == WM_IME_STARTCOMPOSITION {
                if let Some(w) = state(hwnd) {
                    w.composing = true;
                }
            }
            let ed = editor(hwnd);
            let res = ed.as_ref().and_then(|ed| ed.send(msg, wp, lp));
            if msg == WM_IME_ENDCOMPOSITION {
                if let Some(w) = state(hwnd) {
                    w.composing = false;
                }
                sync_editor(hwnd);
            }
            deliver_text(hwnd);
            match res {
                Some(r) => r,
                None => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
        // Cartões encaixam na grade enquanto se movem (ADR-0003 P4); notas se movem livremente.
        WM_MOVING if is_tile(hwnd) => {
            let r = &mut *(lp.0 as *mut RECT);
            let dpi = state(hwnd).map_or(96, |w| w.dpi);
            let mon = MonitorFromRect(r, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if GetMonitorInfoW(mon, &mut mi).as_bool() {
                let (w, h) = (r.right - r.left, r.bottom - r.top);
                r.left = pod_model::grid::snap_axis(r.left, mi.rcWork.left, dpi);
                r.top = pod_model::grid::snap_axis(r.top, mi.rcWork.top, dpi);
                r.right = r.left + w;
                r.bottom = r.top + h;
            }
            LRESULT(1)
        }
        WM_ENTERSIZEMOVE => {
            if let Some(w) = state(hwnd) {
                w.interacting = true;
            }
            LRESULT(0)
        }
        WM_EXITSIZEMOVE => {
            // Fim de mover/redimensionar: só agora a posição vira preferência a gravar.
            let who = state(hwnd).map(|w| {
                w.interacting = false;
                (w.shell.clone(), w.id)
            });
            if let Some((shell, id)) = who {
                shell.window_moved(id);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // Fechar uma nota a oculta (recuperável pela bandeja); excluir e sair são comandos próprios.
            if let Some((shell, id)) = state(hwnd).map(|w| (w.shell.clone(), w.id)) {
                shell.hide(id);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            if let Some(ed) = editor(hwnd) {
                ed.deactivate();
            }
            let shell = state(hwnd).map(|w| w.shell.clone());
            let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Window;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !p.is_null() {
                drop(Box::from_raw(p));
            }
            let _ = shell;
            // Excluir a última janela não encerra o Pod: ele continua na bandeja (menu "Sair" encerra).
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Diagnóstico temporário de protótipo (POD_LOG=caminho).
pub fn log(msg: &str) {
    if let Ok(path) = std::env::var("POD_LOG") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{msg}");
        }
    }
}
