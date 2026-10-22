//! Provedor `GitHub`: lê o token do cofre do Windows e consulta a API (`api.github.com`) pelo corretor
//! HTTPS, numa thread própria. **Só roda com "Widgets de internet" ligado** e com algum widget GitHub
//! visível. Atualiza a cada 2 minutos (ou na hora, depois de salvar/trocar o token). O token fica só
//! nesta thread, em memória, durante o pedido.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pod_model::github::*;

use crate::netbroker::{https_get_github, NetError};

static REQUESTS: AtomicU64 = AtomicU64::new(0);

/// Pedidos feitos à API desde o início (prova de que sem opt-in nada sai).
pub fn requests() -> u64 {
    REQUESTS.load(Ordering::Relaxed)
}

const REFRESH: Duration = Duration::from_secs(120);
const TICK: Duration = Duration::from_secs(20);

fn get(q: Query, token: &str) -> Result<String, NetError> {
    REQUESTS.fetch_add(1, Ordering::Relaxed);
    let bytes = https_get_github(&q.url(), token)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn bad() -> GithubState {
    GithubState::Failed("Resposta inesperada".into())
}

/// Busca o resumo completo com o token dado.
fn fetch(token: &str) -> GithubState {
    let rejected = |e: &NetError| matches!(e, NetError::Status(401 | 403));
    let user = match get(Query::User, token) {
        Ok(b) => b,
        Err(e) if rejected(&e) => return GithubState::Rejected,
        Err(e) => return GithubState::Failed(e.short()),
    };
    let Some(login) = parse_login(&user) else { return bad() };
    let (unread, reviews, mine) = (get(Query::Notifications, token), get(Query::ReviewRequests, token), get(Query::MyPulls, token));
    let (Ok(unread), Ok(reviews), Ok(mine)) = (unread, reviews, mine) else {
        return GithubState::Failed("Sem conex\u{e3}o".into());
    };
    let (Some(unread), Some((review_requests, pulls)), Some((my_pulls, _))) =
        (parse_notification_count(&unread), parse_search(&reviews), parse_search(&mine))
    else {
        return bad();
    };
    GithubState::Ready(Summary { login, unread, unread_capped: unread >= 30, review_requests, my_pulls, reviews: pulls })
}

fn worker(stop: Arc<AtomicBool>, tx: Sender<GithubState>, kick: Receiver<()>) {
    let mut last: Option<Instant> = None;
    while !stop.load(Ordering::Relaxed) {
        let kicked = match kick.recv_timeout(TICK) {
            Ok(()) => true,
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if !kicked && last.is_some_and(|t| t.elapsed() < REFRESH) {
            continue;
        }
        let state = match crate::vault::load(crate::vault::GITHUB) {
            None => GithubState::NoToken,
            Some(t) => fetch(&t),
        };
        // Falha: tenta de novo em 1 minuto; sem token: olha o cofre a cada tick.
        last = Some(match state {
            GithubState::Failed(_) => Instant::now() - REFRESH + Duration::from_secs(60),
            GithubState::NoToken => Instant::now() - REFRESH,
            _ => Instant::now(),
        });
        if tx.send(state).is_err() {
            return;
        }
    }
}

pub struct Provider {
    handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    rx: Option<Receiver<GithubState>>,
    kick: Option<Sender<()>>,
}

impl Default for Provider {
    fn default() -> Self {
        Self { handle: None, stop: Arc::new(AtomicBool::new(false)), rx: None, kick: None }
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
        let (kick_tx, kick_rx) = channel();
        let s = stop.clone();
        self.handle = std::thread::Builder::new().name("pod-github".into()).spawn(move || worker(s, tx, kick_rx)).ok();
        if self.handle.is_some() {
            self.stop = stop;
            self.rx = Some(rx);
            let _ = kick_tx.send(());
            self.kick = Some(kick_tx);
        }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.handle = None;
        self.rx = None;
        self.kick = None;
    }

    /// Atualiza já (token novo ou removido).
    pub fn refresh_now(&self) {
        if let Some(k) = &self.kick {
            let _ = k.send(());
        }
    }

    pub fn take_latest(&mut self) -> Option<GithubState> {
        let rx = self.rx.as_ref()?;
        let mut last = None;
        while let Ok(s) = rx.try_recv() {
            last = Some(s);
        }
        last
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
    fn a_stopped_provider_makes_no_request_and_reads_nothing() {
        let before = requests();
        let mut p = Provider::default();
        p.refresh_now();
        assert!(p.take_latest().is_none());
        assert_eq!(requests(), before);
    }
}
