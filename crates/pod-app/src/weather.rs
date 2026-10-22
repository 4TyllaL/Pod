//! Provedor `Weather`: busca previsão do tempo (Open-Meteo) pelo corretor HTTPS, numa thread própria,
//! para o conjunto de cidades dos widgets visíveis. **Só roda com "Widgets de internet" ligado** (o shell
//! não inicia nem mantém a thread sem o opt-in) e só enquanto algum widget Weather visível assina.
//!
//! A geocodificação (nome → coordenadas) fica em cache na vida da thread; a previsão é atualizada a
//! cada 15 minutos ou quando a lista de cidades muda.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pod_model::net::{parse_forecast, parse_geocode, Endpoint, Place, WeatherState};

use crate::netbroker::{https_get, NetError};

static REQUESTS: AtomicU64 = AtomicU64::new(0);

/// Pedidos HTTP feitos desde o início (prova de que sem opt-in ou sem assinante nada sai).
pub fn requests() -> u64 {
    REQUESTS.load(Ordering::Relaxed)
}

const REFRESH: Duration = Duration::from_secs(15 * 60);
const TICK: Duration = Duration::from_secs(30);

fn get(ep: &Endpoint) -> Result<String, NetError> {
    let url = ep.url().ok_or(NetError::Refused)?;
    REQUESTS.fetch_add(1, Ordering::Relaxed);
    let body = https_get(&url)?;
    String::from_utf8(body).map_err(|_| NetError::Network("resposta n\u{e3}o \u{e9} texto".into()))
}

/// Resultado da geocodificação: lugar, "não achei" (`Ok(None)`) ou falha de rede.
fn geocode(name: &str, cache: &mut HashMap<String, Option<Place>>) -> Result<Option<Place>, NetError> {
    if let Some(p) = cache.get(name) {
        return Ok(p.clone());
    }
    let body = get(&Endpoint::Geocode { name: name.to_string() })?;
    let place = parse_geocode(&body);
    cache.insert(name.to_string(), place.clone());
    Ok(place)
}

fn fetch(city: &str, cache: &mut HashMap<String, Option<Place>>) -> WeatherState {
    let place = match geocode(city, cache) {
        Ok(Some(p)) => p,
        Ok(None) => return WeatherState::NotFound,
        Err(e) => return WeatherState::Failed(e.short()),
    };
    match get(&Endpoint::Forecast { lat: place.lat, lon: place.lon }) {
        Ok(body) => match parse_forecast(&body) {
            Some(report) => {
                let label = if place.country.is_empty() { place.name } else { format!("{}, {}", place.name, place.country) };
                WeatherState::Ready { place: label, report }
            }
            None => WeatherState::Failed("Resposta inv\u{e1}lida".into()),
        },
        Err(e) => WeatherState::Failed(e.short()),
    }
}

fn worker(stop: Arc<AtomicBool>, tx: Sender<(String, WeatherState)>, cities_rx: Receiver<Vec<String>>) {
    let mut cities: Vec<String> = Vec::new();
    let mut last: HashMap<String, Instant> = HashMap::new();
    let mut geo: HashMap<String, Option<Place>> = HashMap::new();
    while !stop.load(Ordering::Relaxed) {
        // Nova lista de cidades (a mais recente vence) ou, sem novidade, o relógio da atualização.
        match cities_rx.recv_timeout(TICK) {
            Ok(mut list) => {
                while let Ok(newer) = cities_rx.try_recv() {
                    list = newer;
                }
                last.retain(|c, _| list.contains(c));
                cities = list;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        for city in cities.clone() {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let due = last.get(&city).is_none_or(|t| t.elapsed() >= REFRESH);
            if !due {
                continue;
            }
            last.insert(city.clone(), Instant::now());
            let state = fetch(&city, &mut geo);
            // Falha: tenta de novo em 1 minuto, não em 15.
            if matches!(state, WeatherState::Failed(_)) {
                last.insert(city.clone(), Instant::now() - REFRESH + Duration::from_secs(60));
            }
            if tx.send((city, state)).is_err() {
                return;
            }
        }
    }
}

/// Dono da thread (vive na thread de UI; só conversa por canais).
pub struct Provider {
    handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    rx: Option<Receiver<(String, WeatherState)>>,
    cities_tx: Option<Sender<Vec<String>>>,
}

impl Default for Provider {
    fn default() -> Self {
        Self { handle: None, stop: Arc::new(AtomicBool::new(false)), rx: None, cities_tx: None }
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
        let (cities_tx, cities_rx) = channel();
        let s = stop.clone();
        self.handle = std::thread::Builder::new().name("pod-weather".into()).spawn(move || worker(s, tx, cities_rx)).ok();
        if self.handle.is_some() {
            self.stop = stop;
            self.rx = Some(rx);
            self.cities_tx = Some(cities_tx);
        }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.handle = None;
        self.rx = None;
        self.cities_tx = None;
    }

    /// Informa as cidades que os widgets visíveis querem (substitui a lista anterior).
    pub fn set_cities(&self, cities: Vec<String>) {
        if let Some(tx) = &self.cities_tx {
            let _ = tx.send(cities);
        }
    }

    /// Resultados chegados desde a última chamada.
    pub fn take_results(&mut self) -> Vec<(String, WeatherState)> {
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

    #[test]
    fn a_stopped_provider_does_nothing_and_makes_no_request() {
        let before = requests();
        let mut p = Provider::default();
        assert!(!p.running());
        p.set_cities(vec!["Recife".into()]); // sem thread: ignorado
        assert!(p.take_results().is_empty());
        assert_eq!(requests(), before, "sem start, nenhum pedido");
    }

    #[test]
    fn an_empty_city_list_makes_no_request() {
        let before = requests();
        let mut p = Provider::default();
        p.start();
        p.set_cities(Vec::new());
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(requests(), before, "thread rodando sem cidades nao sai para a rede");
        p.stop();
    }

    /// Internet de verdade: `cargo test -p pod-app weather -- --ignored --nocapture`.
    #[test]
    #[ignore = "usa a internet"]
    fn real_weather_for_a_city_and_a_missing_one() {
        let mut p = Provider::default();
        p.start();
        p.set_cities(vec!["Recife".into(), "Xyzzyqwkj".into()]);
        let mut got: HashMap<String, WeatherState> = HashMap::new();
        for _ in 0..200 {
            for (c, s) in p.take_results() {
                got.insert(c, s);
            }
            if got.len() == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        p.stop();
        match got.get("Recife") {
            Some(WeatherState::Ready { place, report }) => eprintln!("Recife: {place}, {:.1} graus, codigo {}", report.temp_c, report.code),
            other => panic!("Recife: {other:?}"),
        }
        assert_eq!(got.get("Xyzzyqwkj"), Some(&WeatherState::NotFound));
    }
}
