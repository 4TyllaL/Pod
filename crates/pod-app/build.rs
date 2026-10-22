//! Compila os recursos do executável: manifesto, ícone e informações de versão.
//!
//! Sem dependências: usa o `rc.exe` do Windows SDK (variável `RC` ou o SDK mais novo instalado) e
//! passa o `.res` ao linker. A versão vem do Cargo, então nada fica duplicado à mão. A saída não
//! contém data nem caminhos da máquina, para não atrapalhar a build reproduzível.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=resources/pod.manifest");
    println!("cargo:rerun-if-changed=../../assets/pod.ico");
    println!("cargo:rerun-if-env-changed=RC");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let nums = four_part_version(&version);
    let (comma, dotted) = (
        format!("{},{},{},{}", nums[0], nums[1], nums[2], nums[3]),
        format!("{}.{}.{}.{}", nums[0], nums[1], nums[2], nums[3]),
    );

    let manifest = fs::read_to_string(manifest_dir.join("resources/pod.manifest")).expect("manifesto")
        .replace("@VERSION@", &dotted);
    let manifest_path = out.join("pod.manifest");
    fs::write(&manifest_path, manifest).expect("gravar manifesto");

    let icon = manifest_dir.join("../../assets/pod.ico").canonicalize().expect("assets/pod.ico");
    let rc_text = format!(
        r#"#pragma code_page(65001)
1 ICON "{icon}"
1 24 "{manifest}"
1 VERSIONINFO
 FILEVERSION {comma}
 PRODUCTVERSION {comma}
 FILEFLAGSMASK 0x3fL
 FILEFLAGS 0x0L
 FILEOS 0x40004L
 FILETYPE 0x1L
 FILESUBTYPE 0x0L
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "041604B0"
  BEGIN
   VALUE "FileDescription", "Pod"
   VALUE "FileVersion", "{dotted}"
   VALUE "InternalName", "pod"
   VALUE "OriginalFilename", "Pod.exe"
   VALUE "ProductName", "Pod"
   VALUE "ProductVersion", "{version}"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x416, 1200
 END
END
"#,
        icon = icon.to_string_lossy().replace('\\', "/").trim_start_matches("//?/"),
        manifest = manifest_path.to_string_lossy().replace('\\', "/"),
    );
    let rc_path = out.join("pod.rc");
    fs::write(&rc_path, rc_text).expect("gravar .rc");

    let rc = find_rc().unwrap_or_else(|| {
        panic!(
            "rc.exe não encontrado. Instale o Windows SDK (componente de ferramentas de desktop) ou \
             defina a variável RC com o caminho do rc.exe."
        )
    });
    let res = out.join("pod.res");
    let status = Command::new(&rc)
        .args(["/nologo", "/c65001", "/fo"])
        .arg(&res)
        .arg(&rc_path)
        .status()
        .expect("falha ao executar o rc.exe");
    assert!(status.success(), "rc.exe falhou ({status})");
    println!("cargo:rustc-link-arg-bins={}", res.display());
}

/// "0.1.0" -> [0, 1, 0, 0]; sufixos de pré-lançamento (-beta) são ignorados.
fn four_part_version(v: &str) -> [u32; 4] {
    let core = v.split(['-', '+']).next().unwrap_or(v);
    let mut out = [0u32; 4];
    for (i, part) in core.split('.').take(4).enumerate() {
        out[i] = part.parse().unwrap_or(0);
    }
    out
}

fn find_rc() -> Option<PathBuf> {
    if let Some(p) = env::var_os("RC") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let pf = env::var_os("ProgramFiles(x86)").or_else(|| env::var_os("ProgramFiles"))?;
    let bin = Path::new(&pf).join("Windows Kits").join("10").join("bin");
    let mut versions: Vec<PathBuf> = fs::read_dir(&bin).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    // Ordena por nome de versão (10.0.26100.0 > 10.0.19041.0); diretórios sem rc.exe são ignorados.
    versions.sort();
    versions.reverse();
    versions.into_iter().map(|d| d.join("x64").join("rc.exe")).find(|p| p.is_file())
}
