//! Provedor `News`: busca feeds RSS/Atom do catálogo (`pod_model::news::FEEDS`) pelo corretor HTTPS,
//! numa thread própria, para o conjunto de feeds dos widgets visíveis. **Só roda com "Widgets de
//! internet" ligado** (o shell não inicia a thread sem o opt-in) e só com algum widget News visível.
//! Atualiza a cada 15 minutos ou quando a lista de feeds muda; falha tenta de novo em 1 minuto.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pod_model::net::Endpoint;
use pod_model::news::{decode_text, feed_by_id, parse_feed, NewsState};

use crate::netbroker::https_get;

static REQUESTS: AtomicU64 = AtomicU64::new(0);

/// Pedidos de feed feitos desde o início (prova de que sem opt-in nada sai).
pub fn requests() -> u64 {
    REQUESTS.load(Ordering::Relaxed)
}

const REFRESH: Duration = Duration::from_secs(15 * 60);
const TICK: Duration = Duration::from_secs(30);

fn fetch(feed_id: &str) -> NewsState {
    let Some(feed) = feed_by_id(feed_id) else { return NewsState::Failed("Fonte desconhecida".into()) };
    let Some(url) = (Endpoint::Feed { id: feed_id.to_string() }).url() else {
        return NewsState::Failed("Fonte desconhecida".into());
    };
    REQUESTS.fetch_add(1, Ordering::Relaxed);
    match https_get(&url) {
        Ok(bytes) => {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
            let items = parse_feed(&decode_text(&bytes), now);
            if items.is_empty() {
                NewsState::Failed("Feed sem manchetes".into())
            } else {
                NewsState::Ready { source: feed.name.to_string(), items }
            }
        }
        Err(e) => NewsState::Failed(e.short()),
    }
}

fn worker(stop: Arc<AtomicBool>, tx: Sender<(String, NewsState)>, feeds_rx: Receiver<Vec<String>>) {
    let mut feeds: Vec<String> = Vec::new();
    let mut last: HashMap<String, Instant> = HashMap::new();
    while !stop.load(Ordering::Relaxed) {
        match feeds_rx.recv_timeout(TICK) {
            Ok(mut list) => {
                while let Ok(newer) = feeds_rx.try_recv() {
                    list = newer;
                }
                last.retain(|f, _| list.contains(f));
                feeds = list;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        for f in feeds.clone() {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if !last.get(&f).is_none_or(|t| t.elapsed() >= REFRESH) {
                continue;
            }
            last.insert(f.clone(), Instant::now());
            let state = fetch(&f);
            if matches!(state, NewsState::Failed(_)) {
                last.insert(f.clone(), Instant::now() - REFRESH + Duration::from_secs(60));
            }
            if tx.send((f, state)).is_err() {
                return;
            }
        }
    }
}

/// Dono da thread (vive na thread de UI; só conversa por canais).
pub struct Provider {
    handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    rx: Option<Receiver<(String, NewsState)>>,
    feeds_tx: Option<Sender<Vec<String>>>,
}

impl Default for Provider {
    fn default() -> Self {
        Self { handle: None, stop: Arc::new(AtomicBool::new(false)), rx: None, feeds_tx: None }
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
        let (feeds_tx, feeds_rx) = channel();
        let s = stop.clone();
        self.handle = std::thread::Builder::new().name("pod-news".into()).spawn(move || worker(s, tx, feeds_rx)).ok();
        if self.handle.is_some() {
            self.stop = stop;
            self.rx = Some(rx);
            self.feeds_tx = Some(feeds_tx);
        }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.handle = None;
        self.rx = None;
        self.feeds_tx = None;
    }

    pub fn set_feeds(&self, feeds: Vec<String>) {
        if let Some(tx) = &self.feeds_tx {
            let _ = tx.send(feeds);
        }
    }

    pub fn take_results(&mut self) -> Vec<(String, NewsState)> {
        let Some(rx) = self.rx.as_ref() else { return Vec::new() };
        let mut out = Vec::new();
        while let Ok(r) = rx.try_recv() {
            out.push(r);
        }
        out
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

    /// Os testes compartilham o contador global de pedidos: um de cada vez.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn a_stopped_provider_and_an_empty_feed_list_make_no_request() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = requests();
        let mut p = Provider::default();
        p.set_feeds(vec!["bbc-pt".into()]); // sem thread: ignorado
        assert!(p.take_results().is_empty());
        p.start();
        p.set_feeds(Vec::new());
        std::thread::sleep(Duration::from_millis(600));
        p.stop();
        assert_eq!(requests(), before);
    }

    #[test]
    fn an_unknown_feed_id_fails_without_touching_the_network() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = requests();
        assert_eq!(fetch("nao-existe"), NewsState::Failed("Fonte desconhecida".into()));
        assert_eq!(requests(), before);
    }

    /// Internet de verdade, todos os feeds do catálogo: `cargo test -p pod-app news -- --ignored --nocapture`.
    #[test]
    #[ignore = "usa a internet"]
    fn every_catalog_feed_works_for_real() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for f in pod_model::news::FEEDS {
            match fetch(f.id) {
                NewsState::Ready { source, items } => {
                    assert!(!items.is_empty() && items.iter().all(|h| h.link.starts_with("http")), "{}", f.id);
                    eprintln!("{source}: {} manchetes, primeira com {} caracteres, idade {:?}", items.len(), items[0].title.chars().count(), items[0].age_secs);
                }
                other => panic!("{}: {other:?}", f.id),
            }
        }
    }
}
