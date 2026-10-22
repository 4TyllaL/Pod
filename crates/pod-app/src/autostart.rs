//! "Iniciar com o Windows": um valor `Pod` em `HKCU\...\Run` apontando para este executável.
//! O estado é lido do registro (fonte da verdade, também editável pelo Gerenciador de Tarefas).

use windows::core::w;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::*;

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: windows::core::PCWSTR = w!("Pod");

pub fn enabled() -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_QUERY_VALUE, &mut key) != ERROR_SUCCESS {
            return false;
        }
        let found = RegQueryValueExW(key, VALUE, None, None, None, None) == ERROR_SUCCESS;
        let _ = RegCloseKey(key);
        found
    }
}

/// Liga (com o caminho do executável atual, entre aspas) ou desliga. Devolve se deu certo.
pub fn set(on: bool) -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key) != ERROR_SUCCESS {
            return false;
        }
        let ok = if on {
            let Ok(exe) = std::env::current_exe() else {
                let _ = RegCloseKey(key);
                return false;
            };
            let text = format!("\"{}\"", exe.display());
            let bytes: Vec<u8> = text.encode_utf16().chain(std::iter::once(0u16)).flat_map(|c| c.to_le_bytes()).collect();
            RegSetValueExW(key, VALUE, None, REG_SZ, Some(&bytes)) == ERROR_SUCCESS
        } else {
            let r = RegDeleteValueW(key, VALUE);
            r == ERROR_SUCCESS || !enabled()
        };
        let _ = RegCloseKey(key);
        ok
    }
}
