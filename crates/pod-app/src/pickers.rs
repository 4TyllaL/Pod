//! Seletores do Windows (arquivo de programa e pasta) e abertura de atalhos do lançador. O widget só
//! recebe o caminho escolhido; quem abre é o host, depois de validar de novo.

use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Escolhe um programa (`.exe` ou atalho `.lnk`). `None` se o usuário cancelou.
pub fn pick_program(owner: Option<HWND>) -> Option<String> {
    let mut buf = [0u16; 520];
    // Filtro: pares "descrição\0padrão\0", terminado por \0\0.
    let filter: Vec<u16> = "Programas (*.exe;*.lnk)\0*.exe;*.lnk\0".encode_utf16().chain([0]).collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner.unwrap_or_default(),
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        lpstrTitle: w!("Escolha um programa"),
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_EXPLORER,
        ..Default::default()
    };
    unsafe {
        if !GetOpenFileNameW(&mut ofn).as_bool() {
            return None;
        }
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

/// Escolhe uma pasta. `None` se o usuário cancelou.
pub fn pick_folder(owner: Option<HWND>) -> Option<String> {
    let mut name = [0u16; 260];
    let info = BROWSEINFOW {
        hwndOwner: owner.unwrap_or_default(),
        pszDisplayName: PWSTR(name.as_mut_ptr()),
        lpszTitle: w!("Escolha uma pasta"),
        ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
        ..Default::default()
    };
    unsafe {
        let pidl = SHBrowseForFolderW(&info);
        if pidl.is_null() {
            return None;
        }
        let mut path = [0u16; 260];
        let ok = SHGetPathFromIDListW(pidl, &mut path).as_bool();
        windows::Win32::System::Com::CoTaskMemFree(Some(pidl as *const _));
        if !ok {
            return None;
        }
        let end = path.iter().position(|&c| c == 0).unwrap_or(path.len());
        Some(String::from_utf16_lossy(&path[..end]))
    }
}

/// Abre um atalho do lançador: revalida o caminho (absoluto, `.exe`/`.lnk` ou pasta), confere que
/// existe e é do tipo certo, e entrega ao sistema **sem argumentos**.
pub fn launch(path: &str, folder: bool) -> Result<(), String> {
    let s = pod_model::launcher::make(path, folder).map_err(|e| e.message().to_string())?;
    let meta = std::fs::metadata(&s.path).map_err(|_| "n\u{e3}o encontrado".to_string())?;
    if meta.is_dir() != folder {
        return Err("tipo diferente do salvo".into());
    }
    #[cfg(debug_assertions)]
    if std::env::var_os("POD_DRY_RUN_LINKS").is_some() {
        crate::host::log(&format!("atalho (simulado) folder={folder} path={}", s.path));
        return Ok(());
    }
    unsafe {
        let r = ShellExecuteW(None, w!("open"), &HSTRING::from(s.path.as_str()), None, None, SW_SHOWNORMAL);
        if r.0 as isize <= 32 {
            return Err(format!("ShellExecute {}", r.0 as isize));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_refuses_anything_that_is_not_an_existing_program_or_folder() {
        assert!(launch("calc.exe", false).is_err(), "relativo");
        assert!(launch("C:\\Windows\\System32\\cmd.bat", false).is_err(), "script");
        assert!(launch("C:\\nao\\existe\\app.exe", false).is_err(), "nao existe");
        assert!(launch("C:\\Windows\\", false).is_err(), "pasta salva como programa");
        let exe = std::env::current_exe().unwrap();
        assert!(launch(exe.to_str().unwrap(), true).is_err(), "arquivo salvo como pasta");
    }
}
