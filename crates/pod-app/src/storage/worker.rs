//! Worker de persistência: o único escritor em disco. Recebe cópias imutáveis, mantém no máximo
//! uma gravação em curso e o snapshot mais recente pendente (o mais novo substitui o pendente).

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use pod_model::schema::Workspace;

use super::files::{write_snapshot, FaultHook};

pub struct Job {
    pub generation: u64,
    pub workspace: Workspace,
}

#[derive(Debug)]
pub struct Ack {
    pub generation: u64,
    pub result: Result<(), String>,
}

#[derive(Default)]
struct Slot {
    pending: Option<Job>,
    shutdown: bool,
}

struct Shared {
    slot: Mutex<Slot>,
    cv: Condvar,
}

pub struct Worker {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
    pub acks: Receiver<Ack>,
}

impl Worker {
    /// `wake` é chamado (na thread do worker) depois de cada ack para acordar a UI. Não deve
    /// bloquear nem pedir trabalho à UI.
    pub fn spawn(dir: PathBuf, hook: Arc<dyn FaultHook + Send>, wake: Box<dyn Fn() + Send>) -> Self {
        let shared = Arc::new(Shared { slot: Mutex::new(Slot::default()), cv: Condvar::new() });
        let (tx, acks) = channel();
        let s = shared.clone();
        let handle = thread::Builder::new()
            .name("pod-storage".into())
            .spawn(move || run(s, dir, hook, tx, wake))
            .expect("não foi possível criar a thread de storage");
        Self { shared, handle: Some(handle), acks }
    }

    /// Enfileira um snapshot. Se já havia um pendente, ele é descartado (sem ack): o ack do mais
    /// novo cobre as revisões do descartado.
    pub fn submit(&self, job: Job) {
        let mut slot = self.shared.slot.lock().unwrap();
        slot.pending = Some(job);
        self.shared.cv.notify_one();
    }

    /// Termina depois de gravar o que estiver pendente.
    pub fn shutdown(&mut self) {
        {
            let mut slot = self.shared.slot.lock().unwrap();
            slot.shutdown = true;
            self.shared.cv.notify_one();
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(
    shared: Arc<Shared>,
    dir: PathBuf,
    hook: Arc<dyn FaultHook + Send>,
    tx: Sender<Ack>,
    wake: Box<dyn Fn() + Send>,
) {
    loop {
        let job = {
            let mut slot = shared.slot.lock().unwrap();
            loop {
                if let Some(j) = slot.pending.take() {
                    break Some(j);
                }
                if slot.shutdown {
                    break None;
                }
                slot = shared.cv.wait(slot).unwrap();
            }
        };
        let Some(job) = job else { return };
        // Sem lock durante serialização e I/O.
        let result = job
            .workspace
            .to_json()
            .map_err(|e| e.to_string())
            .and_then(|bytes| write_snapshot(&dir, &bytes, hook.as_ref()).map_err(|e| e.to_string()));
        let _ = tx.send(Ack { generation: job.generation, result });
        wake();
    }
}
