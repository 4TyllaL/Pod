//! Sessão de persistência: trava + worker + rastreador + geração. É a visão que o shell tem do
//! storage. Não conhece janelas nem widgets: recebe registros prontos e devolve acks.

use std::path::PathBuf;
use std::sync::Arc;

use pod_model::schema::{InstanceRecord, Preferences, Workspace, SCHEMA_VERSION};

use super::files::{FaultHook, WorkspaceLock};
use super::tracker::{SaveState, SaveTracker};
use super::worker::{Ack, Job, Worker};

/// Preferência do vidro translúcido (ADR-0002, opção B).
pub const PREF_GLASS: &str = "glass";
/// Preferencia "Widgets de internet" (opt-in; ausente = desligada).
pub const PREF_INTERNET: &str = "internet";
/// Inverso do "Modo desempenho" (ausente = modo ligado: esconde os cartões sob um app em tela cheia).
pub const PREF_KEEP_IN_FULLSCREEN: &str = "keep_in_fullscreen";

/// Dados do arquivo carregado que o aplicativo não interpreta e precisa devolver ao disco.
#[derive(Default)]
pub struct Carried {
    pub preferences: Preferences,
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}

pub struct Session {
    _lock: WorkspaceLock,
    worker: Worker,
    tracker: SaveTracker,
    generation: u64,
    carried: Carried,
}

impl Session {
    pub fn open(
        dir: PathBuf,
        lock: WorkspaceLock,
        last_generation: u64,
        carried: Carried,
        hook: Arc<dyn FaultHook + Send>,
        wake: Box<dyn Fn() + Send>,
    ) -> Self {
        Self {
            _lock: lock,
            worker: Worker::spawn(dir, hook, wake),
            tracker: SaveTracker::default(),
            generation: last_generation,
            carried,
        }
    }

    /// Preferência booleana gravada em `preferences` (ausente = desligada).
    pub fn bool_pref(&self, key: &str) -> bool {
        self.carried.preferences.extra.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
    }

    pub fn set_bool_pref(&mut self, key: &str, value: bool) {
        self.carried.preferences.extra.insert(key.to_string(), serde_json::Value::Bool(value));
    }

    /// Dados compartilhados a gravar junto do workspace (`extra["shared"]`).
    pub fn set_shared(&mut self, value: serde_json::Value) {
        self.carried.extra.insert("shared".to_string(), value);
    }

    pub fn mark_changed(&mut self, now_ms: u64) {
        self.tracker.mark_changed(now_ms);
    }

    pub fn due_at(&self) -> Option<u64> {
        self.tracker.due_at()
    }

    pub fn should_save(&self, now_ms: u64) -> bool {
        self.tracker.should_save(now_ms)
    }

    pub fn has_unsent_changes(&self) -> bool {
        self.tracker.has_unsent_changes()
    }

    pub fn state(&self) -> SaveState {
        self.tracker.state()
    }

    pub fn acked_generation(&self) -> u64 {
        self.tracker.acked_generation()
    }

    fn workspace(&self, generation: u64, instances: Vec<InstanceRecord>) -> Workspace {
        Workspace {
            schema_version: SCHEMA_VERSION,
            generation,
            preferences: self.carried.preferences.clone(),
            instances,
            extra: self.carried.extra.clone(),
        }
    }

    /// Envia um snapshot consistente ao worker e devolve sua geração.
    pub fn save(&mut self, instances: Vec<InstanceRecord>) -> u64 {
        self.generation += 1;
        let generation = self.generation;
        self.tracker.begin_save(generation);
        self.worker.submit(Job { generation, workspace: self.workspace(generation, instances) });
        generation
    }

    /// Consome os acks disponíveis, atualiza o rastreador e os devolve ao shell.
    pub fn poll_acks(&mut self, now_ms: u64) -> Vec<Ack> {
        let acks: Vec<Ack> = self.worker.acks.try_iter().collect();
        for a in &acks {
            self.tracker.on_ack(a.generation, a.result.clone(), now_ms);
        }
        acks
    }

    /// Encerramento: grava o snapshot final e espera o worker terminar (com teto de tempo).
    /// Devolve `Ok` só se a geração final foi confirmada.
    pub fn close(&mut self, instances: Vec<InstanceRecord>, now_ms: u64) -> Result<(), String> {
        let generation = self.save(instances);
        self.worker.shutdown();
        self.poll_acks(now_ms);
        if self.tracker.acked_generation() >= generation {
            Ok(())
        } else {
            match self.tracker.state() {
                SaveState::Error(e) => Err(e),
                _ => Err("o snapshot final não foi confirmado".into()),
            }
        }
    }
}
