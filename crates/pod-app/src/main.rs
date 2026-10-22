//! M1: prova vertical — duas janelas WS_POPUP, D3D11 + D2D + DirectComposition,
//! resize nativo e DPI Per-Monitor V2. Sem texto ainda (próximo passo do M1).
#![windows_subsystem = "windows"]
// Um nome de constante não importado vira variável de padrão e engole as mensagens seguintes.
#![deny(unreachable_patterns)]

mod a11y;
mod autostart;
mod edit;
mod gallery;
mod claude_usage;
mod gfx;
mod icons;
mod host;
mod menu;
mod dock;
mod slot;
mod metrics;
mod monitors;
mod panel;
mod quotes;
mod pickers;
mod render;
mod runtime;
mod shell;
mod storage;
mod localdata;
mod media;
mod netbroker;
mod news;
mod vault;
mod weather;
mod github;
mod notif;
#[cfg(debug_assertions)]
mod shadow_layer;
#[cfg(debug_assertions)]
mod shadow_proof;
mod sysmetrics;
mod widgets;
mod sysinfo;
mod text;
mod tray;
mod ui;

fn main() -> windows::core::Result<()> {
    // `--gallery <pasta>`: renderiza os widgets offscreen e sai (sem janelas nem bandeja).
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--gallery") {
        let dir = args.get(i + 1).map(String::as_str).unwrap_or("gallery");
        if let Err(e) = gallery::run(std::path::Path::new(dir)) {
            let _ = std::fs::write(std::path::Path::new(dir).join("gallery.error"), e);
            std::process::exit(1);
        }
        return Ok(());
    }
    if dev_tools(&args) {
        return Ok(());
    }
    host::run()
}

/// Ferramentas de prova e diagnóstico (ADR-0005): criam janelas de teste e enviam entrada real. **Só existem em builds de
/// debug**; o executável de distribuição não as contém.
#[cfg(debug_assertions)]
fn dev_tools(args: &[String]) -> bool {
    if let Some(i) = args.iter().position(|a| a == "--shadow-proof-target") {
        let n = |k: usize| args.get(i + k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
        if let Err(e) = shadow_proof::run_target(n(1), n(2), n(3), n(4)) {
            eprintln!("shadow-proof-target: {e}");
            std::process::exit(1);
        }
        return true;
    }
    if let Some(i) = args.iter().position(|a| a == "--shadow-gate") {
        let dir = args.get(i + 1).map(String::as_str).unwrap_or("shadow-gate");
        let script = args.get(i + 2).map(String::as_str).unwrap_or("tools/gate-sample.ps1");
        if let Err(e) = shadow_proof::run_gate(std::path::Path::new(dir), script) {
            eprintln!("shadow-gate: {e}");
            std::process::exit(1);
        }
        return true;
    }
    if let Some(i) = args.iter().position(|a| a == "--shadow-layer") {
        let dir = args.get(i + 1).map(String::as_str).unwrap_or("shadow-layer");
        if let Err(e) = shadow_layer::run(std::path::Path::new(dir)) {
            eprintln!("shadow-layer: {e}");
            std::process::exit(1);
        }
        return true;
    }
    if let Some(i) = args.iter().position(|a| a == "--shadow-s56") {
        let dir = args.get(i + 1).map(String::as_str).unwrap_or("shadow-s56");
        let hold = args.get(i + 2).is_some_and(|a| a == "hold");
        if let Err(e) = shadow_layer::run_s56(std::path::Path::new(dir), hold) {
            eprintln!("shadow-s56: {e}");
            std::process::exit(1);
        }
        return true;
    }
    if let Some(i) = args.iter().position(|a| a == "--focus-card") {
        let variant = args.get(i + 1).map(String::as_str).unwrap_or("base");
        let n = |k: usize| args.get(i + k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        if let Err(e) = shadow_layer::run_card_only(variant, n(2) as i32, n(3) as i32, n(4).max(1) as u64) {
            eprintln!("focus-card: {e}");
            std::process::exit(1);
        }
        return true;
    }
    // `--shadow-proof <pasta>`: prova isolada da sombra ampla (ADR-0005, etapa 2); cria janelas de teste e sai.
    if let Some(i) = args.iter().position(|a| a == "--shadow-proof") {
        let dir = args.get(i + 1).map(String::as_str).unwrap_or("shadow-proof");
        if let Err(e) = shadow_proof::run(std::path::Path::new(dir)) {
            eprintln!("shadow-proof: {e}");
            std::process::exit(1);
        }
        return true;
    }
    false
}

#[cfg(not(debug_assertions))]
fn dev_tools(_: &[String]) -> bool {
    false
}
