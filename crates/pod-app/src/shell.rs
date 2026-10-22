//! Shell: compõe as dependências e liga runtime, janelas e persistência. É o único lugar que conhece
//! os três lados (ARCHITECTURE §2). O runtime não conhece HWND; o host de janela não conhece
//! widgets; o storage não conhece nenhum dos dois.
//!
//! `runtime` e `session` ficam em `RefCell`, mas nenhum empréstimo atravessa uma chamada Win32:
//! cada método pega o empréstimo, copia o que precisa e solta antes de tocar em janelas.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Instant;

use windows::core::Result;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::WindowsAndMessaging::*;

use pod_model::ids::{WidgetId, WidgetTypeId};
use pod_model::geometry::RectPx;
use pod_model::placement::{self, Monitor, PlacementKind, PreferredPlacement};
use pod_model::schema::InstanceRecord;
use pod_model::shape::Shape;
use pod_model::widget::{Chrome, Event, Snapshot, StopReason};

use crate::gfx::{self, Gfx};
use crate::host;
use crate::monitors;
use crate::runtime::{Command, RandomIds, Runtime};
use crate::sysinfo;
use crate::ui::theme::{Mode, Theme};
use crate::storage::session::Session;
use crate::storage::tracker::SaveState;
use crate::widgets::{self, quick_notes};

pub const SCHEDULER_TIMER: usize = 1;
/// Debounce de mudanças de topologia (monitores, resolução, DPI, área útil).
pub const TOPOLOGY_TIMER: usize = 2;
/// Reavaliação de tela cheia (debounce e, enquanto ativa, verificação em baixa frequência).
pub const FULLSCREEN_TIMER: usize = 3;
/// Transição do esmaecimento dos cartões (~60 quadros por segundo durante 220 ms).
pub const FADE_TIMER: usize = 7;
/// Atualiza os espaços "+" enquanto se edita (cartões criados, movidos ou excluídos).
pub const SLOTS_TIMER: usize = 8;
const FADE_MS: u64 = 220;
/// Mensagem privada: o dispositivo gráfico precisa ser recriado.
pub const WM_DEVICE_LOST: u32 = WM_APP + 3;
/// Pede à janela de controle para abrir um painel de edição (wParam = `PanelKind`).
pub const WM_OPEN_PANEL: u32 = WM_APP + 12;
/// Mensagem privada: o primeiro plano mudou (vinda do hook de eventos).
pub const WM_FOREGROUND_CHANGED: u32 = WM_APP + 2;
const MAX_DEVICE_RECOVERIES: u32 = 6;
const TIME_MARGIN_MS: u64 = 15;
const TOPOLOGY_DEBOUNCE_MS: u32 = 400;
const DEFAULT_SIZE_DIP: (f32, f32) = (320.0, 240.0);
/// Plano de fundo para quando nenhum monitor foi enumerado (não deveria ocorrer).
const FALLBACK_RECT: RectPx = RectPx { left: 100, top: 100, right: 420, bottom: 340 };

pub struct Shell {
    pub runtime: RefCell<Runtime>,
    gfx: RefCell<Rc<Gfx>>,
    hinst: HINSTANCE,
    windows: RefCell<BTreeMap<WidgetId, HWND>>,
    started: Instant,
    pub control: Cell<HWND>,
    spawned: Cell<usize>,
    session: RefCell<Option<Session>>,
    /// Instâncias que esta versão não consegue carregar (tipo desconhecido ou estado inválido):
    /// voltam intactas ao disco em cada snapshot.
    opaque: RefCell<Vec<InstanceRecord>>,
    /// Exclusões aguardando o ack da geração que já não contém a instância.
    deleting: RefCell<Vec<(WidgetId, u64)>>,
    /// "Ocultar todas": estado transitório, distinto da preferência durável de cada nota.
    hidden_all: Cell<bool>,
    /// Monitores atuais (derivado da plataforma, nunca gravado).
    monitors: RefCell<Vec<Monitor>>,
    /// Placement preferido de cada nota (durável). A mudança de topologia nunca o reescreve.
    preferred: RefCell<BTreeMap<WidgetId, PreferredPlacement>>,
    /// Último retângulo que o próprio Pod aplicou a cada janela: distingue um movimento do
    /// usuário de um clique sem arrasto (que não deve reescrever a preferência).
    applied: RefCell<BTreeMap<WidgetId, RectPx>>,
    /// Dispositivo gráfico perdido: já há uma reconstrução agendada.
    device_lost_pending: Cell<bool>,
    /// Reconstruções consecutivas sem um desenho bem-sucedido entre elas.
    device_failures: Cell<u32>,
    /// Suspensão do sistema ou bloqueio da sessão: sem timers não essenciais.
    suspended: Cell<bool>,
    /// Notas escondidas temporariamente por um aplicativo em tela cheia (não é preferência).
    fullscreen_hidden: RefCell<std::collections::BTreeSet<WidgetId>>,
    /// Tema atual (claro, escuro ou alto contraste) e, em debug, um tema forçado para teste visual.
    theme: RefCell<Theme>,
    theme_override: Cell<Option<Mode>>,
    /// Preferência "vidro" (opt-in, durável em `preferences.glass`).
    glass: Cell<bool>,
    /// Um aplicativo está em foco: os cartões esmaecem (como os widgets de desktop do macOS).
    app_focused: Cell<bool>,
    /// Esmaecimento atual (0..1) e a animação em curso até `mute_target`.
    mute_now: Cell<f32>,
    mute_from: Cell<f32>,
    mute_target: Cell<f32>,
    mute_start: Cell<u64>,
    /// Modo de edição (ADR-0003 P4): selo "−" nos cartões e galeria "Adicionar widget" aberta.
    editing: Cell<bool>,
    gallery: Cell<Option<HWND>>,
    /// Espaços "+" do modo de edição (posição na tela e janela) e o escolhido para o próximo widget.
    slot_wins: RefCell<Vec<((i32, i32), HWND)>>,
    slot_target: Cell<Option<(i32, i32)>>,
    /// Só em debug: finge que os monitores secundários foram desconectados.
    simulate_removed: Cell<bool>,
    /// Formato em grade de cada cartão (durável no schema 3).
    shapes: RefCell<BTreeMap<WidgetId, pod_model::shape::Shape>>,
    /// Próxima fronteira de hora (relógio monotônico) que algum widget visível espera.
    time_due: Cell<Option<u64>>,
    /// `extra["shared"]` existia mas não pôde ser lido: não regravar por cima.
    shared_unreadable: Cell<bool>,
    /// Referência ao próprio `Rc` (para abrir painéis a partir de comandos).
    me: std::rc::Weak<Shell>,
    /// Provedor de métricas do sistema e o próximo prazo de leitura (None = sem assinantes).
    metrics: RefCell<crate::sysmetrics::Provider>,
    metrics_due: Cell<Option<u64>>,
    local: RefCell<crate::localdata::Provider>,
    local_due: Cell<Option<u64>>,
    media: RefCell<crate::media::Provider>,
    media_due: Cell<Option<u64>>,
    notif: RefCell<crate::notif::Provider>,
    notif_due: Cell<Option<u64>>,
    usage: RefCell<crate::claude_usage::Provider>,
    usage_due: Cell<Option<u64>>,
    /// Widgets de internet (opt-in, desligado por padrao; duravel em `preferences.internet`).
    internet: Cell<bool>,
    /// "Modo desempenho": esconde os cartões do monitor ocupado por um app em tela cheia (padrão ligado).
    perf_mode: Cell<bool>,
    weather: RefCell<crate::weather::Provider>,
    weather_due: Cell<Option<u64>>,
    weather_states: RefCell<std::collections::HashMap<String, pod_model::net::WeatherState>>,
    weather_cities: RefCell<Vec<String>>,
    news: RefCell<crate::news::Provider>,
    news_due: Cell<Option<u64>>,
    github: RefCell<crate::github::Provider>,
    quotes: RefCell<crate::quotes::Provider>,
    quotes_due: Cell<Option<u64>>,
    quotes_states: RefCell<std::collections::HashMap<pod_model::quotes::QuoteRequest, pod_model::quotes::QuoteState>>,
    quotes_reqs: RefCell<Vec<pod_model::quotes::QuoteRequest>>,
    github_due: Cell<Option<u64>>,
    github_state: RefCell<pod_model::github::GithubState>,
    news_states: RefCell<std::collections::HashMap<String, pod_model::news::NewsState>>,
    news_feeds: RefCell<Vec<String>>,
    /// Último texto de status exibido, para invalidar as janelas só quando mudar.
    last_status: RefCell<&'static str>,
}

impl Shell {
    pub fn new(gfx: Rc<Gfx>, hinst: HINSTANCE) -> Rc<Self> {
        let mut runtime = Runtime::new(Box::new(RandomIds::new()));
        widgets::register_all(&mut runtime);
        Rc::new_cyclic(|me| Self {
            me: me.clone(),
            runtime: RefCell::new(runtime),
            gfx: RefCell::new(gfx),
            hinst,
            windows: RefCell::new(BTreeMap::new()),
            started: Instant::now(),
            control: Cell::new(HWND::default()),
            spawned: Cell::new(0),
            session: RefCell::new(None),
            opaque: RefCell::new(Vec::new()),
            deleting: RefCell::new(Vec::new()),
            hidden_all: Cell::new(false),
            monitors: RefCell::new(Vec::new()),
            preferred: RefCell::new(BTreeMap::new()),
            applied: RefCell::new(BTreeMap::new()),
            device_lost_pending: Cell::new(false),
            device_failures: Cell::new(0),
            suspended: Cell::new(false),
            fullscreen_hidden: RefCell::new(std::collections::BTreeSet::new()),
            theme: RefCell::new(sysinfo::system_theme()),
            theme_override: Cell::new(None),
            glass: Cell::new(false),
            app_focused: Cell::new(false),
            mute_now: Cell::new(0.0),
            mute_from: Cell::new(0.0),
            mute_target: Cell::new(0.0),
            mute_start: Cell::new(0),
            editing: Cell::new(false),
            gallery: Cell::new(None),
            slot_wins: RefCell::new(Vec::new()),
            slot_target: Cell::new(None),
            simulate_removed: Cell::new(false),
            shapes: RefCell::new(BTreeMap::new()),
            time_due: Cell::new(None),
            shared_unreadable: Cell::new(false),
            metrics: RefCell::new(Default::default()),
            metrics_due: Cell::new(None),
            local: RefCell::new(Default::default()),
            local_due: Cell::new(None),
            media: RefCell::new(Default::default()),
            media_due: Cell::new(None),
            notif: RefCell::new(Default::default()),
            notif_due: Cell::new(None),
            usage: RefCell::new(Default::default()),
            usage_due: Cell::new(None),
            internet: Cell::new(false),
            perf_mode: Cell::new(true),
            weather: RefCell::new(Default::default()),
            weather_due: Cell::new(None),
            weather_states: RefCell::new(Default::default()),
            weather_cities: RefCell::new(Vec::new()),
            news: RefCell::new(Default::default()),
            news_due: Cell::new(None),
            github: RefCell::new(Default::default()),
            quotes: RefCell::new(Default::default()),
            quotes_due: Cell::new(None),
            quotes_states: RefCell::new(Default::default()),
            quotes_reqs: RefCell::new(Vec::new()),
            github_due: Cell::new(None),
            github_state: RefCell::new(pod_model::github::GithubState::Loading),
            news_states: RefCell::new(Default::default()),
            news_feeds: RefCell::new(Vec::new()),
            last_status: RefCell::new(""),
        })
    }

    /// Registra no log (diagnóstico) o tema que o sistema pede neste momento.
    pub fn log_theme_state(&self, origin: &str) {
        host::log(&format!(
            "tema ({origin}): sistema pede {} (AppsUseLightTheme={}, alto contraste={}); em uso: {:?}",
            if sysinfo::high_contrast_on() { "alto contraste" } else if sysinfo::apps_use_light_theme() { "claro" } else { "escuro" },
            sysinfo::apps_use_light_theme(),
            sysinfo::high_contrast_on(),
            self.theme.borrow().mode
        ));
    }

    /// `WM_SETTINGCHANGE` de tema vindo do sistema.
    pub fn on_system_theme_message(&self) {
        self.refresh_theme();
        self.log_theme_state("mensagem do sistema");
    }

    pub fn theme(&self) -> Theme {
        self.theme.borrow().clone()
    }

    /// Reavalia o tema (sistema ou forçado) e, se mudou, atualiza o editor e redesenha tudo.
    pub fn refresh_theme(&self) {
        let mut new = sysinfo::theme_for(self.theme_override.get(), self.glass.get());
        new = new.muted_by(self.mute_now.get());
        if *self.theme.borrow() == new {
            return;
        }
        *self.theme.borrow_mut() = new.clone();
        let all: Vec<HWND> = self.windows.borrow().values().copied().collect();
        for h in all {
            host::apply_theme(h, &new);
        }
        host::log(&format!("tema: {:?}", new.mode));
    }

    /// Só em debug: Claro → Escuro → Alto contraste → sistema.
    pub fn cycle_theme_override(&self) {
        let next = match self.theme_override.get() {
            None => Some(Mode::Light),
            Some(Mode::Light) => Some(Mode::Dark),
            Some(Mode::Dark) => Some(Mode::HighContrast),
            Some(Mode::HighContrast) => None,
        };
        self.theme_override.set(next);
        self.refresh_theme();
    }

    pub fn gfx(&self) -> Rc<Gfx> {
        self.gfx.borrow().clone()
    }

    /// Relógio monotônico em milissegundos desde o início do processo.
    pub fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    pub fn window_count(&self) -> usize {
        self.windows.borrow().len()
    }

    pub fn attach_session(&self, session: Session) {
        self.glass.set(session.bool_pref(crate::storage::session::PREF_GLASS));
        self.internet.set(session.bool_pref(crate::storage::session::PREF_INTERNET));
        self.perf_mode.set(!session.bool_pref(crate::storage::session::PREF_KEEP_IN_FULLSCREEN));
        self.app_focused.set(host::app_in_foreground());
        let m = if self.app_focused.get() && !self.editing.get() { 1.0 } else { 0.0 };
        (self.mute_now.set(m), self.mute_from.set(m), self.mute_target.set(m));
        self.refresh_theme();
        *self.session.borrow_mut() = Some(session);
    }

    pub fn save_state(&self) -> Option<SaveState> {
        self.session.borrow().as_ref().map(|s| s.state())
    }

    /// Cria uma instância nova (ID novo) com defaults e sua janela, em cascata no monitor principal.
    pub fn spawn(self: &Rc<Self>, type_id: &WidgetTypeId, slot: usize) -> Result<WidgetId> {
        let id = self
            .runtime
            .borrow_mut()
            .create(type_id, None)
            .map_err(|_| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
        let desc = self.runtime.borrow().descriptor_of(id);
        let shape = desc.as_ref().and_then(|d| d.default_shape);
        if let Some(sh) = shape {
            self.shapes.borrow_mut().insert(id, sh);
        }
        let size = shape.map(|sh| sh.size_dip()).map_or(DEFAULT_SIZE_DIP, |z| (z.w, z.h));
        let (rect, pref) = self.default_placement(slot, size, shape.is_some());
        if let Err(e) = self.open_window(id, rect, true) {
            self.shapes.borrow_mut().remove(&id);
            return Err(e);
        }
        if let Some(p) = pref {
            self.preferred.borrow_mut().insert(id, p);
        }
        self.mark_changed();
        Ok(id)
    }

    /// Cria um widget de cartão do tipo dado, em cascata (menu da bandeja).
    pub fn new_widget(self: &Rc<Self>, type_id: &WidgetTypeId) {
        if self.hidden_all.get() {
            self.toggle_hidden_all();
        }
        if self.monitors.borrow().is_empty() {
            self.refresh_monitors();
        }
        let slot = self.window_count() % 8;
        let _ = self.spawn(type_id, slot); // nasce no desktop, atrás dos aplicativos
        self.refresh_slots();
    }

    /// Cartões (não notas) que existem agora.
    pub fn tile_ids(&self) -> Vec<WidgetId> {
        self.windows.borrow().iter().filter(|(_, h)| host::is_tile(**h)).map(|(id, _)| *id).collect()
    }

    /// "Restaurar padrão" (modo de edição): pergunta e, se confirmado, remove os cartões e recria os 3 iniciais.
    /// As notas não são tocadas. Devolve se restaurou.
    pub fn restore_defaults(self: &Rc<Self>, owner: Option<HWND>) -> bool {
        let tiles = self.tile_ids();
        if !host::confirm_restore(owner, tiles.len()) {
            return false;
        }
        self.slot_target.set(None);
        for id in tiles {
            self.remove(id);
        }
        self.seed_first_run();
        self.save_now();
        self.refresh_slots();
        host::log("restaurar padrao: feito");
        true
    }

    /// Primeira execução: cria os cartões padrão (relógio, calendário e sistema) nas primeiras células livres da grade.
    pub fn seed_first_run(self: &Rc<Self>) {
        for type_id in crate::widgets::first_run_defaults() {
            self.new_widget(&type_id);
        }
    }

    /// Cria um cartão já centrado em `cursor` (pixels de tela) para ser arrastado da galeria.
    pub fn spawn_dragged(self: &Rc<Self>, type_id: &WidgetTypeId, cursor: (i32, i32)) -> Option<HWND> {
        if self.hidden_all.get() {
            self.toggle_hidden_all();
        }
        if self.monitors.borrow().is_empty() {
            self.refresh_monitors();
        }
        let slot = self.window_count() % 8;
        let id = self.spawn(type_id, slot).ok()?;
        let h = self.windows.borrow().get(&id).copied()?;
        let r = host::rect_of(h);
        let (w, hh) = (r.right - r.left, r.bottom - r.top);
        host::set_rect(h, RectPx { left: cursor.0 - w / 2, top: cursor.1 - hh / 2, right: cursor.0 - w / 2 + w, bottom: cursor.1 - hh / 2 + hh });
        Some(h)
    }

    /// Comando "Nova nota\": Quick Notes vazia, em cascata a partir do canto da área útil.
    pub fn new_note(self: &Rc<Self>) {
        if self.hidden_all.get() {
            self.toggle_hidden_all();
        }
        if self.monitors.borrow().is_empty() {
            self.refresh_monitors();
        }
        let slot = self.window_count() % 8;
        if let Ok(id) = self.spawn(&quick_notes::TYPE_ID, slot) {
            if let Some(h) = self.windows.borrow().get(&id).copied() {
                host::raise_note(h); // para digitar; volta ao fundo ao perder o foco
            }
        }
    }

    /// Retângulo e placement preferido de uma nota nova no monitor principal.
    fn default_placement(&self, slot: usize, size: (f32, f32), on_grid: bool) -> (RectPx, Option<PreferredPlacement>) {
        let monitors = self.monitors.borrow();
        let Some(mon) = placement::primary(&monitors).map(|i| &monitors[i]) else {
            return (FALLBACK_RECT, None);
        };
        let px = |dip: f64| (dip * mon.dpi as f64 / 96.0).round() as i32;
        let step = 24.0 + 32.0 * slot as f64;
        let (mut left, mut top) = (mon.work.left + px(step), mon.work.top + px(step));
        if on_grid {
            // Cartões nascem na primeira célula livre da grade (ADR-0003, P4); sem lugar, em cascata.
            let occupied: Vec<pod_model::grid::Box4> = self
                .windows
                .borrow()
                .values()
                .map(|h| host::rect_of(*h))
                .map(|r| (r.left, r.top, r.right, r.bottom))
                .collect();
            let work = (mon.work.left, mon.work.top, mon.work.right, mon.work.bottom);
            let (sw, sh) = (px(size.0 as f64), px(size.1 as f64));
            // O espaço "+" escolhido na edição, se o cartão cabe ali sem cobrir nada.
            let chosen = self.slot_target.take().filter(|&(l, t)| {
                let c = (l, t, l + sw, t + sh);
                c.0 >= work.0 && c.1 >= work.1 && c.2 <= work.2 && c.3 <= work.3 && !occupied.iter().any(|o| c.0 < o.2 && o.0 < c.2 && c.1 < o.3 && o.1 < c.3)
            });
            if let Some((l, t)) = chosen.or_else(|| pod_model::grid::first_free_slot(work, (sw, sh), mon.dpi, &occupied)) {
                (left, top) = (l, t);
            }
        }
        let want = RectPx {
            left,
            top,
            right: left + px(size.0 as f64),
            bottom: top + px(size.1 as f64),
        };
        let pref = placement::to_preferred(want, mon.dpi, mon);
        (placement::effective_rect(&pref, mon, None), Some(pref))
    }

    /// Restaura as instâncias gravadas. Falhas isolam só a instância afetada (ela fica opaca).
    pub fn restore(self: &Rc<Self>, records: Vec<InstanceRecord>) {
        struct Pending {
            id: WidgetId,
            record: InstanceRecord,
            visible: bool,
        }
        self.refresh_monitors();
        let mut pending = Vec::new();
        for r in records {
            match self.restore_instance(&r) {
                Ok(id) => pending.push(Pending { id, visible: r.visible, record: r }),
                Err(why) => {
                    host::log(&format!("instância {} mantida intacta: {why}", r.id));
                    self.opaque.borrow_mut().push(r);
                }
            }
        }
        // Resolve todas juntas: as remapeadas ficam em cascata por ordem estável de ID.
        let monitors = self.monitors.borrow().clone();
        let items: Vec<(WidgetId, PreferredPlacement)> =
            pending.iter().map(|p| (p.id, p.record.placement.clone())).collect();
        let resolved: BTreeMap<WidgetId, placement::Resolved> =
            placement::resolve_all(&items, &monitors).into_iter().collect();
        for p in pending {
            let rect = resolved.get(&p.id).map(|r| r.rect).unwrap_or(FALLBACK_RECT);
            // Posição absoluta do schema 1 vira preferência relativa ao monitor encontrado.
            let pref = placement::locate(&p.record.placement, &monitors)
                .map(|(_, _, converted)| converted)
                .unwrap_or_else(|| p.record.placement.clone());
            match self.open_window(p.id, rect, p.visible) {
                Ok(()) => {
                    self.preferred.borrow_mut().insert(p.id, pref);
                }
                Err(e) => {
                    host::log(&format!("janela da instância {} falhou: {e}", p.id.to_hex()));
                    self.opaque.borrow_mut().push(p.record);
                }
            }
        }
    }

    fn restore_instance(&self, r: &InstanceRecord) -> std::result::Result<WidgetId, String> {
        let id = WidgetId::from_hex(&r.id).ok_or("id inválido")?;
        let type_id = self.runtime.borrow().find_type(&r.type_id).ok_or("tipo desconhecido")?;
        let snapshot = Snapshot { version: r.state_version, data: r.state.clone() };
        self.runtime.borrow_mut().restore(id, &type_id, &snapshot).map_err(|e| format!("{e:?}"))?;
        // Cartão de grade: o formato gravado vale se o tipo o aceita; senão, o padrão do tipo.
        if let Some(d) = self.runtime.borrow().descriptor_of(id).filter(|d| d.chrome == Chrome::Tile) {
            let saved = r.shape.as_deref().and_then(Shape::parse).filter(|s| d.shapes.contains(s));
            if let Some(sh) = saved.or(d.default_shape) {
                self.shapes.borrow_mut().insert(id, sh);
            }
        }
        Ok(id)
    }

    /// Janela + Loaded → Mounted → Running. Se a janela falhar, nada fica no runtime.
    fn open_window(self: &Rc<Self>, id: WidgetId, rect: RectPx, visible: bool) -> Result<()> {
        let n = self.spawned.get();
        self.spawned.set(n + 1);
        let chrome = self.runtime.borrow().descriptor_of(id).map_or(Chrome::Note, |d| d.chrome);
        let shape = self.shapes.borrow().get(&id).copied();
        // SAFETY: thread de UI; `create_window` documenta a posse do Box.
        let accent = (chrome == Chrome::Tile)
            .then(|| self.runtime.borrow().descriptor_of(id).and_then(|d| crate::widgets::accent_for(&d.type_id)))
            .flatten()
            .unwrap_or(n % 4);
        let created = unsafe {
            host::create_window(self, self.hinst, id, (rect.left, rect.top), accent, chrome, shape)
        };
        let hwnd = match created {
            Ok(h) => h,
            Err(e) => {
                self.runtime.borrow_mut().remove(id, StopReason::Removed);
                return Err(e);
            }
        };
        host::set_rect(hwnd, rect);
        if let Some(title) = self.runtime.borrow().descriptor_of(id).and_then(|d| widgets::title_of(&d.type_id)) {
            host::set_title(hwnd, title); // nome da raiz no UI Automation
        }
        if let Some(sh) = shape {
            host::fit_tile(hwnd, sh); // tamanho exato do formato no DPI real da janela
        }
        self.applied.borrow_mut().insert(id, host::rect_of(hwnd));
        self.windows.borrow_mut().insert(id, hwnd);
        let now = self.now_ms();
        let started = {
            let mut rt = self.runtime.borrow_mut();
            let _ = rt.mount(id);
            let cmds = rt.start(id, now).unwrap_or_default();
            let _ = rt.set_visible(id, visible);
            cmds
        };
        // O editor depende da view (campo de texto) e só existe depois de montar a instância.
        host::sync_editor(hwnd);
        if visible && !self.hidden_all.get() {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        }
        // Comandos do `Started` (por exemplo, a hora inicial de quem assina o tempo).
        self.handle_commands(started);
        self.flush();
        self.reschedule();
        Ok(())
    }

    // ------------------------------------------------------------------ topologia

    /// Reenumera os monitores (em debug, `simulate_removed` finge que só o principal existe).
    pub fn refresh_monitors(&self) {
        let mut ms = monitors::enumerate();
        if self.simulate_removed.get() {
            ms.retain(|m| m.primary);
        }
        for m in &ms {
            host::log(&format!(
                "monitor: {} primario={} dpi={} bounds=({},{},{},{}) work=({},{},{},{}) edid={:?} path={}",
                m.name, m.primary, m.dpi, m.bounds.left, m.bounds.top, m.bounds.right, m.bounds.bottom,
                m.work.left, m.work.top, m.work.right, m.work.bottom, m.edid_id, m.device_path
            ));
        }
        *self.monitors.borrow_mut() = ms;
    }

    /// Agenda a reavaliação (debounce): monitores, resolução, DPI e área útil mudam em rajadas.
    pub fn schedule_topology(&self) {
        let control = self.control.get();
        if !control.0.is_null() {
            unsafe {
                SetTimer(Some(control), TOPOLOGY_TIMER, TOPOLOGY_DEBOUNCE_MS, None);
            }
        }
    }

    /// Recalcula o placement efetivo de todas as notas a partir do preferido e reposiciona as
    /// janelas. O preferido não é tocado: reconectar o monitor restaura a posição original.
    pub fn apply_topology(&self) {
        self.refresh_monitors();
        let monitors = self.monitors.borrow().clone();
        let live: Vec<WidgetId> = self.windows.borrow().keys().copied().collect();
        let items: Vec<(WidgetId, PreferredPlacement)> = {
            let pref = self.preferred.borrow();
            live.iter().filter_map(|id| pref.get(id).map(|p| (*id, p.clone()))).collect()
        };
        let resolved = placement::resolve_all(&items, &monitors);
        let remapped = resolved.iter().filter(|(_, r)| r.kind == PlacementKind::Remapped).count();
        for (id, r) in &resolved {
            if let Some(h) = self.windows.borrow().get(id).copied() {
                host::move_to(h, r.rect);
                self.applied.borrow_mut().insert(*id, host::rect_of(h));
            }
        }
        host::log(&format!(
            "topologia: {} monitor(es), {} nota(s), {} remapeada(s)",
            monitors.len(),
            resolved.len(),
            remapped
        ));
    }

    /// Fim de um movimento ou redimensionamento feito pelo usuário: confirma nova preferência
    /// (inclusive quando a nota estava remapeada para o monitor principal).
    pub fn window_moved(&self, id: WidgetId) {
        let Some(hwnd) = self.windows.borrow().get(&id).copied() else { return };
        if self.monitors.borrow().is_empty() {
            self.refresh_monitors();
        }
        let rect = host::rect_of(hwnd);
        if self.applied.borrow().get(&id) == Some(&rect) {
            return; // nada se moveu: um clique não confirma preferência
        }
        self.applied.borrow_mut().insert(id, rect);
        let dpi = host::dpi_of(hwnd);
        let pref = {
            let monitors = self.monitors.borrow();
            placement::monitor_for_rect(rect, &monitors).map(|i| placement::to_preferred(rect, dpi, &monitors[i]))
        };
        if let Some(p) = pref {
            self.preferred.borrow_mut().insert(id, p);
            self.mark_changed();
        }
    }

    // ------------------------------------------------------------------ suspensão e sessão

    /// Suspensão do sistema ou bloqueio da sessão: tenta gravar as pendências e para o agendador.
    /// O estado continua em memória; nada é apagado.
    pub fn on_suspend(&self) {
        if self.suspended.replace(true) {
            return;
        }
        let pending = self.session.borrow().as_ref().is_some_and(|s| s.has_unsent_changes());
        if pending {
            self.save_now();
        }
        let control = self.control.get();
        if !control.0.is_null() {
            unsafe {
                let _ = KillTimer(Some(control), SCHEDULER_TIMER);
            }
        }
        host::log("suspensão/bloqueio: pendências enviadas, timers parados");
    }

    /// Retorno: revalida topologia, bandeja e desenho. Prazos vencidos disparam uma vez (o
    /// agendador não repete ticks perdidos).
    pub fn on_resume(&self) {
        self.suspended.set(false);
        self.schedule_topology();
        let control = self.control.get();
        if !control.0.is_null() {
            crate::tray::add(control);
        }
        let all: Vec<HWND> = self.windows.borrow().values().copied().collect();
        for h in all {
            unsafe {
                let _ = InvalidateRect(Some(h), None, false);
            }
        }
        self.deliver_time();
        self.metrics_due.set(None);
        self.local_due.set(None);
        self.metrics.borrow_mut().forget_cpu();
        self.reschedule();
        host::log("retorno: topologia, bandeja e desenho revalidados");
    }

    // ------------------------------------------------------------------ perda de dispositivo

    /// Um desenho falhou por perda do dispositivo. Pode ser chamado de dentro do WM_PAINT, então só
    /// agenda a reconstrução.
    pub fn device_lost(&self) {
        if self.device_lost_pending.replace(true) {
            return;
        }
        let control = self.control.get();
        if !control.0.is_null() {
            unsafe {
                let _ = PostMessageW(Some(control), WM_DEVICE_LOST, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// Um desenho bem-sucedido encerra a sequência de falhas.
    pub fn device_ok(&self) {
        if self.device_failures.get() != 0 {
            self.device_failures.set(0);
        }
    }

    /// Recria dispositivo, superfícies e invalida tudo, com o mesmo estado de UI. Depois de falhas
    /// repetidas tenta WARP; depois de muitas, para (sem laço infinito) e mantém os dados.
    pub fn recover_devices(&self) {
        self.device_lost_pending.set(false);
        gfx::consume_loss_round();
        let n = self.device_failures.get() + 1;
        self.device_failures.set(n);
        if n > MAX_DEVICE_RECOVERIES {
            host::log("dispositivo gráfico: recuperação esgotada; dados preservados, janelas sem desenho");
            return;
        }
        let warp = n >= 3;
        match Gfx::new(warp) {
            Ok(g) => {
                *self.gfx.borrow_mut() = Rc::new(g);
                let all: Vec<HWND> = self.windows.borrow().values().copied().collect();
                for h in all {
                    host::rebuild_surface(h);
                }
                host::log(&format!(
                    "dispositivo gráfico recriado (tentativa {n}, {})",
                    if warp { "WARP, modo degradado" } else { "adaptador padrão" }
                ));
            }
            Err(e) => {
                host::log(&format!("falha ao recriar o dispositivo (tentativa {n}): {e}"));
                self.device_lost();
            }
        }
    }

    // ------------------------------------------------------------------ tela cheia

    /// Heurística conservadora: o primeiro plano cobre um monitor inteiro (limites de moldura do
    /// DWM, tolerância de 2 px), excluindo o Pod e o shell. Esconde temporariamente as notas
    /// daquele monitor, sem tocar na preferência de visibilidade. Não identifica jogos.
    pub fn evaluate_fullscreen(&self) {
        self.update_activation(); // confere de novo depois que o primeiro plano assentou
        let monitors = self.monitors.borrow().clone();
        let busy = if self.perf_mode.get() { host::fullscreen_monitor(&monitors) } else { None };
        let control = self.control.get();
        let mut now_hidden = std::collections::BTreeSet::new();
        if let Some(idx) = busy {
            let windows: Vec<(WidgetId, HWND)> = self.windows.borrow().iter().map(|(i, h)| (*i, *h)).collect();
            for (id, h) in windows {
                let on_busy = placement::monitor_for_rect(host::rect_of(h), &monitors) == Some(idx);
                if on_busy {
                    now_hidden.insert(id);
                }
            }
        }
        let previous = self.fullscreen_hidden.borrow().clone();
        for id in previous.difference(&now_hidden) {
            // Voltou: só reaparece o que o usuário quer visível e não está em "ocultar todas".
            let wanted = self.runtime.borrow().is_visible(*id) != Some(false) && !self.hidden_all.get();
            if let (true, Some(h)) = (wanted, self.windows.borrow().get(id).copied()) {
                unsafe {
                    let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
                }
            }
        }
        for id in now_hidden.difference(&previous) {
            if let Some(h) = self.windows.borrow().get(id).copied() {
                unsafe {
                    let _ = ShowWindow(h, SW_HIDE);
                }
            }
        }
        if previous != now_hidden {
            host::log(&format!("tela cheia: {} nota(s) escondida(s) temporariamente", now_hidden.len()));
        }
        let active = busy.is_some();
        *self.fullscreen_hidden.borrow_mut() = now_hidden;
        // Enquanto houver tela cheia, reavalia em baixa frequência (o tamanho do primeiro plano
        // pode mudar sem um novo evento de foco).
        if !control.0.is_null() {
            unsafe {
                if active {
                    SetTimer(Some(control), FULLSCREEN_TIMER, 1000, None);
                } else {
                    let _ = KillTimer(Some(control), FULLSCREEN_TIMER);
                }
            }
        }
    }

    /// Debounce do evento de primeiro plano.
    pub fn schedule_fullscreen_check(&self) {
        let control = self.control.get();
        if !control.0.is_null() {
            unsafe {
                SetTimer(Some(control), FULLSCREEN_TIMER, 500, None);
            }
        }
    }

    /// Só em debug: simula a perda do dispositivo gráfico por `rounds` reconstruções.
    pub fn simulate_device_loss(&self, rounds: u32) {
        gfx::force_loss_rounds(rounds);
        let all: Vec<HWND> = self.windows.borrow().values().copied().collect();
        for h in all {
            unsafe {
                let _ = InvalidateRect(Some(h), None, false);
            }
        }
    }

    /// Retângulos totais dos monitores atuais (para manter uma nota movida por teclado alcançável).
    pub fn monitor_bounds(&self) -> Vec<RectPx> {
        if self.monitors.borrow().is_empty() {
            self.refresh_monitors();
        }
        self.monitors.borrow().iter().map(|m| m.bounds).collect()
    }

    /// Só em debug: alterna a simulação de monitores secundários desconectados.
    pub fn toggle_simulated_removal(&self) {
        self.simulate_removed.set(!self.simulate_removed.get());
        self.apply_topology();
    }

    /// Exclusão pedida pelo usuário: grava um snapshot sem a instância e só destrói depois do ack.
    /// Se a gravação falhar, a nota permanece acessível.
    pub fn request_remove(&self, id: WidgetId) {
        // Excluir uma nota com conteúdo exige confirmação local.
        if self.runtime.borrow().has_content(id) {
            let owner = self.windows.borrow().get(&id).copied();
            if !host::confirm_delete(owner) {
                return;
            }
        }
        if self.session.borrow().is_none() {
            self.remove(id);
            return;
        }
        if self.deleting.borrow().iter().any(|(d, _)| *d == id) {
            return;
        }
        // O ID entra na lista antes de montar os registros para ser excluído deles.
        self.deleting.borrow_mut().push((id, 0));
        let generation = self.save_now();
        if let Some(entry) = self.deleting.borrow_mut().iter_mut().find(|(d, _)| *d == id) {
            entry.1 = generation;
        }
    }

    /// Para a instância, libera seus prazos e destrói a janela. Idempotente.
    pub fn remove(&self, id: WidgetId) {
        self.deleting.borrow_mut().retain(|(d, _)| *d != id);
        self.preferred.borrow_mut().remove(&id);
        self.applied.borrow_mut().remove(&id);
        let hwnd = self.windows.borrow_mut().remove(&id);
        let commands = self.runtime.borrow_mut().remove(id, StopReason::Removed);
        self.handle_commands(commands);
        if let Some(h) = hwnd {
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
        self.reschedule();
    }

    pub fn dispatch(&self, id: WidgetId, event: Event) {
        let now = self.now_ms();
        let result = {
            let mut rt = self.runtime.borrow_mut();
            rt.set_now(now);
            rt.dispatch(id, &event)
        };
        if let Ok(commands) = result {
            self.handle_commands(commands);
        }
        self.flush();
        self.reschedule();
    }

    /// Chamado pelo timer único da janela de controle: prazos de widgets e autosave.
    pub fn on_timer(&self) {
        let now = self.now_ms();
        let commands = self.runtime.borrow_mut().tick(now);
        self.handle_commands(commands);
        if self.time_due.get().is_some_and(|d| now >= d) {
            self.deliver_time();
        }
        if self.metrics_due.get().is_some_and(|d| now >= d) {
            self.deliver_metrics();
        }
        if self.local_due.get().is_some_and(|d| now >= d) {
            self.deliver_local();
        }
        if self.media_due.get().is_some_and(|d| now >= d) {
            self.deliver_media();
        }
        if self.notif_due.get().is_some_and(|d| now >= d) {
            self.deliver_notifications();
        }
        if self.usage_due.get().is_some_and(|d| now >= d) {
            self.deliver_usage();
        }
        if self.weather_due.get().is_some_and(|d| now >= d) {
            self.deliver_weather();
        }
        if self.news_due.get().is_some_and(|d| now >= d) {
            self.deliver_news();
        }
        if self.github_due.get().is_some_and(|d| now >= d) {
            self.deliver_github();
        }
        if self.quotes_due.get().is_some_and(|d| now >= d) {
            self.deliver_quotes();
        }
        let due = self.session.borrow().as_ref().is_some_and(|s| s.should_save(now));
        if due {
            self.save_now();
        }
        self.flush();
        self.reschedule();
    }

    /// Formatos que o tipo do cartão declara (para a alça de redimensionar).
    pub fn allowed_shapes(&self, id: WidgetId) -> Vec<Shape> {
        self.runtime.borrow().descriptor_of(id).map(|d| d.shapes.clone()).unwrap_or_default()
    }

    /// Muda o formato de um cartão (menu "Tamanho"): só os formatos que o tipo declara.
    pub fn set_shape(&self, id: WidgetId, shape: Shape) {
        let allowed = self.runtime.borrow().descriptor_of(id).is_some_and(|d| d.shapes.contains(&shape));
        let Some(hwnd) = self.windows.borrow().get(&id).copied().filter(|_| allowed) else { return };
        if self.shapes.borrow().get(&id) == Some(&shape) {
            return;
        }
        self.shapes.borrow_mut().insert(id, shape);
        host::set_tile_shape(hwnd, shape);
        self.window_moved(id); // o tamanho novo também é preferência durável
        self.dispatch(id, Event::ShapeChanged(shape));
        self.mark_changed();
    }

    /// Item de menu do widget escolhido pelo usuário.
    pub fn menu_command(&self, id: WidgetId, command: u32) {
        self.dispatch(id, Event::MenuCommand(command));
    }

    /// Entrega a hora atual aos widgets assinantes visíveis.
    fn deliver_time(&self) {
        let commands = self.runtime.borrow_mut().tick_time(sysinfo::local_time());
        self.handle_commands(commands);
        let commands = self.runtime.borrow_mut().tick_zones(&sysinfo::zone_time);
        self.handle_commands(commands);
    }

    /// Abre um link salvo no navegador padrão. O endereço vem dos dados compartilhados e é validado
    /// de novo aqui (só http/https, sem usuário/senha) antes de ir ao sistema.
    fn open_link(&self, id: u64) {
        let url = self.runtime.borrow().shared().link(id).map(|l| l.url.clone());
        let Some(url) = url else { return };
        // Só para testes: registra o que seria aberto sem acionar o navegador.
        #[cfg(debug_assertions)]
        if std::env::var_os("POD_DRY_RUN_LINKS").is_some() {
            host::log(&format!("link (simulado) id={id} url={url}"));
            return;
        }
        match pod_model::shared::clean_url(&url) {
            Ok(safe) if safe == url => unsafe {
                use windows::Win32::UI::Shell::ShellExecuteW;
                let wide = windows::core::HSTRING::from(url.as_str());
                let r = ShellExecuteW(None, windows::core::w!("open"), &wide, None, None, SW_SHOWNORMAL);
                if r.0 as isize <= 32 {
                    host::log(&format!("link não aberto (ShellExecute {})", r.0 as isize));
                }
            },
            _ => host::log("link recusado: endereço inválido"),
        }
    }

    /// Lê as métricas que os assinantes visíveis pedem e as entrega. Sem assinantes, não lê nada.
    fn deliver_metrics(&self) {
        let needs = self.runtime.borrow().metrics_needs();
        if needs.is_empty() {
            self.metrics_due.set(None);
            self.metrics.borrow_mut().forget_cpu();
            return;
        }
        let sample = self.metrics.borrow_mut().read(needs);
        let commands = self.runtime.borrow_mut().tick_metrics(sample);
        self.handle_commands(commands);
        self.metrics_due.set(Some(self.now_ms() + needs.interval_ms()));
    }

    pub fn internet(&self) -> bool {
        self.internet.get()
    }

    pub fn perf_mode(&self) -> bool {
        self.perf_mode.get()
    }

    /// Liga/desliga o modo desempenho; ao desligar, os cartões escondidos por tela cheia voltam na hora.
    pub fn toggle_perf_mode(&self) {
        let on = !self.perf_mode.get();
        self.perf_mode.set(on);
        if let Some(s) = self.session.borrow_mut().as_mut() {
            s.set_bool_pref(crate::storage::session::PREF_KEEP_IN_FULLSCREEN, !on);
        }
        self.evaluate_fullscreen();
    }

    /// Liga/desliga "Widgets de internet" (opt-in, desligado por padrao): grava a preferencia, para a
    /// thread de rede ao desligar e avisa os widgets na hora.
    pub fn toggle_internet(&self) {
        let on = !self.internet.get();
        self.internet.set(on);
        if let Some(s) = self.session.borrow_mut().as_mut() {
            s.set_bool_pref(crate::storage::session::PREF_INTERNET, on);
        }
        if !on {
            self.weather.borrow_mut().stop();
            self.weather_states.borrow_mut().clear();
            self.weather_cities.borrow_mut().clear();
            self.news.borrow_mut().stop();
            self.news_states.borrow_mut().clear();
            self.news_feeds.borrow_mut().clear();
            self.github.borrow_mut().stop();
            *self.github_state.borrow_mut() = pod_model::github::GithubState::Loading;
            self.quotes.borrow_mut().stop();
            self.quotes_states.borrow_mut().clear();
            self.quotes_reqs.borrow_mut().clear();
        }
        host::log(&format!("internet: {on}"));
        self.deliver_weather();
        self.deliver_news();
        self.deliver_github();
        self.deliver_quotes();
        self.reschedule();
        self.mark_changed();
    }

    /// Entrega o clima aos widgets Weather visiveis. Sem o opt-in so entrega `Off` e nada sai.
    fn deliver_weather(&self) {
        use pod_model::net::WeatherState;
        if self.runtime.borrow().weather_cities().is_empty() {
            self.weather_due.set(None);
            self.weather.borrow_mut().stop();
            self.weather_cities.borrow_mut().clear();
            return;
        }
        if self.internet.get() {
            let results = self.weather.borrow_mut().take_results();
            let mut states = self.weather_states.borrow_mut();
            for (city, st) in results {
                states.insert(city, st);
            }
        }
        let internet = self.internet.get();
        let states = self.weather_states.borrow().clone();
        let state_of = move |city: &str| -> WeatherState {
            if !internet {
                WeatherState::Off
            } else {
                states.get(city).cloned().unwrap_or(WeatherState::Loading)
            }
        };
        let commands = self.runtime.borrow_mut().tick_weather(&state_of);
        self.handle_commands(commands);
        self.weather_due.set(Some(self.now_ms() + 1000));
    }

    /// Entrega as manchetes aos widgets News visiveis. Sem o opt-in so entrega `Off` e nada sai.
    fn deliver_news(&self) {
        use pod_model::news::NewsState;
        if self.runtime.borrow().news_feeds().is_empty() {
            self.news_due.set(None);
            self.news.borrow_mut().stop();
            self.news_feeds.borrow_mut().clear();
            return;
        }
        if self.internet.get() {
            let results = self.news.borrow_mut().take_results();
            let mut states = self.news_states.borrow_mut();
            for (feed, st) in results {
                states.insert(feed, st);
            }
        }
        let internet = self.internet.get();
        let states = self.news_states.borrow().clone();
        let state_of = move |feed: &str| -> NewsState {
            if !internet {
                NewsState::Off
            } else {
                states.get(feed).cloned().unwrap_or(NewsState::Loading)
            }
        };
        let commands = self.runtime.borrow_mut().tick_news(&state_of);
        self.handle_commands(commands);
        self.news_due.set(Some(self.now_ms() + 1000));
    }

    /// Entrega o resumo do GitHub. Sem o opt-in so entrega `Off`; o token nunca sai do cofre/provedor.
    fn deliver_github(&self) {
        use pod_model::github::GithubState;
        if !self.runtime.borrow().wants_github() {
            self.github_due.set(None);
            self.github.borrow_mut().stop();
            return;
        }
        let state = if self.internet.get() {
            if let Some(s) = self.github.borrow_mut().take_latest() {
                *self.github_state.borrow_mut() = s;
            }
            self.github_state.borrow().clone()
        } else {
            GithubState::Off
        };
        let commands = self.runtime.borrow_mut().tick_github(&state);
        self.handle_commands(commands);
        self.github_due.set(Some(self.now_ms() + 1000));
    }

    /// Entrega as cotações aos widgets Bolsa visíveis. Sem o opt-in só entrega `Off` e nada sai.
    fn deliver_quotes(&self) {
        use pod_model::quotes::QuoteState;
        if self.runtime.borrow().quote_requests().is_empty() {
            self.quotes_due.set(None);
            self.quotes.borrow_mut().stop();
            self.quotes_reqs.borrow_mut().clear();
            return;
        }
        let internet = self.internet.get();
        if internet {
            let results = self.quotes.borrow_mut().take_results();
            let mut states = self.quotes_states.borrow_mut();
            for (req, st) in results {
                states.insert(req, st);
            }
        }
        let states = self.quotes_states.borrow().clone();
        let state_of = move |req: &pod_model::quotes::QuoteRequest| -> QuoteState {
            if !internet {
                QuoteState::Off
            } else {
                states.get(req).cloned().unwrap_or(QuoteState::Loading)
            }
        };
        let commands = self.runtime.borrow_mut().tick_quotes(&state_of);
        self.handle_commands(commands);
        self.quotes_due.set(Some(self.now_ms() + 1000));
    }

    /// Um token novo entrou no cofre (ou saiu): reconsulta na hora.
    pub fn github_token_changed(&self) {
        *self.github_state.borrow_mut() = pod_model::github::GithubState::Loading;
        self.github.borrow().refresh_now();
        self.deliver_github();
    }

    /// Abre a materia da manchete `index` do feed do widget. O endereco vem do que o proprio host buscou
    /// (nunca do widget) e passa de novo por `clean_url` (so http/https, sem usuario).
    fn open_news(&self, id: WidgetId, index: usize) {
        let Some(feed) = self.runtime.borrow().news_feed_of(id) else { return };
        let url = match self.news_states.borrow().get(&feed) {
            Some(pod_model::news::NewsState::Ready { items, .. }) => items.get(index).map(|h| h.link.clone()),
            _ => None,
        };
        let Some(url) = url else { return };
        #[cfg(debug_assertions)]
        if std::env::var_os("POD_DRY_RUN_LINKS").is_some() {
            host::log(&format!("manchete (simulada) feed={feed} n={index} url={url}"));
            return;
        }
        match pod_model::shared::clean_url(&url) {
            Ok(safe) if safe == url => unsafe {
                use windows::Win32::UI::Shell::ShellExecuteW;
                let wide = windows::core::HSTRING::from(url.as_str());
                let r = ShellExecuteW(None, windows::core::w!("open"), &wide, None, None, SW_SHOWNORMAL);
                if r.0 as isize <= 32 {
                    host::log(&format!("manchete nao aberta (ShellExecute {})", r.0 as isize));
                }
            },
            _ => host::log("manchete recusada: endereco invalido"),
        }
    }

    /// Entrega o uso do Claude Code (lido de ~/.claude/projects em outra thread) aos assinantes visiveis.
    fn deliver_usage(&self) {
        if !self.runtime.borrow().usage_wanted() {
            self.usage_due.set(None);
            self.usage.borrow_mut().stop();
            return;
        }
        let latest = self.usage.borrow_mut().take_latest();
        if let Some(snap) = latest {
            let commands = self.runtime.borrow_mut().tick_usage(snap);
            self.handle_commands(commands);
        }
        self.usage_due.set(Some(self.now_ms() + 1000));
    }

    /// Entrega as notificacoes mais recentes aos assinantes visiveis (a leitura roda em outra thread).
    fn deliver_notifications(&self) {
        if !self.runtime.borrow().notifications_wanted() {
            self.notif_due.set(None);
            self.notif.borrow_mut().stop();
            return;
        }
        let latest = self.notif.borrow_mut().take_latest();
        if let Some(snap) = latest {
            let commands = self.runtime.borrow_mut().tick_notifications(snap);
            self.handle_commands(commands);
        }
        self.notif_due.set(Some(self.now_ms() + 1000));
    }

    /// Entrega a sessao de midia mais recente aos assinantes visiveis (a leitura roda em outra thread).
    fn deliver_media(&self) {
        if !self.runtime.borrow().media_wanted() {
            self.media_due.set(None);
            self.media.borrow_mut().stop();
            return;
        }
        let latest = self.media.borrow_mut().take_latest();
        if let Some(snap) = latest {
            let commands = self.runtime.borrow_mut().tick_media(snap);
            self.handle_commands(commands);
        }
        self.media_due.set(Some(self.now_ms() + 500));
    }

    /// Lê pastas e área de transferência só se algum assinante visível pediu.
    fn deliver_local(&self) {
        let needs = self.runtime.borrow().local_needs();
        if needs.is_empty() {
            self.local_due.set(None);
            self.local.borrow_mut().reset();
            return;
        }
        let snap = self.local.borrow_mut().read(needs);
        let commands = self.runtime.borrow_mut().tick_local(snap);
        self.handle_commands(commands);
        self.local_due.set(Some(self.now_ms() + needs.interval_ms()));
    }

    /// O relógio do sistema foi acertado (ou mudou o fuso/horário de verão): rele a hora já.
    pub fn on_time_changed(&self) {
        self.deliver_time();
        self.flush();
        self.reschedule();
    }

    /// Geometria ou visibilidade mudaram: o snapshot precisa ser regravado.
    pub fn mark_changed(&self) {
        let now = self.now_ms();
        if let Some(s) = self.session.borrow_mut().as_mut() {
            s.mark_changed(now);
        }
        self.refresh_status();
        self.reschedule();
    }

    pub fn hide(&self, id: WidgetId) {
        if self.runtime.borrow_mut().set_visible(id, false).is_ok() {
            if let Some(h) = self.windows.borrow().get(&id).copied() {
                unsafe {
                    let _ = ShowWindow(h, SW_HIDE);
                }
            }
            self.mark_changed();
        }
    }

    pub fn show_all(&self) {
        let all: Vec<(WidgetId, HWND)> = self.windows.borrow().iter().map(|(i, h)| (*i, *h)).collect();
        let mut changed = false;
        for (id, h) in all {
            if self.runtime.borrow().is_visible(id) == Some(false) {
                let _ = self.runtime.borrow_mut().set_visible(id, true);
                unsafe {
                    let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
                }
                changed = true;
            }
        }
        if changed {
            self.deliver_time();
            self.metrics_due.set(None); // volta a ler já, com o ponto de partida refeito
            self.local_due.set(None);
            self.flush();
            self.mark_changed();
        }
    }

    /// Texto de status exibido no rodapé de cada nota. "Salvo" só existe após o ack do worker.
    pub fn status_text(&self) -> &'static str {
        match self.save_state() {
            None => "",
            Some(SaveState::Saved) => "Salvo",
            Some(SaveState::Dirty) => "Salvando\u{2026}",
            Some(SaveState::Error(_)) => "Erro ao salvar \u{2014} tentando de novo",
        }
    }

    fn refresh_status(&self) {
        let now = self.status_text();
        if *self.last_status.borrow() == now {
            return;
        }
        *self.last_status.borrow_mut() = now;
        let all: Vec<HWND> = self.windows.borrow().values().copied().collect();
        for h in all {
            unsafe {
                let _ = InvalidateRect(Some(h), None, false);
            }
        }
    }

    pub fn editing(&self) -> bool {
        self.editing.get()
    }

    /// Liga/desliga o modo de edição: abre ou fecha a galeria e redesenha os cartões (selo "−").
    pub fn set_editing(self: &Rc<Self>, on: bool) {
        if self.editing.get() == on {
            return;
        }
        self.editing.set(on);
        host::log(&format!("edicao: {on}"));
        self.retarget_mute();
        if on {
            if self.gallery.get().is_none() {
                self.gallery.set(crate::dock::open(self));
            }
            if self.gallery.get().is_none() {
                self.editing.set(false); // não abriu: não fica "preso" em edição
            }
        } else if let Some(h) = self.gallery.take() {
            crate::dock::close(h);
        }
        self.sync_slots();
        let all: Vec<HWND> = self.windows.borrow().values().copied().collect();
        for h in all {
            if host::is_tile(h) {
                host::set_edit_layer(h, on);
            }
            unsafe {
                let _ = InvalidateRect(Some(h), None, false);
            }
        }
    }

    pub fn slot_target(&self) -> Option<(i32, i32)> {
        self.slot_target.get()
    }

    /// Clicar num espaço "+": escolhe (ou, clicando de novo, desfaz) onde o próximo widget vai.
    pub fn select_slot(&self, pos: (i32, i32)) {
        self.slot_target.set(if self.slot_target.get() == Some(pos) { None } else { Some(pos) });
        for (_, h) in self.slot_wins.borrow().iter() {
            crate::slot::redraw(*h);
        }
    }

    /// Liga os espaços "+" (e o temporizador que os mantém em dia) ao entrar na edição; desliga ao sair.
    fn sync_slots(self: &Rc<Self>) {
        let control = self.control.get();
        if self.editing.get() {
            self.refresh_slots();
            if !control.0.is_null() {
                unsafe {
                    SetTimer(Some(control), SLOTS_TIMER, 600, None);
                }
            }
        } else {
            if !control.0.is_null() {
                unsafe {
                    let _ = KillTimer(Some(control), SLOTS_TIMER);
                }
            }
            let closing: Vec<_> = self.slot_wins.borrow_mut().drain(..).collect();
            for (_, h) in closing {
                crate::slot::close(h);
            }
            self.slot_target.set(None);
        }
    }

    /// Recalcula até 3 células livres da grade do monitor principal (fora dos cartões e da galeria) e ajusta as janelas.
    pub fn refresh_slots(self: &Rc<Self>) {
        if !self.editing.get() {
            return;
        }
        let (work, dpi) = {
            let monitors = self.monitors.borrow();
            let Some(mon) = placement::primary(&monitors).map(|i| &monitors[i]) else { return };
            ((mon.work.left, mon.work.top, mon.work.right, mon.work.bottom), mon.dpi)
        };
        let cell = (pod_model::shape::CELL_DIP as f64 * dpi as f64 / 96.0).round() as i32;
        let mut occupied: Vec<pod_model::grid::Box4> = self
            .windows
            .borrow()
            .values()
            .filter(|h| unsafe { IsWindowVisible(**h).as_bool() })
            .map(|h| host::rect_of(*h))
            .map(|r| (r.left, r.top, r.right, r.bottom))
            .collect();
        if let Some(g) = self.gallery.get() {
            let r = host::rect_of(g);
            occupied.push((r.left, r.top, r.right, r.bottom));
        }
        let mut free = pod_model::grid::free_slots(work, (cell, cell), dpi, &occupied, 3);
        if let Some(t) = self.slot_target.get() {
            if !free.contains(&t) {
                if !occupied.iter().any(|o| t.0 < o.2 && o.0 < t.0 + cell && t.1 < o.3 && o.1 < t.1 + cell) {
                    free.insert(0, t);
                    free.truncate(3);
                } else {
                    self.slot_target.set(None);
                }
            }
        }
        let stale: Vec<HWND> = {
            let mut wins = self.slot_wins.borrow_mut();
            let mut gone = Vec::new();
            wins.retain(|(pos, h)| {
                let keep = free.contains(pos);
                if !keep {
                    gone.push(*h);
                }
                keep
            });
            gone
        };
        for h in stale {
            crate::slot::close(h);
        }
        for pos in free {
            if !self.slot_wins.borrow().iter().any(|(p, _)| *p == pos) {
                if let Some(h) = crate::slot::open(self, pos, (cell, cell), dpi) {
                    self.slot_wins.borrow_mut().push((pos, h));
                }
            }
        }
        if let Some(g) = self.gallery.get() {
            unsafe {
                let _ = SetWindowPos(g, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }

    /// A galeria foi destruída (Concluído, Esc ou fechar): sai do modo de edição.
    pub fn gallery_closed(self: &Rc<Self>) {
        self.gallery.set(None);
        self.set_editing(false);
    }

    /// Reavalia se há um aplicativo em foco (muda o esmaecimento dos cartões).
    pub fn update_activation(&self) {
        self.set_app_focused(host::app_in_foreground());
    }

    pub fn update_activation_for(&self, fg: HWND) {
        self.set_app_focused(host::app_in_foreground_for(fg));
    }

    /// Meta do esmaecimento: cinza com aplicativo em foco e fora da edição; inicia a transição.
    fn retarget_mute(&self) {
        let target = if self.app_focused.get() && !self.editing.get() { 1.0 } else { 0.0 };
        if target == self.mute_target.get() {
            return;
        }
        self.mute_from.set(self.mute_now.get());
        self.mute_target.set(target);
        self.mute_start.set(self.now_ms());
        let control = self.control.get();
        if control.0.is_null() {
            self.mute_now.set(target);
            self.refresh_theme();
        } else {
            unsafe {
                SetTimer(Some(control), FADE_TIMER, 16, None);
            }
        }
    }

    /// Quadro da transição (timer da janela de controle).
    pub fn on_fade_tick(&self) {
        let t = ((self.now_ms().saturating_sub(self.mute_start.get())) as f32 / FADE_MS as f32).clamp(0.0, 1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        let (a, b) = (self.mute_from.get(), self.mute_target.get());
        self.mute_now.set(a + (b - a) * eased);
        host::log(&format!("fade {:.2}", self.mute_now.get()));
        self.refresh_theme();
        if t >= 1.0 {
            let control = self.control.get();
            unsafe {
                let _ = KillTimer(Some(control), FADE_TIMER);
            }
        }
    }

    fn set_app_focused(&self, focused: bool) {
        if focused != self.app_focused.get() {
            host::log(&format!("app em foco: {focused}"));
            self.app_focused.set(focused);
            self.retarget_mute();
        }
    }

    pub fn glass(&self) -> bool {
        self.glass.get()
    }

    /// Liga/desliga o vidro (opt-in), grava a preferência e redesenha os cartões. Com "efeitos de
    /// transparência" desligados no Windows, ou em alto contraste, o cartão continua opaco.
    pub fn toggle_glass(&self) {
        let on = !self.glass.get();
        self.glass.set(on);
        if let Some(s) = self.session.borrow_mut().as_mut() {
            s.set_bool_pref(crate::storage::session::PREF_GLASS, on);
        }
        self.refresh_theme();
        self.mark_changed();
    }

    pub fn hidden_all(&self) -> bool {
        self.hidden_all.get()
    }

    /// "Ocultar/mostrar todas": transitório, não altera a preferência de cada nota.
    pub fn toggle_hidden_all(&self) {
        let hide = !self.hidden_all.get();
        self.hidden_all.set(hide);
        let all: Vec<(WidgetId, HWND)> = self.windows.borrow().iter().map(|(i, h)| (*i, *h)).collect();
        for (id, h) in all {
            let wanted = !hide && self.runtime.borrow().is_visible(id) != Some(false);
            unsafe {
                let _ = ShowWindow(h, if wanted { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
        }
    }

    /// Rótulos das notas para a bandeja: primeira linha do texto, em memória (sem metadados extras).
    pub fn note_labels(&self) -> Vec<(WidgetId, String, bool)> {
        let rt = self.runtime.borrow();
        let ids: Vec<WidgetId> = rt.ids().collect();
        ids.into_iter()
            .filter(|id| self.windows.borrow().contains_key(id))
            .filter_map(|id| {
                let (type_id, snap) = rt.snapshot(id).ok()?;
                let label =
                    if type_id == quick_notes::TYPE_ID {
                        note_label(&snap.data)
                    } else {
                        widgets::title_of(&type_id).map_or_else(|| type_id.0.to_string(), str::to_string)
                    };
                Some((id, label, rt.is_visible(id).unwrap_or(true)))
            })
            .collect()
    }

    /// Torna a nota visível (preferência durável) e a traz para frente: recuperação pela bandeja.
    pub fn reveal(&self, id: WidgetId) {
        if self.hidden_all.get() {
            self.toggle_hidden_all();
        }
        let hwnd = self.windows.borrow().get(&id).copied();
        let Some(h) = hwnd else { return };
        if self.runtime.borrow().is_visible(id) == Some(false) {
            let _ = self.runtime.borrow_mut().set_visible(id, true);
            unsafe {
                let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
            }
            self.mark_changed();
        }
        host::raise_note(h);
    }

    /// Registros duráveis de todas as instâncias vivas (menos as em exclusão) + as opacas.
    fn records(&self) -> Vec<InstanceRecord> {
        let deleting: Vec<WidgetId> = self.deleting.borrow().iter().map(|(d, _)| *d).collect();
        let windows = self.windows.borrow().clone();
        let preferred = self.preferred.borrow().clone();
        let rt = self.runtime.borrow();
        let mut out: Vec<InstanceRecord> = rt
            .ids()
            .filter(|id| !deleting.contains(id))
            .filter_map(|id| {
                let (type_id, snap) = rt.snapshot(id).ok()?;
                let hwnd = windows.get(&id)?;
                // Grava o preferido; o efetivo (onde a janela está agora) é derivado e nunca persistido.
                let placement = preferred.get(&id).cloned().unwrap_or_else(|| {
                    PreferredPlacement::from_legacy(host::rect_of(*hwnd).left, host::rect_of(*hwnd).top, 320.0, 240.0)
                });
                Some(InstanceRecord {
                    id: id.to_hex(),
                    type_id: type_id.0.to_string(),
                    state_version: snap.version,
                    state: snap.data,
                    visible: rt.is_visible(id).unwrap_or(true),
                    placement,
                    shape: self.shapes.borrow().get(&id).map(|s| s.key()),
                    settings: None,
                    extra: Default::default(),
                })
            })
            .collect();
        out.extend(self.opaque.borrow().iter().cloned());
        out
    }

    /// Coloca os dados compartilhados atuais no que a sessão vai gravar. Se o arquivo trazia dados
    /// que não conseguimos ler, eles seguem intocados até o primeiro dado válido ser salvo.
    fn sync_shared_to_session(&self) {
        if self.shared_unreadable.get() {
            return;
        }
        let value = self.runtime.borrow().shared().to_json();
        if let Some(s) = self.session.borrow_mut().as_mut() {
            s.set_shared(value);
        }
    }

    /// Carga inicial dos dados compartilhados a partir de `extra["shared"]`.
    pub fn load_shared(&self, raw: Option<serde_json::Value>) {
        let Some(raw) = raw else { return };
        match pod_model::shared::SharedData::from_json(&raw) {
            Ok(d) => self.runtime.borrow_mut().set_shared(d),
            Err(e) => {
                // Não sobrescreve o que não entendemos: segue vazio e preserva o original no disco.
                host::log(&format!("dados compartilhados ilegíveis, mantidos intactos: {e}"));
                self.shared_unreadable.set(true);
            }
        }
    }

    /// Edição feita pelo painel: altera, avisa os widgets e agenda a gravação.
    pub fn edit_shared<R>(&self, f: impl FnOnce(&mut pod_model::shared::SharedData) -> R) -> R {
        let r = self.runtime.borrow_mut().mutate_shared(f);
        self.shared_unreadable.set(false); // a partir daqui os dados válidos substituem os antigos
        self.mark_changed();
        crate::panel::refresh_all();
        self.flush();
        r
    }

    /// Envia um snapshot ao worker agora. Devolve a geração enviada (0 sem sessão).
    pub fn save_now(&self) -> u64 {
        let records = self.records();
        self.sync_shared_to_session();
        match self.session.borrow_mut().as_mut() {
            Some(s) => s.save(records),
            None => 0,
        }
    }

    /// Acks do worker: atualiza "salvo", conclui exclusões confirmadas e desfaz as que falharam.
    pub fn on_acks(&self) {
        let now = self.now_ms();
        let acks = match self.session.borrow_mut().as_mut() {
            Some(s) => s.poll_acks(now),
            None => return,
        };
        for ack in acks {
            let pending: Vec<(WidgetId, u64)> = self.deleting.borrow().clone();
            for (id, gen) in pending {
                if gen == 0 || gen > ack.generation {
                    continue;
                }
                match &ack.result {
                    Ok(()) => self.remove(id),
                    Err(e) => {
                        // A nota continua acessível; o autosave tentará de novo.
                        host::log(&format!("exclusão não confirmada ({e}); nota mantida"));
                        self.deleting.borrow_mut().retain(|(d, _)| *d != id);
                        self.mark_changed();
                    }
                }
            }
        }
        self.refresh_status();
        self.reschedule();
    }

    /// Encerramento: snapshot final com espera pelo ack, depois destrói tudo.
    pub fn shutdown(&self) -> std::result::Result<(), String> {
        let records = self.records();
        self.sync_shared_to_session();
        let now = self.now_ms();
        let result = match self.session.borrow_mut().as_mut() {
            Some(s) => s.close(records, now),
            None => Ok(()),
        };
        self.runtime.borrow_mut().shutdown();
        let all: Vec<HWND> = std::mem::take(&mut *self.windows.borrow_mut()).into_values().collect();
        for h in all {
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
        result
    }

    fn handle_commands(&self, commands: Vec<Command>) {
        for c in commands {
            match c {
                Command::StateChanged(_) => self.mark_changed(),
                Command::Chime(_) => host::chime(),
                Command::SharedChanged => {
                    self.mark_changed();
                    crate::panel::refresh_all();
                }
                Command::TimeNow(id) => {
                    let more = self.runtime.borrow_mut().deliver_time(id, sysinfo::local_time());
                    self.handle_commands(more);
                }
                Command::Media(cmd) => self.media.borrow().send(cmd),
                Command::OpenNews(id, n) => self.open_news(id, n),
                Command::PickPath(id, folder) => {
                    // Seletor do Windows (modal); só o caminho volta ao widget.
                    let owner = Some(self.control.get());
                    let picked = if folder { crate::pickers::pick_folder(owner) } else { crate::pickers::pick_program(owner) };
                    if let Some(path) = picked {
                        self.dispatch(id, Event::PathPicked { path, folder });
                    }
                }
                Command::Launch(path, folder) => {
                    if let Err(e) = crate::pickers::launch(&path, folder) {
                        host::log(&format!("atalho n\u{e3}o aberto: {e}"));
                    }
                }
                Command::ForgetToken => {
                    crate::vault::forget(crate::vault::GITHUB);
                    self.github_token_changed();
                }
                Command::OpenUri(link) => self.open_link(link),
                Command::OpenInput(id, spec) => {
                    if let Some(me) = self.me.upgrade() {
                        crate::panel::open_input(&me, id, spec);
                    }
                }
                Command::InputError(id, msg) => crate::panel::input_error(id, &msg),
                Command::OpenPanel(_, kind) => unsafe {
                    // O painel precisa do `Rc<Shell>`: a janela de controle o tem.
                    let _ = PostMessageW(Some(self.control.get()), WM_OPEN_PANEL, WPARAM(kind as usize), LPARAM(0));
                },
                Command::SetText { id, field, text } => {
                    if let Some(h) = self.windows.borrow().get(&id).copied() {
                        host::apply_set_text(h, field, &text);
                    }
                }
            }
        }
    }

    /// Invalida só as janelas cujas instâncias mudaram.
    fn flush(&self) {
        let dirty = self.runtime.borrow_mut().take_invalidated();
        for (id, inv) in dirty {
            let hwnd = self.windows.borrow().get(&id).copied();
            if let Some(h) = hwnd {
                if inv.layout {
                    host::sync_editor(h); // a view mudou de forma: o campo pode ter novo retângulo
                }
                if host::is_tile(h) {
                    host::invalidate_tile_if_changed(h); // diff de view: sem mudança, sem quadro
                } else {
                    unsafe {
                        let _ = InvalidateRect(Some(h), None, false);
                    }
                }
            }
        }
    }

    /// Um único `SetTimer` para o próximo prazo (de widget ou de autosave); sem prazo pendente,
    /// sem timer (nenhum wakeup).
    fn reschedule(&self) {
        let control = self.control.get();
        if control.0.is_null() || self.suspended.get() {
            return;
        }
        let widget = self.runtime.borrow().next_deadline();
        let save = self.session.borrow().as_ref().and_then(|s| s.due_at());
        // Fronteira de hora: só existe se algum widget visível assina; uma margem curta garante
        // que o relógio do sistema já virou o minuto/segundo quando o timer disparar.
        let time = self
            .runtime
            .borrow()
            .next_time_wait_ms(&sysinfo::local_time())
            .map(|w| self.now_ms() + w as u64 + TIME_MARGIN_MS);
        self.time_due.set(time);
        // Métricas: sem assinante visível não há prazo nem leitura. O primeiro prazo é curto (CPU
        // precisa de um intervalo desde o ponto de partida); depois segue o intervalo dos pedidos.
        let needs = self.runtime.borrow().metrics_needs();
        if needs.is_empty() {
            self.metrics_due.set(None);
            self.metrics.borrow_mut().forget_cpu();
        } else if self.metrics_due.get().is_none() {
            if needs.cpu {
                self.metrics.borrow_mut().baseline();
            }
            let first = if needs.cpu { 1_000 } else { 50 };
            self.metrics_due.set(Some(self.now_ms() + first));
        }
        // Pastas e área de transferência: mesma regra; a primeira leitura é imediata.
        let local = self.runtime.borrow().local_needs();
        if local.is_empty() {
            self.local_due.set(None);
            self.local.borrow_mut().reset();
        } else if self.local_due.get().is_none() {
            self.local_due.set(Some(self.now_ms() + 50));
        }
        // Midia: a thread de leitura so existe com assinante visivel.
        if self.runtime.borrow().media_wanted() {
            self.media.borrow_mut().start();
            if self.media_due.get().is_none() {
                self.media_due.set(Some(self.now_ms() + 100));
            }
        } else if self.media.borrow().running() {
            self.media_due.set(None);
            self.media.borrow_mut().stop();
        }
        if self.runtime.borrow().notifications_wanted() {
            self.notif.borrow_mut().start();
            if self.notif_due.get().is_none() {
                self.notif_due.set(Some(self.now_ms() + 200));
            }
        } else if self.notif.borrow().running() {
            self.notif_due.set(None);
            self.notif.borrow_mut().stop();
        }
        if self.runtime.borrow().usage_wanted() {
            self.usage.borrow_mut().start();
            if self.usage_due.get().is_none() {
                self.usage_due.set(Some(self.now_ms() + 200));
            }
        } else if self.usage.borrow().running() {
            self.usage_due.set(None);
            self.usage.borrow_mut().stop();
        }
        // Clima: a thread de rede so existe com o opt-in ligado E algum widget Weather visivel.
        let wanted_cities = self.runtime.borrow().weather_cities();
        if !wanted_cities.is_empty() {
            if self.weather_due.get().is_none() {
                self.weather_due.set(Some(self.now_ms() + 100));
            }
            if self.internet.get() {
                self.weather.borrow_mut().start();
                if *self.weather_cities.borrow() != wanted_cities {
                    self.weather.borrow().set_cities(wanted_cities.clone());
                    *self.weather_cities.borrow_mut() = wanted_cities;
                }
            }
        } else if self.weather.borrow().running() {
            self.weather_due.set(None);
            self.weather.borrow_mut().stop();
            self.weather_cities.borrow_mut().clear();
        }
        // Noticias: mesma regra do clima (opt-in ligado E algum widget News visivel).
        let wanted_feeds = self.runtime.borrow().news_feeds();
        if !wanted_feeds.is_empty() {
            if self.news_due.get().is_none() {
                self.news_due.set(Some(self.now_ms() + 100));
            }
            if self.internet.get() {
                self.news.borrow_mut().start();
                if *self.news_feeds.borrow() != wanted_feeds {
                    self.news.borrow().set_feeds(wanted_feeds.clone());
                    *self.news_feeds.borrow_mut() = wanted_feeds;
                }
            }
        } else if self.news.borrow().running() {
            self.news_due.set(None);
            self.news.borrow_mut().stop();
            self.news_feeds.borrow_mut().clear();
        }
        // Cotações: opt-in ligado E algum widget Bolsa visível.
        let wanted_quotes = self.runtime.borrow().quote_requests();
        if !wanted_quotes.is_empty() {
            if self.quotes_due.get().is_none() {
                self.quotes_due.set(Some(self.now_ms() + 100));
            }
            if self.internet.get() {
                self.quotes.borrow_mut().start();
                if *self.quotes_reqs.borrow() != wanted_quotes {
                    self.quotes.borrow().set_requests(wanted_quotes.clone());
                    *self.quotes_reqs.borrow_mut() = wanted_quotes;
                }
            }
        } else if self.quotes.borrow().running() {
            self.quotes_due.set(None);
            self.quotes.borrow_mut().stop();
            self.quotes_reqs.borrow_mut().clear();
        }
        // GitHub: opt-in ligado E algum widget GitHub visivel.
        if self.runtime.borrow().wants_github() {
            if self.github_due.get().is_none() {
                self.github_due.set(Some(self.now_ms() + 100));
            }
            if self.internet.get() {
                self.github.borrow_mut().start();
            }
        } else if self.github.borrow().running() {
            self.github_due.set(None);
            self.github.borrow_mut().stop();
        }
        let next = [widget, save, time, self.metrics_due.get(), self.local_due.get(), self.media_due.get(), self.notif_due.get(), self.usage_due.get(), self.weather_due.get(), self.news_due.get(), self.github_due.get(), self.quotes_due.get()]
            .into_iter()
            .flatten()
            .min();
        unsafe {
            match next {
                Some(deadline) => {
                    let delay = deadline.saturating_sub(self.now_ms()).clamp(1, u32::MAX as u64) as u32;
                    SetTimer(Some(control), SCHEDULER_TIMER, delay, None);
                }
                None => {
                    let _ = KillTimer(Some(control), SCHEDULER_TIMER);
                }
            }
        }
    }
}

/// Primeira linha não vazia, cortada em 40 caracteres. Nota vazia vira um rótulo fixo.
fn note_label(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.is_empty() {
        return "Nota vazia".to_string();
    }
    let mut out: String = line.chars().take(40).collect();
    if line.chars().count() > 40 {
        out.push('\u{2026}');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::note_label;

    #[test]
    fn label_is_the_first_non_empty_line() {
        assert_eq!(note_label("\n  \nOl\u{e1}, mundo\nsegunda"), "Ol\u{e1}, mundo");
        assert_eq!(note_label(""), "Nota vazia");
        assert_eq!(note_label(" \t\n "), "Nota vazia");
    }

    #[test]
    fn label_truncates_by_characters_not_bytes() {
        let long = "\u{e9}".repeat(60);
        let l = note_label(&long);
        assert_eq!(l.chars().count(), 41, "40 caracteres + reticências");
        assert!(l.ends_with('\u{2026}'));
        assert_eq!(note_label(&"a".repeat(40)), "a".repeat(40), "exatamente 40 não ganha reticências");
    }
}
