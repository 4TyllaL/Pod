//! Provedor `Notifications`: lê as notificações (toasts) do Centro de Notificações do Windows com
//! `UserNotificationListener`, numa thread própria, e só enquanto algum widget visível assina.
//!
//! O Windows exige a permissão "Acesso a notificações" e, para esta API, normalmente identidade de
//! pacote. Sem ela as chamadas falham e o estado vira `Unavailable` (mostrado ao usuário, nunca
//! escondido). Pergunta de permissão (`RequestAccessAsync`) só é feita se o Windows disser
//! "não especificada". O corpo das mensagens nunca é lido além do título.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows::UI::Notifications::Management::{UserNotificationListener, UserNotificationListenerAccessStatus as Access};
use windows::UI::Notifications::{KnownNotificationBindings, NotificationKinds};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use pod_model::notifications::{NotifAccess, NotifSnapshot};

static READS: AtomicU64 = AtomicU64::new(0);

/// Leituras da lista feitas desde o início (prova de que sem assinante nada é lido).
pub fn reads() -> u64 {
    READS.load(Ordering::Relaxed)
}

const POLL: Duration = Duration::from_millis(3000);
/// Segundos entre 1601-01-01 (época do `DateTime` do WinRT) e 1970-01-01.
const EPOCH_GAP_SECS: u64 = 11_644_473_600;

fn now_secs_since_1601() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) + EPOCH_GAP_SECS
}

/// Lê a lista de toasts. `Err` = o Windows recusou a chamada (sem identidade de pacote, por exemplo).
fn read_list(listener: &UserNotificationListener) -> windows::core::Result<Vec<(String, String, u64)>> {
    let list = listener.GetNotificationsAsync(NotificationKinds::Toast)?.join()?;
    let now = now_secs_since_1601();
    let generic = KnownNotificationBindings::ToastGeneric()?;
    let mut out = Vec::new();
    for n in list {
        let app = n
            .AppInfo()
            .and_then(|a| a.DisplayInfo())
            .and_then(|d| d.DisplayName())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let created = n.CreationTime().map_or(0, |t| (t.UniversalTime.max(0) / 10_000_000) as u64);
        let title = n
            .Notification()
            .and_then(|x| x.Visual())
            .and_then(|v| v.GetBinding(&generic))
            .and_then(|b| b.GetTextElements())
            .ok()
            .and_then(|els| els.into_iter().next())
            .and_then(|t| t.Text().ok())
            .map(|s| s.to_string())
            .unwrap_or_default();
        out.push((app, title, now.saturating_sub(created)));
    }
    Ok(out)
}

fn snapshot(listener: &Option<UserNotificationListener>) -> NotifSnapshot {
    let Some(l) = listener else { return NotifSnapshot::without_access(NotifAccess::Unavailable) };
    let status = match l.GetAccessStatus() {
        Ok(s) => s,
        Err(_) => return NotifSnapshot::without_access(NotifAccess::Unavailable),
    };
    let status = if status == Access::Unspecified {
        // Única chamada que pode mostrar a pergunta de permissão do Windows.
        l.RequestAccessAsync().and_then(|a| a.join()).unwrap_or(Access::Unspecified)
    } else {
        status
    };
    match status {
        Access::Allowed => {
            READS.fetch_add(1, Ordering::Relaxed);
            match read_list(l) {
                Ok(raw) => NotifSnapshot::from_raw(raw),
                Err(_) => NotifSnapshot::without_access(NotifAccess::Unavailable),
            }
        }
        Access::Denied => NotifSnapshot::without_access(NotifAccess::Denied),
        _ => NotifSnapshot::without_access(NotifAccess::Unavailable),
    }
}

fn worker(stop: Arc<AtomicBool>, tx: Sender<NotifSnapshot>, wake: Receiver<()>) {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let listener = UserNotificationListener::Current().ok();
    while !stop.load(Ordering::Relaxed) {
        let snap = snapshot(&listener);
        crate::host::log(&format!("notificacoes: acesso {:?}, {} na lista", snap.access, snap.total));
        if tx.send(snap).is_err() {
            return;
        }
        match wake.recv_timeout(POLL) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Dono da thread de leitura (vive na thread de UI; só conversa por canais).
pub struct Provider {
    handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    rx: Option<Receiver<NotifSnapshot>>,
    _wake: Option<Sender<()>>,
}

impl Default for Provider {
    fn default() -> Self {
        Self { handle: None, stop: Arc::new(AtomicBool::new(false)), rx: None, _wake: None }
    }
}

impl Provider {
    pub fn running(&self) -> bool {
        self.handle.is_some()
    }

    pub fn start(&mut self) {
        if self.running() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let (wake_tx, wake_rx) = channel();
        let s = stop.clone();
        self.handle = std::thread::Builder::new().name("pod-notif".into()).spawn(move || worker(s, tx, wake_rx)).ok();
        if self.handle.is_some() {
            self.stop = stop;
            self.rx = Some(rx);
            self._wake = Some(wake_tx);
        }
    }

    /// Pede o fim da thread e a solta (ela termina no próximo ciclo; a UI não espera).
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.handle = None;
        self.rx = None;
        self._wake = None;
    }

    pub fn take_latest(&mut self) -> Option<NotifSnapshot> {
        let rx = self.rx.as_ref()?;
        let mut latest = None;
        while let Ok(s) = rx.try_recv() {
            latest = Some(s);
        }
        latest
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_answers_with_some_access_state_and_stops_reading() {
        let mut p = Provider::default();
        assert_eq!(p.take_latest(), None);
        p.start();
        let mut got = None;
        for _ in 0..80 {
            if let Some(s) = p.take_latest() {
                got = Some(s);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let s = got.expect("a thread entregou um estado em 8 s");
        eprintln!("estado das notificacoes neste ambiente: {:?} ({} itens)", s.access, s.total);
        assert!(matches!(s.access, NotifAccess::Allowed | NotifAccess::Denied | NotifAccess::Unavailable | NotifAccess::Unknown));
        p.stop();
        assert!(!p.running());
        std::thread::sleep(Duration::from_millis(3500));
        let after = reads();
        std::thread::sleep(Duration::from_millis(3500));
        assert_eq!(reads(), after, "parou de ler depois do stop");
    }

    #[test]
    fn epoch_gap_matches_the_known_constant() {
        // 1970-01-01 em segundos desde 1601 (valor clássico do FILETIME).
        assert_eq!(EPOCH_GAP_SECS, 11_644_473_600);
        assert!(now_secs_since_1601() > EPOCH_GAP_SECS);
    }
}
