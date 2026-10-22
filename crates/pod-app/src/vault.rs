//! Cofre de segredos: o Gerenciador de Credenciais do Windows (por usuário, criptografado pelo
//! sistema). Guarda o token do GitHub; nada de segredo vai para o `workspace.json`, para logs ou para
//! o estado dos widgets.

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Security::Credentials::*;

/// Nome do segredo do GitHub.
pub const GITHUB: &str = "Pod/github-token";

pub fn store(target: &str, secret: &str) -> bool {
    let mut name: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    let mut user: Vec<u16> = "Pod".encode_utf16().chain(Some(0)).collect();
    let mut blob = secret.as_bytes().to_vec();
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user.as_mut_ptr()),
        ..Default::default()
    };
    let ok = unsafe { CredWriteW(&cred, 0).is_ok() };
    blob.fill(0);
    ok
}

pub fn load(target: &str) -> Option<String> {
    unsafe {
        let mut p: *mut CREDENTIALW = std::ptr::null_mut();
        CredReadW(&HSTRING::from(target), CRED_TYPE_GENERIC, None, &mut p).ok()?;
        let c = &*p;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
        let out = String::from_utf8(bytes.to_vec()).ok();
        CredFree(p as *const _);
        out
    }
}

pub fn forget(target: &str) -> bool {
    unsafe { CredDeleteW(&HSTRING::from(target), CRED_TYPE_GENERIC, None).is_ok() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_round_trips_and_is_forgotten() {
        let t = "Pod/test-vault-roundtrip";
        assert_eq!(load(t), None);
        assert!(store(t, "valor-de-teste-123"));
        assert_eq!(load(t).as_deref(), Some("valor-de-teste-123"));
        assert!(store(t, "outro"));
        assert_eq!(load(t).as_deref(), Some("outro"));
        assert!(forget(t));
        assert_eq!(load(t), None);
        assert!(!forget(t));
    }
}
