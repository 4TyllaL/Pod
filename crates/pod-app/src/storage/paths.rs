//! Diretório de dados: `%LOCALAPPDATA%\Pod`, obtido pela API de known folders.
//!
//! Precedência: `--data-dir <pasta>` (ou `--data-dir=<pasta>`) na linha de comando, que serve a modo
//! portátil e a diagnóstico; em builds de debug, a variável `POD_DATA_DIR`; senão a pasta padrão.

use std::path::PathBuf;

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

/// Extrai `--data-dir` dos argumentos (sem o nome do programa). Valor vazio é ignorado.
pub fn parse_data_dir<I: IntoIterator<Item = String>>(args: I) -> Option<PathBuf> {
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let value = if let Some(v) = a.strip_prefix("--data-dir=") {
            Some(v.to_string())
        } else if a == "--data-dir" {
            it.next()
        } else {
            None
        };
        if let Some(v) = value.filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(v));
        }
    }
    None
}

pub fn data_dir() -> Result<PathBuf, String> {
    if let Some(d) = parse_data_dir(std::env::args().skip(1)) {
        return Ok(d);
    }
    if cfg!(debug_assertions) {
        if let Some(d) = std::env::var_os("POD_DATA_DIR") {
            return Ok(PathBuf::from(d));
        }
    }
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT, None).map_err(|e| e.to_string())?;
        let s = p.to_string().map_err(|e| e.to_string());
        CoTaskMemFree(Some(p.0 as *const _));
        Ok(PathBuf::from(s?).join("Pod"))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_data_dir;
    use std::path::PathBuf;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn accepts_both_forms_and_unicode_paths() {
        assert_eq!(parse_data_dir(args(&["--data-dir", "C:\\Dados Pod"])), Some(PathBuf::from("C:\\Dados Pod")));
        assert_eq!(parse_data_dir(args(&["--data-dir=D:\\a\u{e7}\u{e3}o"])), Some(PathBuf::from("D:\\a\u{e7}\u{e3}o")));
        assert_eq!(parse_data_dir(args(&["--outra", "x", "--data-dir=E:\\p"])), Some(PathBuf::from("E:\\p")));
    }

    #[test]
    fn ignores_missing_or_empty_values() {
        assert_eq!(parse_data_dir(args(&[])), None);
        assert_eq!(parse_data_dir(args(&["--data-dir"])), None);
        assert_eq!(parse_data_dir(args(&["--data-dir="])), None);
        assert_eq!(parse_data_dir(args(&["--data-dirx", "a"])), None);
    }
}
