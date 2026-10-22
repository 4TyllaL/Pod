//! Gravação atômica do snapshot, carga e recuperação. Só `std` e `ReplaceFileW`; sem UI nem COM.
//!
//! Regras (ARCHITECTURE §8): nunca apagar o destino válido para destravar uma gravação; nunca
//! escolher um `.tmp` como estado confirmado; preservar evidência antes de qualquer recuperação;
//! recusar versão de schema mais nova sem tocar nos arquivos.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use pod_model::schema::{SchemaError, Workspace, MAX_SNAPSHOT_BYTES, SCHEMA_VERSION};
use windows::core::HSTRING;
use windows::Win32::Storage::FileSystem::{ReplaceFileW, REPLACE_FILE_FLAGS};

pub const MAIN: &str = "workspace.json";
pub const PREVIOUS: &str = "workspace.previous.json";
pub const RECOVERY_DIR: &str = "recovery";

/// Pontos onde um teste pode injetar falha para provar a recuperação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    CreateTemp,
    WriteTemp,
    FlushTemp,
    Replace,
}

pub trait FaultHook: Sync {
    fn check(&self, step: Step) -> io::Result<()>;
}

pub struct NoFaults;

impl FaultHook for NoFaults {
    fn check(&self, _: Step) -> io::Result<()> {
        Ok(())
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(dir: &Path) -> PathBuf {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("workspace.{}-{}.tmp", std::process::id(), n))
}

/// Grava `bytes` como novo estado confirmado: temp único no mesmo diretório → flush → substituição.
/// Em qualquer falha o arquivo confirmado anterior continua intacto.
pub fn write_snapshot(dir: &Path, bytes: &[u8], hook: &dyn FaultHook) -> io::Result<()> {
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "snapshot acima do limite"));
    }
    let tmp = temp_path(dir);
    let result = write_via_temp(dir, &tmp, bytes, hook);
    if result.is_err() {
        // Melhor esforço: não deixar lixo próprio. Um temp de processo morto é tratado no início.
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_via_temp(dir: &Path, tmp: &Path, bytes: &[u8], hook: &dyn FaultHook) -> io::Result<()> {
    hook.check(Step::CreateTemp)?;
    let mut f = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    hook.check(Step::WriteTemp)?;
    f.write_all(bytes)?;
    hook.check(Step::FlushTemp)?;
    f.sync_all()?;
    drop(f); // fecha antes de substituir
    hook.check(Step::Replace)?;
    let main = dir.join(MAIN);
    if main.exists() {
        // Preserva a geração anterior como `workspace.previous.json`.
        let (m, r, b) = (HSTRING::from(main.as_os_str()), HSTRING::from(tmp.as_os_str()), HSTRING::from(dir.join(PREVIOUS).as_os_str()));
        unsafe { ReplaceFileW(&m, &r, &b, REPLACE_FILE_FLAGS(0), None, None) }
            .map_err(|e| io::Error::from_raw_os_error(win32_code(&e)))
    } else {
        // Primeira gravação: destino inexistente, `rename` no mesmo volume.
        fs::rename(tmp, &main)
    }
}

fn win32_code(e: &windows::core::Error) -> i32 {
    // HRESULT_FROM_WIN32 (0x8007xxxx) → código Win32.
    let h = e.code().0 as u32;
    if h & 0xFFFF_0000 == 0x8007_0000 { (h & 0xFFFF) as i32 } else { h as i32 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Main,
    /// O principal estava ausente ou inválido; o estado veio da geração anterior.
    PreviousRecovered,
}

#[derive(Debug)]
pub enum LoadOutcome {
    /// Nenhum estado anterior: primeira execução.
    Fresh,
    /// `migrated_from`: versão antiga do schema lida do arquivo (o original foi preservado).
    Loaded { workspace: Workspace, source: Source, migrated_from: Option<u32> },
    /// Gravado por versão mais nova: o aplicativo não pode escrever neste diretório.
    NewerVersion { found: u32, supported: u32 },
    /// Nada utilizável; os originais foram preservados. Oferecer workspace novo ao usuário.
    Unusable { main: Option<String>, previous: Option<String> },
}

enum Read {
    Missing,
    Ok(Workspace, Vec<u8>, u32),
    Newer(u32, u32),
    Bad(String),
}

fn read_file(path: &Path) -> io::Result<Read> {
    let len = match fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Read::Missing),
        Err(e) => return Err(e),
    };
    if len as usize > MAX_SNAPSHOT_BYTES {
        return Ok(Read::Bad(SchemaError::TooLarge(len as usize).to_string()));
    }
    let bytes = fs::read(path)?;
    Ok(match Workspace::from_json_detailed(&bytes) {
        Ok((ws, version)) => Read::Ok(ws, bytes, version),
        Err(SchemaError::NewerVersion { found, supported }) => Read::Newer(found, supported),
        Err(e) => Read::Bad(e.to_string()),
    })
}

fn stamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{secs}-{n}")
}

/// Copia `src` para `recovery/<prefixo>-<carimbo>-<nome>`. Evidência nunca é sobrescrita.
fn preserve_copy(dir: &Path, src: &Path, prefix: &str) -> io::Result<()> {
    let rec = dir.join(RECOVERY_DIR);
    fs::create_dir_all(&rec)?;
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    fs::copy(src, rec.join(format!("{prefix}-{}-{name}", stamp()))).map(|_| ())
}

/// Move temporários órfãos (gravações interrompidas) para `recovery/`. Nunca os usa como estado.
/// Só chamar com a trava exclusiva do workspace já adquirida.
pub fn quarantine_stray_temps(dir: &Path) -> io::Result<usize> {
    let rec = dir.join(RECOVERY_DIR);
    let mut moved = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("workspace.") && name.ends_with(".tmp") && entry.file_type()?.is_file() {
            fs::create_dir_all(&rec)?;
            fs::rename(entry.path(), rec.join(format!("tmp-{}-{name}", stamp())))?;
            moved += 1;
        }
    }
    Ok(moved)
}

/// Carrega o workspace aplicando as regras de recuperação. Só chamar com a trava adquirida.
pub fn load(dir: &Path, hook: &dyn FaultHook) -> io::Result<LoadOutcome> {
    quarantine_stray_temps(dir)?;
    let main_path = dir.join(MAIN);
    let prev_path = dir.join(PREVIOUS);

    let main_err = match read_file(&main_path)? {
        Read::Ok(ws, _, version) => {
            // Migração em cópia: o arquivo antigo é preservado antes de a próxima gravação o
            // substituir por um de schema novo.
            let migrated_from = (version < SCHEMA_VERSION).then_some(version);
            if let Some(v) = migrated_from {
                preserve_copy(dir, &main_path, &format!("premigration-v{v}"))?;
            }
            return Ok(LoadOutcome::Loaded { workspace: ws, source: Source::Main, migrated_from });
        }
        Read::Newer(found, supported) => return Ok(LoadOutcome::NewerVersion { found, supported }),
        Read::Missing => None,
        Read::Bad(e) => {
            preserve_copy(dir, &main_path, "corrupt")?; // evidência antes de qualquer reparo
            Some(e)
        }
    };

    match read_file(&prev_path)? {
        Read::Ok(ws, bytes, version) => {
            // Restaura o principal a partir da geração anterior (senão a próxima gravação
            // substituiria a boa pela corrompida ao rotacionar o backup).
            if main_path.exists() {
                fs::remove_file(&main_path)?; // já preservado em recovery/
            }
            write_snapshot(dir, &bytes, hook)?;
            let migrated_from = (version < SCHEMA_VERSION).then_some(version);
            Ok(LoadOutcome::Loaded { workspace: ws, source: Source::PreviousRecovered, migrated_from })
        }
        Read::Newer(found, supported) => Ok(LoadOutcome::NewerVersion { found, supported }),
        Read::Missing if main_err.is_none() => Ok(LoadOutcome::Fresh),
        Read::Missing => Ok(LoadOutcome::Unusable { main: main_err, previous: None }),
        Read::Bad(e) => {
            preserve_copy(dir, &prev_path, "corrupt")?;
            Ok(LoadOutcome::Unusable { main: main_err, previous: Some(e) })
        }
    }
}

/// Depois que o usuário aceita começar do zero: move (não apaga) os originais para `recovery/`.
pub fn quarantine_all(dir: &Path) -> io::Result<()> {
    let rec = dir.join(RECOVERY_DIR);
    fs::create_dir_all(&rec)?;
    for name in [MAIN, PREVIOUS] {
        let p = dir.join(name);
        if p.exists() {
            fs::rename(&p, rec.join(format!("discarded-{}-{name}", stamp())))?;
        }
    }
    Ok(())
}

/// Trava exclusiva do workspace: handle sem compartilhamento mantido durante toda a execução.
/// Cobre também outra sessão do mesmo usuário. A existência do arquivo não indica nada.
pub struct WorkspaceLock {
    _file: File,
}

impl WorkspaceLock {
    pub fn acquire(dir: &Path) -> io::Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        fs::create_dir_all(dir)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(dir.join("workspace.lock"))?;
        Ok(Self { _file: file })
    }
}

pub fn is_sharing_violation(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32) | Some(33))
}
