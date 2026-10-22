//! Lançador de apps e pastas: lista de atalhos do usuário. Só aceita caminhos absolutos do Windows de
//! programas (`.exe`, `.lnk`) ou pastas; nunca scripts nem argumentos. A validação final acontece de
//! novo no host na hora de abrir.

use serde::{Deserialize, Serialize};

pub const MAX_ITEMS: usize = 12;
pub const MAX_PATH: usize = 260;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcut {
    pub name: String,
    pub path: String,
    /// Pasta (abre no Explorador) ou programa.
    pub folder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    NotAbsolute,
    Invalid,
    NotAllowed,
    Full,
    Duplicate,
}

impl PathError {
    pub fn message(&self) -> &'static str {
        match self {
            PathError::NotAbsolute => "Escolha um caminho completo.",
            PathError::Invalid => "Caminho inv\u{e1}lido.",
            PathError::NotAllowed => "S\u{f3} programas (.exe, .lnk) e pastas.",
            PathError::Full => "Limite de atalhos atingido.",
            PathError::Duplicate => "Esse atalho j\u{e1} existe.",
        }
    }
}

/// `C:\pasta\arquivo` ou `\\servidor\pasta` (rede): absoluto, sem `..`, sem `/` misturada e sem
/// caracteres de controle.
fn well_formed(p: &str) -> bool {
    if p.is_empty() || p.len() > MAX_PATH || p.chars().any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '|' | '?' | '*')) {
        return false;
    }
    let drive = p.len() >= 3 && p.as_bytes()[0].is_ascii_alphabetic() && &p[1..3] == ":\\";
    let unc = p.starts_with("\\\\") && p.len() > 3 && !p.starts_with("\\\\?\\") && !p.starts_with("\\\\.\\");
    (drive || unc) && !p.split('\\').any(|part| part == "..")
}

/// Extensão (minúscula) do último componente, sem o ponto.
fn extension(p: &str) -> Option<String> {
    let last = p.rsplit('\\').next()?;
    let (stem, ext) = last.rsplit_once('.')?;
    (!stem.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// Nome de exibição: o nome do arquivo sem a extensão (ou o da pasta).
pub fn display_name(path: &str, folder: bool) -> String {
    let trimmed = path.trim_end_matches('\\');
    let last = trimmed.rsplit('\\').next().unwrap_or(trimmed);
    let name = if folder { last } else { last.rsplit_once('.').map_or(last, |(s, _)| s) };
    let name = if name.is_empty() || name.ends_with(':') { trimmed } else { name };
    name.chars().take(24).collect()
}

/// Valida um caminho escolhido e monta o atalho.
pub fn make(path: &str, folder: bool) -> Result<Shortcut, PathError> {
    let p = path.trim();
    if !p.starts_with("\\\\") && !(p.len() >= 2 && p.as_bytes()[1] == b':') {
        return Err(PathError::NotAbsolute);
    }
    if !well_formed(p) {
        return Err(PathError::Invalid);
    }
    if !folder && !matches!(extension(p).as_deref(), Some("exe" | "lnk")) {
        return Err(PathError::NotAllowed);
    }
    Ok(Shortcut { name: display_name(p, folder), path: p.to_string(), folder })
}

pub fn add(list: &mut Vec<Shortcut>, path: &str, folder: bool) -> Result<(), PathError> {
    let s = make(path, folder)?;
    if list.len() >= MAX_ITEMS {
        return Err(PathError::Full);
    }
    if list.iter().any(|x| x.path.eq_ignore_ascii_case(&s.path)) {
        return Err(PathError::Duplicate);
    }
    list.push(s);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_and_folders_are_accepted_and_named() {
        let s = make("C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe", false).unwrap();
        assert_eq!((s.name.as_str(), s.folder), ("chrome", false));
        assert_eq!(make("C:\\Users\\a\\Atalho.LNK", false).unwrap().name, "Atalho");
        let f = make("D:\\Projetos\\Pod\\", true).unwrap();
        assert_eq!(f.name, "Pod");
        assert_eq!(make("C:\\", true).unwrap().name, "C:");
        assert!(make("\\\\servidor\\compartilhado\\app.exe", false).is_ok());
    }

    #[test]
    fn scripts_relative_and_tricky_paths_are_refused() {
        for (p, folder) in [
            ("C:\\x\\run.bat", false),
            ("C:\\x\\run.ps1", false),
            ("C:\\x\\a.exe.txt", false),
            ("C:\\x\\.exe", false),
            ("calc.exe", false),
            ("..\\a.exe", false),
            ("C:\\x\\..\\y\\a.exe", false),
            ("C:/x/a.exe", false),
            ("\\\\?\\C:\\a.exe", false),
            ("C:\\x\\a|b.exe", false),
            ("", true),
            ("C:\\x\n\\a.exe", false),
        ] {
            assert!(make(p, folder).is_err(), "{p:?}");
        }
        assert_eq!(make("C:\\x\\run.bat", false), Err(PathError::NotAllowed));
        assert_eq!(make("calc.exe", false), Err(PathError::NotAbsolute));
        assert!(make(&format!("C:\\{}", "a".repeat(300)), true).is_err());
    }

    #[test]
    fn list_has_limits_and_no_duplicates_ignoring_case() {
        let mut l = Vec::new();
        add(&mut l, "C:\\a\\x.exe", false).unwrap();
        assert_eq!(add(&mut l, "c:\\A\\X.EXE", false), Err(PathError::Duplicate));
        for i in 1..MAX_ITEMS {
            add(&mut l, &format!("C:\\p{i}\\"), true).unwrap();
        }
        assert_eq!(add(&mut l, "C:\\extra\\", true), Err(PathError::Full));
    }
}
