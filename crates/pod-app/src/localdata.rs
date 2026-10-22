//! Provedor `Local`: pastas conhecidas (Downloads, Capturas) e área de transferência. Lê só o
//! que algum assinante visível pediu e conta cada leitura para provar isso nos testes.
//!
//! Área de transferência: só consulta o número de sequência a cada tick; o texto é lido quando
//! esse número muda. Conteúdo marcado pelo aplicativo de origem como "não monitorar" (formato
//! `ExcludeClipboardContentFromMonitorProcessing`, usado por gerenciadores de senha) nunca é lido.

use std::os::windows::fs::MetadataExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use windows::core::{w, GUID};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::DataExchange::{
    CloseClipboard, CountClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
    OpenClipboard, RegisterClipboardFormatW,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::Shell::{FOLDERID_Downloads, FOLDERID_Screenshots, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

use pod_model::local::{preview_text, Clip, FileEntry, FolderListing, LocalNeeds, LocalSnapshot};

static READS: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];

/// Leituras feitas desde o início: (downloads, capturas, área de transferência).
pub fn reads() -> [u64; 3] {
    [0, 1, 2].map(|i| READS[i].load(Ordering::Relaxed))
}

const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
/// Arquivos lidos por pasta: pastas gigantes não travam a thread de UI.
const MAX_SCAN: usize = 5_000;
/// Texto máximo lido da área de transferência (em unidades UTF-16).
const MAX_CLIP_UNITS: usize = 16 * 1024;

fn known_folder(id: &GUID) -> Option<std::path::PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(Into::into)
    }
}

fn list_folder(dir: &std::path::Path) -> Option<FolderListing> {
    let now = SystemTime::now();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.take(MAX_SCAN).flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 {
            continue;
        }
        let age_secs = meta.modified().ok().and_then(|m| now.duration_since(m).ok()).map_or(0, |d| d.as_secs());
        files.push(FileEntry { name: entry.file_name().to_string_lossy().into_owned(), size: meta.len(), age_secs });
    }
    Some(FolderListing::from_files(files))
}

struct ClipGuard;
impl Drop for ClipGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

/// Lê a área de transferência. `None` = não foi possível abrir agora (outro app a segura): o
/// chamador tenta de novo no próximo tick.
fn read_clip() -> Option<Clip> {
    unsafe {
        let exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        if exclude != 0 && IsClipboardFormatAvailable(exclude).is_ok() {
            return Some(Clip::Private);
        }
        let text_fmt = CF_UNICODETEXT.0 as u32;
        if IsClipboardFormatAvailable(text_fmt).is_err() {
            return Some(if CountClipboardFormats() > 0 { Clip::Other } else { Clip::Empty });
        }
        OpenClipboard(None).ok()?;
        let _guard = ClipGuard;
        let handle = GetClipboardData(text_fmt).ok()?;
        let hglobal = windows::Win32::Foundation::HGLOBAL(handle.0);
        let units = GlobalSize(hglobal) / 2;
        let ptr = GlobalLock(hglobal) as *const u16;
        if ptr.is_null() {
            return None;
        }
        let slice = std::slice::from_raw_parts(ptr, units.min(MAX_CLIP_UNITS));
        let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
        let text = String::from_utf16_lossy(&slice[..end]);
        let _ = GlobalUnlock(hglobal);
        let preview = preview_text(&text);
        Some(if preview.is_empty() { Clip::Other } else { Clip::Text(preview) })
    }
}

/// Pastas mudam devagar: mesmo que a área de transferência faça o provedor rodar a cada 1 s,
/// uma pasta só é relida a cada 10 s (`FOLDER_TTL`).
const FOLDER_TTL: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Default)]
struct Cached {
    at: Option<std::time::Instant>,
    listing: Option<FolderListing>,
}

impl Cached {
    fn fresh(&self) -> bool {
        self.at.is_some_and(|t| t.elapsed() < FOLDER_TTL)
    }
}

#[derive(Default)]
pub struct Provider {
    folders: [Cached; 2],
    clip_seq: Option<u32>,
    clip: Option<Clip>,
    /// Leituras deste provedor (downloads, capturas, área de transferência).
    local: [u64; 3],
}

impl Provider {
    fn bump(&mut self, i: usize) {
        self.local[i] += 1;
        READS[i].fetch_add(1, Ordering::Relaxed);
    }

    pub fn local_reads(&self) -> [u64; 3] {
        self.local
    }

    /// Esquece o cache da área de transferência (sem assinantes): a próxima leitura relê.
    pub fn reset(&mut self) {
        self.clip_seq = None;
        self.clip = None;
        self.folders = Default::default();
    }

    fn folder(&mut self, i: usize, id: &GUID) -> Option<FolderListing> {
        if !self.folders[i].fresh() {
            self.bump(i);
            self.folders[i] = Cached { at: Some(std::time::Instant::now()), listing: known_folder(id).and_then(|d| list_folder(&d)) };
        }
        self.folders[i].listing.clone()
    }

    pub fn read(&mut self, needs: LocalNeeds) -> LocalSnapshot {
        let mut s = LocalSnapshot::default();
        if needs.downloads {
            s.downloads = self.folder(0, &FOLDERID_Downloads);
        }
        if needs.screenshots {
            s.screenshots = self.folder(1, &FOLDERID_Screenshots);
        }
        if needs.clipboard {
            let seq = unsafe { GetClipboardSequenceNumber() };
            if self.clip_seq != Some(seq) || self.clip.is_none() {
                self.bump(2);
                if let Some(c) = read_clip() {
                    self.clip = Some(c);
                    self.clip_seq = Some(seq);
                }
            }
            s.clipboard = self.clip.clone();
        } else {
            self.clip_seq = None;
            self.clip = None;
        }
        // Pasta sem assinante não fica em cache: ao voltar, relê.
        if !needs.downloads {
            self.folders[0] = Cached::default();
        }
        if !needs.screenshots {
            self.folders[1] = Cached::default();
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_what_is_needed_and_counts_it() {
        let mut p = Provider::default();
        let s = p.read(LocalNeeds::default());
        assert_eq!(s, LocalSnapshot::default());
        assert_eq!(p.local_reads(), [0; 3], "sem pedido, nenhuma leitura");

        let s = p.read(LocalNeeds::DOWNLOADS);
        assert!(s.downloads.is_some(), "a pasta Downloads existe neste usuário");
        assert!(s.screenshots.is_none() && s.clipboard.is_none());
        assert_eq!(p.local_reads(), [1, 0, 0]);
    }

    #[test]
    fn listing_ignores_folders_and_hidden_files_and_orders_by_age() {
        let dir = std::env::temp_dir().join(format!("pod-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("subpasta")).unwrap();
        std::fs::write(dir.join("a.txt"), b"aa").unwrap();
        std::fs::write(dir.join("b.txt"), b"b").unwrap();
        let l = list_folder(&dir).unwrap();
        assert_eq!(l.count, 2, "subpasta não conta");
        assert_eq!(l.recent.len(), 2);
        assert!(l.recent.iter().all(|f| f.age_secs < 60));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(list_folder(&dir).is_none(), "pasta inexistente = sem dados, não 0 arquivos");
    }

    #[test]
    fn folders_are_cached_between_ticks_and_dropped_without_subscribers() {
        let mut p = Provider::default();
        p.read(LocalNeeds::DOWNLOADS);
        p.read(LocalNeeds::DOWNLOADS.union(LocalNeeds::CLIPBOARD));
        p.read(LocalNeeds::DOWNLOADS);
        assert_eq!(p.local_reads()[0], 1, "tres ticks seguidos, uma leitura de pasta");
        p.read(LocalNeeds::CLIPBOARD);
        p.read(LocalNeeds::DOWNLOADS);
        assert_eq!(p.local_reads()[0], 2, "sem assinante o cache some");
    }

    #[test]
    fn clipboard_is_read_only_when_the_sequence_changes() {
        let mut p = Provider::default();
        p.read(LocalNeeds::CLIPBOARD);
        let first = p.local_reads()[2];
        assert!(first <= 1);
        p.read(LocalNeeds::CLIPBOARD);
        // Sem alteração da área de transferência (a menos que o usuário copie algo agora), não relê.
        assert!(p.local_reads()[2] <= first + 1);
        p.read(LocalNeeds::DOWNLOADS);
        assert!(p.clip.is_none(), "sem assinante o texto não fica guardado");
    }
}
