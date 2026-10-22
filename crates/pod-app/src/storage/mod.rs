//! Persistência: snapshot JSON versionado, worker de escrita, rastreador de autosave e trava.
//! Nada aqui conhece HWND, COM ou widgets; o shell conecta as peças.

pub mod files;
pub mod paths;
pub mod session;
pub mod tracker;
pub mod worker;

#[cfg(test)]
mod tests;
