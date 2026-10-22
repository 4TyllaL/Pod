//! Provedor `ClaudeUsage`: lê os transcritos do Claude Code em `~/.claude/projects/**/*.jsonl` de forma
//! incremental e entrega o uso dos últimos 7 dias. Leitura **somente local e somente leitura**, de uma
//! única pasta permitida (a lista de permissão do ADR-0001: `ReadFiles`); nada de rede.
//!
//! A leitura roda numa thread própria, só enquanto algum widget visível assina. O primeiro ciclo lê os
//! arquivos dos últimos 8 dias por inteiro (podem ser centenas de MB); os seguintes só leem o que foi
//! acrescentado desde o último deslocamento conhecido de cada arquivo.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};

use pod_model::usage::{local_day, parse_line, Aggregator, UsageSnapshot};

static CYCLES: AtomicU64 = AtomicU64::new(0);

/// Ciclos de leitura feitos desde o início (prova de que sem assinante nada é lido).
pub fn cycles() -> u64 {
    CYCLES.load(Ordering::Relaxed)
}

const POLL: Duration = Duration::from_secs(60);
/// Só arquivos tocados nos últimos 8 dias entram (a janela do cartão é de 7).
const MAX_AGE: Duration = Duration::from_secs(8 * 86_400);

/// `~/.claude/projects` (a única pasta que este provedor lê).
pub fn projects_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".claude").join("projects"))
}

/// Minutos a leste de UTC do fuso local agora (Brasil = -180).
fn local_offset_min() -> i32 {
    let mut tz = TIME_ZONE_INFORMATION::default();
    let id = unsafe { GetTimeZoneInformation(&mut tz) };
    let daylight = id == 2; // TIME_ZONE_ID_DAYLIGHT
    -(tz.Bias + if daylight { tz.DaylightBias } else { tz.StandardBias })
}

fn collect(dir: &Path, now: SystemTime, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            collect(&p, now, out);
        } else if p.extension().is_some_and(|x| x == "jsonl") {
            let fresh = meta.modified().ok().and_then(|m| now.duration_since(m).ok()).is_none_or(|age| age <= MAX_AGE);
            if fresh {
                out.push(p);
            }
        }
    }
}

/// Estado entre ciclos: onde cada arquivo parou e o que já foi somado.
#[derive(Default)]
pub struct Scanner {
    offsets: HashMap<PathBuf, u64>,
    agg: Aggregator,
    /// Bytes lidos no último ciclo (para os testes provarem que a leitura é incremental).
    pub last_bytes: u64,
}

impl Scanner {
    /// Um ciclo: lê o que há de novo em `root` e devolve o resumo do dia `now` no fuso `offset_min`.
    pub fn scan(&mut self, root: &Path, now: SystemTime, offset_min: i32) -> UsageSnapshot {
        let mut files = Vec::new();
        collect(root, now, &mut files);
        self.last_bytes = 0;
        for f in &files {
            self.read_new(f, offset_min);
        }
        // Esquece arquivos que sumiram da lista (apagados ou velhos demais).
        self.offsets.retain(|p, _| files.contains(p));
        let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        self.agg.snapshot(local_day(secs, offset_min), files.len())
    }

    fn read_new(&mut self, path: &Path, offset_min: i32) {
        let Ok(file) = std::fs::File::open(path) else { return };
        let len = file.metadata().map_or(0, |m| m.len());
        let start = match self.offsets.get(path) {
            Some(&o) if o <= len => o,
            _ => 0, // arquivo novo ou truncado
        };
        if start == len {
            self.offsets.insert(path.to_path_buf(), start);
            return;
        }
        let mut reader = BufReader::with_capacity(1 << 20, file);
        if reader.seek(SeekFrom::Start(start)).is_err() {
            return;
        }
        let mut consumed = start;
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if buf.last() != Some(&b'\n') {
                        break; // linha ainda incompleta (o Claude Code está escrevendo): fica para o próximo ciclo
                    }
                    consumed += n as u64;
                    self.last_bytes += n as u64;
                    if let Ok(line) = std::str::from_utf8(&buf) {
                        if let Some(e) = parse_line(line.trim_end(), offset_min) {
                            self.agg.add(e);
                        }
                    }
                }
            }
        }
        self.offsets.insert(path.to_path_buf(), consumed);
    }
}

fn worker(stop: Arc<AtomicBool>, tx: Sender<UsageSnapshot>, wake: Receiver<()>) {
    let mut scanner = Scanner::default();
    let mut last: Option<UsageSnapshot> = None;
    while !stop.load(Ordering::Relaxed) {
        CYCLES.fetch_add(1, Ordering::Relaxed);
        let started = std::time::Instant::now();
        let snap = match projects_dir() {
            Some(root) if root.is_dir() => scanner.scan(&root, SystemTime::now(), local_offset_min()),
            _ => UsageSnapshot::empty(),
        };
        if last.as_ref() != Some(&snap) {
            crate::host::log(&format!("claude usage: {} arquivos, hoje {} respostas, ciclo {} ms, {} bytes lidos", snap.files, snap.messages_today, started.elapsed().as_millis(), scanner.last_bytes));
            if tx.send(snap.clone()).is_err() {
                return;
            }
            last = Some(snap);
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
    rx: Option<Receiver<UsageSnapshot>>,
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
        self.handle = std::thread::Builder::new().name("pod-usage".into()).spawn(move || worker(s, tx, wake_rx)).ok();
        if self.handle.is_some() {
            self.stop = stop;
            self.rx = Some(rx);
            self._wake = Some(wake_tx);
        }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.handle = None;
        self.rx = None;
        self._wake = None;
    }

    pub fn take_latest(&mut self) -> Option<UsageSnapshot> {
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
    use std::io::Write;

    fn line(id: &str, ts: &str, out: u64) -> String {
        format!(
            "{{\"type\":\"assistant\",\"timestamp\":\"{ts}\",\"message\":{{\"id\":\"{id}\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":{out},\"cache_read_input_tokens\":50,\"cache_creation_input_tokens\":2}}}}}}\n"
        )
    }

    fn temp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pod-usage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("proj-a").join("subagents")).unwrap();
        d
    }

    fn append(path: &Path, text: &str) {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    /// 2026-10-18 12:00 UTC.
    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs((pod_model::usage::days_from_civil(2026, 10, 18) * 86_400 + 12 * 3_600) as u64)
    }

    #[test]
    fn scans_incrementally_dedupes_and_ignores_other_files() {
        let root = temp_root("scan");
        let a = root.join("proj-a").join("s1.jsonl");
        let sub = root.join("proj-a").join("subagents").join("s2.jsonl");
        append(&a, &line("m1", "2026-10-18T10:00:00Z", 100));
        append(&a, "{\"type\":\"user\",\"message\":{\"content\":\"texto que nunca deve ser lido como uso\"}}\n");
        append(&sub, &line("m2", "2026-10-17T10:00:00Z", 7));
        append(&root.join("proj-a").join("notas.txt"), "nao e jsonl\n");

        let mut sc = Scanner::default();
        let s1 = sc.scan(&root, now(), 0);
        assert_eq!(s1.files, 2, "so os .jsonl, inclusive em subpastas");
        assert_eq!(s1.today().output, 100);
        assert_eq!(s1.days[5].output, 7, "ontem");
        assert_eq!(s1.messages_today, 1);
        assert!(sc.last_bytes > 0);

        // Sem novidade: nenhum byte lido de novo.
        let s2 = sc.scan(&root, now(), 0);
        assert_eq!(sc.last_bytes, 0, "incremental");
        assert_eq!(s1, s2);

        // Acrescenta a mesma resposta atualizada e uma nova: só os bytes novos são lidos e nada duplica.
        let added = line("m1", "2026-10-18T10:00:05Z", 130) + &line("m3", "2026-10-18T11:00:00Z", 20);
        append(&a, &added);
        let s3 = sc.scan(&root, now(), 0);
        assert_eq!(sc.last_bytes, added.len() as u64);
        assert_eq!(s3.today().output, 150, "m1 atualizada (130) + m3 (20)");
        assert_eq!(s3.messages_today, 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_half_written_line_waits_for_the_next_cycle() {
        let root = temp_root("partial");
        let f = root.join("proj-a").join("s.jsonl");
        let full = line("m1", "2026-10-18T10:00:00Z", 40);
        let (a, b) = full.split_at(full.len() - 15);
        append(&f, a);
        let mut sc = Scanner::default();
        assert_eq!(sc.scan(&root, now(), 0).today().output, 0, "linha incompleta nao conta");
        append(&f, b);
        assert_eq!(sc.scan(&root, now(), 0).today().output, 40, "completa no ciclo seguinte");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_truncated_file_is_read_again_and_missing_root_is_empty() {
        let root = temp_root("trunc");
        let f = root.join("proj-a").join("s.jsonl");
        append(&f, &(line("m1", "2026-10-18T10:00:00Z", 300) + &line("m2", "2026-10-18T10:01:00Z", 300)));
        let mut sc = Scanner::default();
        assert_eq!(sc.scan(&root, now(), 0).today().output, 600);
        std::fs::write(&f, line("m9", "2026-10-18T10:02:00Z", 5)).unwrap();
        let s = sc.scan(&root, now(), 0);
        assert!(s.today().output >= 5, "releu o arquivo menor");
        let none = Scanner::default().scan(&root.join("nao-existe"), now(), 0);
        assert_eq!(none, UsageSnapshot::empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn old_files_are_skipped() {
        let root = temp_root("old");
        let f = root.join("proj-a").join("s.jsonl");
        append(&f, &line("m1", "2026-10-18T10:00:00Z", 9));
        // "agora" 30 dias depois: o arquivo (recem-criado) parece de 30 dias atras.
        let later = now() + Duration::from_secs(30 * 86_400);
        let _ = later;
        let mut files = Vec::new();
        collect(&root, SystemTime::now() + Duration::from_secs(10 * 86_400), &mut files);
        assert!(files.is_empty(), "mais velho que 8 dias fica de fora");
        collect(&root, SystemTime::now(), &mut files);
        assert_eq!(files.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn local_offset_is_sane_and_provider_stops_reading() {
        let off = local_offset_min();
        assert!((-14 * 60..=14 * 60).contains(&off), "{off}");
        let mut p = Provider::default();
        assert_eq!(p.take_latest(), None);
        p.start();
        let mut got = None;
        for _ in 0..600 {
            if let Some(s) = p.take_latest() {
                got = Some(s);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let s = got.expect("a thread entregou um resumo em 60 s");
        eprintln!("claude usage neste PC: {} arquivos, {} respostas hoje", s.files, s.messages_today);
        p.stop();
        assert!(!p.running());
    }
}
