//! Provedor `Quotes`: busca cotações (AwesomeAPI, Brapi ou Yahoo) pelo corretor HTTPS, numa thread
//! própria, para o conjunto de pedidos dos widgets Bolsa visíveis. **Só roda com "Widgets de internet"
//! ligado** e com algum widget visível. Atualiza a cada 90 s (ou quando a lista de pedidos muda); falha
//! tenta de novo em 1 minuto.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pod_model::quotes::{parse_awesome, parse_brapi, parse_yahoo, Quote, QuoteRequest, QuoteState, Source};

use crate::netbroker::https_get;

static REQUESTS: AtomicU64 = AtomicU64::new(0);

/// Pedidos feitos desde o início (prova de que sem opt-in nada sai).
pub fn requests() -> u64 {
    REQUESTS.load(Ordering::Relaxed)
}

const REFRESH: Duration = Duration::from_secs(90);
const TICK: Duration = Duration::from_secs(15);

fn get_text(url: &str) -> Option<String> {
    REQUESTS.fetch_add(1, Ordering::Relaxed);
    https_get(url).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn fetch(req: &QuoteRequest) -> QuoteState {
    let urls = req.urls();
    if urls.is_empty() {
        return QuoteState::Failed("Sem ativos".into());
    }
    let quotes: Vec<Quote> = match req.source {
        Source::Awesome => get_text(&urls[0]).map(|b| parse_awesome(&b, &req.symbols)).unwrap_or_default(),
        Source::Brapi => get_text(&urls[0]).map(|b| parse_brapi(&b, &req.symbols)).unwrap_or_default(),
        Source::Yahoo => req
            .symbols
            .iter()
            .zip(&urls)
            .filter_map(|(sym, url)| get_text(url).and_then(|b| parse_yahoo(&b, sym)))
            .collect(),
    };
    if quotes.is_empty() {
        QuoteState::Failed("Sem cota\u{e7}\u{f5}es".into())
    } else {
        QuoteState::Ready { source: req.source, quotes }
    }
}

fn worker(stop: Arc<AtomicBool>, tx: Sender<(QuoteRequest, QuoteState)>, reqs_rx: Receiver<Vec<QuoteRequest>>) {
    let mut reqs: Vec<QuoteRequest> = Vec::new();
    let mut last: HashMap<QuoteRequest, Instant> = HashMap::new();
    while !stop.load(Ordering::Relaxed) {
        match reqs_rx.recv_timeout(TICK) {
            Ok(mut list) => {
                while let Ok(newer) = reqs_rx.try_recv() {
                    list = newer;
                }
                last.retain(|r, _| list.contains(r));
                reqs = list;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        for r in reqs.clone() {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if !last.get(&r).is_none_or(|t| t.elapsed() >= REFRESH) {
                continue;
            }
            last.insert(r.clone(), Instant::now());
            let state = fetch(&r);
            if matches!(state, QuoteState::Failed(_)) {
                last.insert(r.clone(), Instant::now() - REFRESH + Duration::from_secs(60));
            }
            if tx.send((r, state)).is_err() {
                return;
            }
        }
    }
}

pub struct Provider {
    handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    rx: Option<Receiver<(QuoteRequest, QuoteState)>>,
    reqs_tx: Option<Sender<Vec<QuoteRequest>>>,
}

impl Default for Provider {
    fn default() -> Self {
        Self { handle: None, stop: Arc::new(AtomicBool::new(false)), rx: None, reqs_tx: None }
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
        let (reqs_tx, reqs_rx) = channel();
        let s = stop.clone();
        self.handle = std::thread::Builder::new().name("pod-quotes".into()).spawn(move || worker(s, tx, reqs_rx)).ok();
        if self.handle.is_some() {
            self.stop = stop;
            self.rx = Some(rx);
            self.reqs_tx = Some(reqs_tx);
        }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.handle = None;
        self.rx = None;
        self.reqs_tx = None;
    }

    pub fn set_requests(&self, reqs: Vec<QuoteRequest>) {
        if let Some(tx) = &self.reqs_tx {
            let _ = tx.send(reqs);
        }
    }

    pub fn take_results(&mut self) -> Vec<(QuoteRequest, QuoteState)> {
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

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn a_stopped_provider_and_an_empty_list_make_no_request() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = requests();
        let mut p = Provider::default();
        p.set_requests(vec![QuoteRequest::clean(Source::Awesome, &[])]); // sem thread: ignorado
        assert!(p.take_results().is_empty());
        p.start();
        p.set_requests(Vec::new());
        std::thread::sleep(Duration::from_millis(600));
        p.stop();
        assert_eq!(requests(), before);
    }

    /// Internet de verdade, as três fontes: `cargo test -p pod-app quotes -- --ignored --nocapture`.
    #[test]
    #[ignore = "usa a internet"]
    fn every_source_works_for_real() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for s in Source::ALL {
            let r = QuoteRequest::clean(s, &[]);
            match fetch(&r) {
                QuoteState::Ready { quotes, .. } => {
                    assert!(!quotes.is_empty(), "{s:?}");
                    for q in &quotes {
                        eprintln!("{:?}: {} {} {}", s, q.label, pod_model::quotes::format_price(q.price), pod_model::quotes::format_pct(q.change_pct));
                    }
                }
                other => panic!("{s:?}: {other:?}"),
            }
        }
    }
}
