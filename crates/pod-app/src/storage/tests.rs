use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::time::Duration;

use pod_model::placement::PreferredPlacement;
use pod_model::schema::{InstanceRecord, Workspace};

use super::files::*;
use super::worker::{Job, Worker};

/// Diretório temporário único, removido ao final.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "pod-test-{}-{}-{}",
            std::process::id(),
            tag,
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ws(generation: u64, state: &str) -> Workspace {
    let mut w = Workspace::empty();
    w.generation = generation;
    w.instances.push(InstanceRecord {
        id: pod_model::ids::WidgetId(42).to_hex(),
        type_id: "pod.test-card".into(),
        state_version: 1,
        state: state.into(),
        visible: true,
        placement: PreferredPlacement::from_legacy(-100, 20, 320.0, 240.0),
        shape: None,
        settings: None,
        extra: Default::default(),
    });
    w
}

fn put(dir: &Path, w: &Workspace) {
    write_snapshot(dir, &w.to_json().unwrap(), &NoFaults).unwrap();
}

fn loaded(dir: &Path) -> (Workspace, Source) {
    match load(dir, &NoFaults).unwrap() {
        LoadOutcome::Loaded { workspace, source, .. } => (workspace, source),
        other => panic!("esperava Loaded, veio {other:?}"),
    }
}

fn recovery_files(dir: &Path) -> Vec<String> {
    match fs::read_dir(dir.join(RECOVERY_DIR)) {
        Ok(rd) => rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => Vec::new(),
    }
}

struct FailAt(Step);

impl FaultHook for FailAt {
    fn check(&self, step: Step) -> io::Result<()> {
        if step == self.0 {
            Err(io::Error::other(format!("falha injetada em {step:?}")))
        } else {
            Ok(())
        }
    }
}

#[test]
fn first_run_is_fresh() {
    let d = TempDir::new("fresh");
    assert!(matches!(load(d.path(), &NoFaults).unwrap(), LoadOutcome::Fresh));
}

#[test]
fn write_then_load_roundtrips_and_keeps_the_previous_generation() {
    let d = TempDir::new("roundtrip");
    put(d.path(), &ws(1, "a"));
    put(d.path(), &ws(2, "b"));
    let (w, source) = loaded(d.path());
    assert_eq!((w.generation, source), (2, Source::Main));
    let prev = Workspace::from_json(&fs::read(d.path().join(PREVIOUS)).unwrap()).unwrap();
    assert_eq!(prev.generation, 1, "workspace.previous.json guarda a geração anterior");
}

#[test]
fn a_failure_at_any_step_leaves_the_last_confirmed_generation_intact() {
    for step in [Step::CreateTemp, Step::WriteTemp, Step::FlushTemp, Step::Replace] {
        let d = TempDir::new("fault");
        put(d.path(), &ws(1, "confirmado"));
        let err = write_snapshot(d.path(), &ws(2, "perdido").to_json().unwrap(), &FailAt(step));
        assert!(err.is_err(), "{step:?} deveria falhar");
        let (w, _) = loaded(d.path());
        assert_eq!(w.generation, 1, "após falha em {step:?}");
        assert_eq!(w.instances[0].state, "confirmado");
        let tmps: Vec<_> = fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(tmps.is_empty(), "falha em {step:?} deixou temporário: {tmps:?}");
    }
}

#[test]
fn a_failure_on_the_very_first_write_leaves_nothing_loadable_but_nothing_corrupt() {
    for step in [Step::CreateTemp, Step::WriteTemp, Step::FlushTemp, Step::Replace] {
        let d = TempDir::new("first-fault");
        assert!(write_snapshot(d.path(), &ws(1, "x").to_json().unwrap(), &FailAt(step)).is_err());
        assert!(matches!(load(d.path(), &NoFaults).unwrap(), LoadOutcome::Fresh), "{step:?}");
    }
}

#[test]
fn orphan_temp_from_a_killed_process_is_preserved_never_used_as_state() {
    let d = TempDir::new("orphan");
    put(d.path(), &ws(1, "confirmado"));
    // Processo morto no meio da gravação: temp com conteúdo válido, porém não confirmado.
    fs::write(d.path().join("workspace.999-0.tmp"), ws(2, "nao-confirmado").to_json().unwrap()).unwrap();
    let (w, _) = loaded(d.path());
    assert_eq!(w.generation, 1);
    assert!(!d.path().join("workspace.999-0.tmp").exists());
    assert!(recovery_files(d.path()).iter().any(|n| n.contains("workspace.999-0.tmp")));
}

#[test]
fn corrupt_main_recovers_from_previous_preserving_evidence_and_restoring_main() {
    let d = TempDir::new("corrupt");
    put(d.path(), &ws(1, "bom"));
    put(d.path(), &ws(2, "mais novo"));
    fs::write(d.path().join(MAIN), b"{ truncado").unwrap();
    let (w, source) = loaded(d.path());
    assert_eq!((w.generation, source), (1, Source::PreviousRecovered));
    assert!(recovery_files(d.path()).iter().any(|n| n.starts_with("corrupt-")), "evidência preservada");
    // O principal voltou a ser válido: a próxima gravação não rotaciona lixo para o backup.
    put(d.path(), &ws(3, "depois"));
    let prev = Workspace::from_json(&fs::read(d.path().join(PREVIOUS)).unwrap()).unwrap();
    assert_eq!(prev.generation, 1);
    let (w, source) = loaded(d.path());
    assert_eq!((w.generation, source), (3, Source::Main));
}

#[test]
fn missing_main_with_valid_previous_is_recovered() {
    let d = TempDir::new("missing-main");
    put(d.path(), &ws(1, "a"));
    put(d.path(), &ws(2, "b"));
    fs::remove_file(d.path().join(MAIN)).unwrap(); // p.ex. falha no meio de ReplaceFileW
    let (w, source) = loaded(d.path());
    assert_eq!((w.generation, source), (1, Source::PreviousRecovered));
}

#[test]
fn both_files_bad_is_unusable_and_originals_are_never_overwritten() {
    let d = TempDir::new("both-bad");
    fs::write(d.path().join(MAIN), b"lixo A").unwrap();
    fs::write(d.path().join(PREVIOUS), b"lixo B").unwrap();
    match load(d.path(), &NoFaults).unwrap() {
        LoadOutcome::Unusable { main, previous } => assert!(main.is_some() && previous.is_some()),
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read(d.path().join(MAIN)).unwrap(), b"lixo A");
    assert_eq!(fs::read(d.path().join(PREVIOUS)).unwrap(), b"lixo B");
    assert!(recovery_files(d.path()).len() >= 2);

    // O usuário aceita começar do zero: os originais são movidos, não apagados.
    quarantine_all(d.path()).unwrap();
    assert!(!d.path().join(MAIN).exists());
    assert!(recovery_files(d.path()).iter().any(|n| n.starts_with("discarded-")));
    assert!(matches!(load(d.path(), &NoFaults).unwrap(), LoadOutcome::Fresh));
}

#[test]
fn newer_schema_is_refused_and_files_are_left_byte_identical() {
    let d = TempDir::new("newer");
    let future = br#"{"schema_version": 99, "formato": "do futuro"}"#;
    fs::write(d.path().join(MAIN), future).unwrap();
    let before = fs::read_dir(d.path()).unwrap().count();
    match load(d.path(), &NoFaults).unwrap() {
        LoadOutcome::NewerVersion { found: 99, .. } => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read(d.path().join(MAIN)).unwrap(), future);
    assert_eq!(fs::read_dir(d.path()).unwrap().count(), before, "nenhum arquivo criado ou movido");
    assert!(recovery_files(d.path()).is_empty());
}

#[test]
fn lock_is_exclusive_until_released() {
    let d = TempDir::new("lock");
    let first = WorkspaceLock::acquire(d.path()).unwrap();
    let second = WorkspaceLock::acquire(d.path());
    assert!(second.as_ref().err().is_some_and(is_sharing_violation), "segunda trava deveria falhar");
    drop(first);
    WorkspaceLock::acquire(d.path()).expect("liberada após o drop");
}

#[test]
fn worker_acks_the_generation_actually_written_and_wakes_the_ui() {
    let d = TempDir::new("worker");
    let wakes = Arc::new(AtomicU32::new(0));
    let w2 = wakes.clone();
    let worker = Worker::spawn(
        d.path().to_path_buf(),
        Arc::new(NoFaults),
        Box::new(move || {
            w2.fetch_add(1, Ordering::SeqCst);
        }),
    );
    worker.submit(Job { generation: 1, workspace: ws(1, "um") });
    let ack = worker.acks.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!((ack.generation, ack.result), (1, Ok(())));
    // O worker envia o ack e só depois chama o despertador: espera o despertar em vez de supor a ordem.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while wakes.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    assert_eq!(loaded(d.path()).0.generation, 1, "o ack só chega com o arquivo em disco");
}

#[test]
fn worker_failure_is_reported_in_the_ack_and_keeps_the_old_file() {
    let d = TempDir::new("worker-fail");
    put(d.path(), &ws(1, "confirmado"));
    let worker = Worker::spawn(d.path().to_path_buf(), Arc::new(FailAt(Step::Replace)), Box::new(|| {}));
    worker.submit(Job { generation: 2, workspace: ws(2, "novo") });
    let ack = worker.acks.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(ack.generation, 2);
    assert!(ack.result.is_err());
    assert_eq!(loaded(d.path()).0.generation, 1);
}

#[test]
fn worker_flushes_pending_work_on_shutdown_and_drops_superseded_snapshots() {
    let d = TempDir::new("worker-latest");
    let mut worker = Worker::spawn(d.path().to_path_buf(), Arc::new(NoFaults), Box::new(|| {}));
    for g in 1..=50 {
        worker.submit(Job { generation: g, workspace: ws(g, &g.to_string()) });
    }
    worker.shutdown(); // grava o que estiver pendente antes de terminar
    let mut last = 0;
    loop {
        match worker.acks.recv_timeout(Duration::from_millis(50)) {
            Ok(a) => {
                assert!(a.result.is_ok());
                assert!(a.generation > last, "acks em ordem crescente");
                last = a.generation;
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        }
    }
    assert_eq!(last, 50, "o mais recente sempre é gravado");
    assert_eq!(loaded(d.path()).0.generation, 50);
}

#[test]
fn oversized_snapshot_is_refused_without_touching_disk() {
    let d = TempDir::new("oversize");
    put(d.path(), &ws(1, "ok"));
    let huge = vec![b'x'; pod_model::schema::MAX_SNAPSHOT_BYTES + 1];
    assert!(write_snapshot(d.path(), &huge, &NoFaults).is_err());
    assert_eq!(loaded(d.path()).0.generation, 1);
}

#[test]
fn schema1_file_is_migrated_with_the_original_preserved_before_any_write() {
    let d = TempDir::new("migrate");
    let v1 = br#"{"schema_version":1,"generation":4,"instances":[{"id":"0000000000000000000000000000000a",
        "type_id":"pod.quick-notes","state_version":1,"state":"conteudo","visible":true,
        "placement":{"x":-1500,"y":120,"width_dip":320.0,"height_dip":240.0}}]}"#;
    fs::write(d.path().join(MAIN), v1).unwrap();
    match load(d.path(), &NoFaults).unwrap() {
        LoadOutcome::Loaded { workspace, migrated_from, .. } => {
            assert_eq!(migrated_from, Some(1));
            assert_eq!(workspace.schema_version, pod_model::schema::SCHEMA_VERSION);
            assert_eq!(workspace.instances[0].state, "conteudo");
        }
        other => panic!("{other:?}"),
    }
    // O original (v1) já está preservado e é byte a byte idêntico, antes de qualquer gravação.
    let copy = recovery_files(d.path()).into_iter().find(|n| n.starts_with("premigration-v1-")).expect("cópia");
    assert_eq!(fs::read(d.path().join(RECOVERY_DIR).join(copy)).unwrap(), v1);
    assert_eq!(fs::read(d.path().join(MAIN)).unwrap(), v1, "o principal ainda não foi reescrito");

    // Depois de gravar, o principal é schema 2 e a próxima carga não é mais migração.
    let (ws, _) = loaded(d.path());
    put(d.path(), &ws);
    match load(d.path(), &NoFaults).unwrap() {
        LoadOutcome::Loaded { migrated_from, .. } => assert_eq!(migrated_from, None),
        other => panic!("{other:?}"),
    }
}
