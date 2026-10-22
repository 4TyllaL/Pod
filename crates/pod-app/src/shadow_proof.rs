//! Prova isolada da sombra ampla (ADR-0005, etapa 2). **Não faz parte do produto**: só roda com
//! `pod-app --shadow-proof <pasta>`, cria janelas de teste, mede e sai. Não toca nos cartões.
//!
//! Compara dois desenhos de sombra com margem de 24 DIP ao redor de um cartão de 88 DIP:
//! - **A**: uma janela única em camadas (alfa por pixel) maior que o cartão; a margem devolve
//!   `HTTRANSPARENT` no `WM_NCHITTEST`.
//! - **B**: o cartão e, atrás dele, uma janela só de sombra com `WS_EX_TRANSPARENT` (nunca recebe cliques).
//!
//! Mede: clique/acerto na margem e no cartão (S1), foco (S2), memória com 40 janelas (S4) e a sobreposição
//! da sombra de um cartão no vizinho a 12 DIP (S3). Grava `report.md` e `shadow-proof.png`.

use std::fmt::Write as _;
use std::path::Path;

use windows::core::{w, Result};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Foundation::{COLORREF, FILETIME, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::*;

const CARD_DIP: f32 = 88.0;
pub(crate) const MARGIN_DIP: f32 = 24.0;
pub(crate) const RADIUS_DIP: f32 = 22.0;
const GAP_DIP: f32 = 12.0;
pub(crate) const SHADOW_ALPHA: f32 = 0.30;
pub(crate) const SHADOW_DY_DIP: f32 = 8.0;
const N_WINDOWS: usize = 40;
const GATE_SECONDS: u64 = 6;

/// Buffer BGRA com alfa pré-multiplicado.
pub(crate) struct Buf {
    pub(crate) w: usize,
    pub(crate) h: usize,
    pub(crate) px: Vec<u32>,
}

pub(crate) fn rounded_mask(w: usize, h: usize, x0: f32, y0: f32, cw: f32, ch: f32, r: f32) -> Vec<f32> {
    let mut m = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (cx, cy) = (x0 + cw / 2.0, y0 + ch / 2.0);
            let (dx, dy) = ((px - cx).abs() - (cw / 2.0 - r), (py - cy).abs() - (ch / 2.0 - r));
            let d = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() + dx.max(dy).min(0.0) - r;
            m[y * w + x] = (0.5 - d).clamp(0.0, 1.0);
        }
    }
    m
}

pub(crate) fn box_blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut tmp = vec![0.0; w * h];
    let mut out = vec![0.0; w * h];
    let norm = (2 * r + 1) as f32;
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for k in x.saturating_sub(r)..=(x + r).min(w - 1) {
                s += src[y * w + k];
            }
            tmp[y * w + x] = s / norm;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for k in y.saturating_sub(r)..=(y + r).min(h - 1) {
                s += tmp[k * w + x];
            }
            out[y * w + x] = s / norm;
        }
    }
    out
}

pub(crate) fn premul(a: f32, rgb: [f32; 3]) -> u32 {
    let a8 = (a.clamp(0.0, 1.0) * 255.0).round() as u32;
    let c = |v: f32| ((v * a.clamp(0.0, 1.0)).clamp(0.0, 255.0)).round() as u32;
    a8 << 24 | c(rgb[0]) << 16 | c(rgb[1]) << 8 | c(rgb[2])
}

/// (janela inteira com sombra + cartão, só sombra, só cartão sem margem). Cartão branco, sombra preta.
fn render(scale: f32) -> (Buf, Buf, Buf) {
    let card = (CARD_DIP * scale).round();
    let margin = (MARGIN_DIP * scale).round();
    let r = RADIUS_DIP * scale;
    let total = (card + 2.0 * margin) as usize;
    let mask_card = rounded_mask(total, total, margin, margin, card, card, r);
    // Sombra: máscara do cartão deslocada para baixo e borrada (três caixas ≈ gaussiana).
    let dy = (SHADOW_DY_DIP * scale).round();
    let shifted = rounded_mask(total, total, margin, margin + dy, card, card, r);
    let rad = ((MARGIN_DIP * scale) / 3.0).round().max(1.0) as usize;
    let mut blur = shifted;
    for _ in 0..3 {
        blur = box_blur(&blur, total, total, rad);
    }
    let mut full = vec![0u32; total * total];
    let mut shadow = vec![0u32; total * total];
    for i in 0..total * total {
        let sa = blur[i] * SHADOW_ALPHA;
        shadow[i] = premul(sa, [0.0, 0.0, 0.0]);
        let ca = mask_card[i];
        // card (branco) sobre sombra (preta): saída = c + s*(1-ca), pré-multiplicada.
        let a = ca + sa * (1.0 - ca);
        let rgb = ca * 255.0;
        full[i] = {
            let a8 = (a.clamp(0.0, 1.0) * 255.0).round() as u32;
            let c = rgb.round().clamp(0.0, 255.0) as u32;
            a8 << 24 | c << 16 | c << 8 | c
        };
    }
    let cs = card as usize;
    let mask_only = rounded_mask(cs, cs, 0.0, 0.0, card, card, r);
    let card_only = mask_only.iter().map(|a| premul(*a, [255.0, 255.0, 255.0])).collect();
    (Buf { w: total, h: total, px: full }, Buf { w: total, h: total, px: shadow }, Buf { w: cs, h: cs, px: card_only })
}

fn hit_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_NCHITTEST => {
                // GWLP_USERDATA: margem | largura do cartão << 16 (0 = janela só com cartão: tudo é cliente).
                let data = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as usize;
                let (margin, card) = ((data & 0xFFFF) as i32, ((data >> 16) & 0xFFFF) as i32);
                if card == 0 {
                    return LRESULT(HTCLIENT as isize);
                }
                let mut pt = POINT { x: (lp.0 & 0xFFFF) as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32 };
                let _ = ScreenToClient(hwnd, &mut pt);
                let inside = pt.x >= margin && pt.x < margin + card && pt.y >= margin && pt.y < margin + card;
                LRESULT((if inside { HTCLIENT as i32 } else { HTTRANSPARENT }) as isize)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

unsafe extern "system" fn proof_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    hit_proc(hwnd, msg, wp, lp)
}

unsafe fn register() -> Result<()> {
    let hinst = GetModuleHandleW(None)?.into();
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proof_proc),
        hInstance: hinst,
        lpszClassName: w!("PodShadowProof"),
        ..Default::default()
    };
    if RegisterClassExW(&class) == 0 {
        // Já registrada nesta execução não é erro.
    }
    Ok(())
}

/// Janela em camadas, sem ativar, mostrada em (x, y) com o buffer como imagem (alfa por pixel).
unsafe fn layered(buf: &Buf, x: i32, y: i32, transparent: bool, user: usize) -> Result<HWND> {
    let ex = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | if transparent { WS_EX_TRANSPARENT } else { WINDOW_EX_STYLE(0) };
    let hinst = GetModuleHandleW(None)?.into();
    let hwnd = CreateWindowExW(ex, w!("PodShadowProof"), w!("pod-shadow-proof"), WS_POPUP, x, y, buf.w as i32, buf.h as i32, None, None, Some(hinst), None)?;
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, user as isize);
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(Some(screen));
    let bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: buf.w as i32,
            biHeight: -(buf.h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let dib = CreateDIBSection(Some(mem), &bi, DIB_RGB_COLORS, &mut bits, None, 0)?;
    std::ptr::copy_nonoverlapping(buf.px.as_ptr(), bits as *mut u32, buf.px.len());
    let old = SelectObject(mem, dib.into());
    let size = SIZE { cx: buf.w as i32, cy: buf.h as i32 };
    let dst = POINT { x, y };
    let src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
    UpdateLayeredWindow(hwnd, Some(screen), Some(&dst), Some(&size), Some(mem), Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA)?;
    SelectObject(mem, old);
    let _ = DeleteObject(dib.into());
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    Ok(hwnd)
}

pub(crate) fn describe(h: HWND) -> String {
    if h.0.is_null() {
        return "(nenhuma)".into();
    }
    unsafe {
        let mut name = [0u16; 64];
        let n = GetClassNameW(h, &mut name) as usize;
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        format!("{} (pid {pid})", String::from_utf16_lossy(&name[..n]))
    }
}

fn private_bytes() -> usize {
    unsafe {
        let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let _ = GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb);
        c.PrivateUsage
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for n in 0..256u32 {
        let mut c = n;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        table[n as usize] = c;
    }
    !data.iter().fold(!0u32, |c, b| table[((c ^ *b as u32) & 0xFF) as usize] ^ (c >> 8))
}

/// PNG RGB sem compressão (blocos "stored"): suficiente para uma imagem de revisão.
fn write_png(path: &Path, w: usize, h: usize, rgb: &[u8]) -> std::io::Result<()> {
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for y in 0..h {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]);
    }
    let mut z = vec![0x78, 0x01];
    for (i, chunk) in raw.chunks(65535).enumerate() {
        let last = (i + 1) * 65535 >= raw.len();
        z.push(last as u8);
        z.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(chunk.len() as u16)).to_le_bytes());
        z.extend_from_slice(chunk);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for v in &raw {
        a = (a + *v as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&(b << 16 | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    std::fs::write(path, out)
}

/// Compõe a janela de cada cartão em ordem (o de trás primeiro) sobre `bg`, como o DWM faria.
fn composite(bgc: [u8; 3], wins: &[(&Buf, i32)]) -> (usize, usize, Vec<u8>) {
    let h = wins[0].0.h;
    let w = (wins.iter().map(|(b, x)| *x + b.w as i32).max().unwrap_or(0)) as usize;
    let mut img = vec![0u8; w * h * 3];
    for p in img.chunks_mut(3) {
        p.copy_from_slice(&bgc);
    }
    for (buf, x0) in wins {
        for y in 0..buf.h {
            for x in 0..buf.w {
                let px = buf.px[y * buf.w + x];
                let a = (px >> 24) as f32 / 255.0;
                let tx = *x0 as usize + x;
                if tx >= w {
                    continue;
                }
                let o = (y * w + tx) * 3;
                for (k, sh) in [16u32, 8, 0].iter().enumerate() {
                    let c = ((px >> sh) & 0xFF) as f32;
                    img[o + k] = (c + img[o + k] as f32 * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    (w, h, img)
}

pub fn run(dir: &Path) -> std::result::Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    unsafe { inner(dir).map_err(|e| e.to_string()) }
}

unsafe fn inner(dir: &Path) -> Result<()> {
    register()?;
    let scale = GetDpiForSystem() as f32 / 96.0;
    let (full, shadow, card) = render(scale);
    let margin = (MARGIN_DIP * scale).round() as i32;
    let cardpx = card.w as i32;
    let mut rep = String::new();
    let _ = writeln!(rep, "# Prova da sombra ampla (ADR-0005, etapa 2)\n");
    let _ = writeln!(rep, "Escala do sistema: {:.2} (card {cardpx} px, margem {margin} px, janela {} px).\n", scale, full.w);

    // Pontos de prova (centro do cartão e três na margem) numa região livre do monitor primário.
    let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let (wx, wy) = (sw / 2 - full.w as i32 / 2, sh / 2 - full.h as i32 / 2);
    let (cx, cy) = (wx + margin + cardpx / 2, wy + margin + cardpx / 2);
    let probes = [
        ("centro do cartão", cx, cy),
        ("margem externa (alfa 0)", wx + 2, cy),
        ("margem com sombra (esquerda)", wx + margin - 6, cy),
        ("margem com sombra (abaixo)", cx, wy + margin + cardpx + 6),
    ];
    let log = dir.join("target-clicks.log");
    let _ = std::fs::remove_file(&log);
    let exe = std::env::current_exe().map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()))?;
    let mut target = std::process::Command::new(exe)
        .args(["--shadow-proof-target", &(wx - 40).to_string(), &(wy - 40).to_string(), &(full.w as i32 + 80).to_string(), &(full.h as i32 + 80).to_string()])
        .env("POD_PROOF_LOG", &log)
        .spawn()
        .map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()))?;
    for _ in 0..40 {
        if FindWindowW(w!("PodShadowTarget"), None).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    pump();
    let baseline: Vec<HWND> = probes.iter().map(|(_, x, y)| WindowFromPoint(POINT { x: *x, y: *y })).collect();
    let fg0 = GetForegroundWindow();

    let table = |label: &str, owners: &[HWND], rep: &mut String| {
        let _ = writeln!(rep, "### {label}\n");
        let _ = writeln!(rep, "| Ponto | Sem janelas de prova | Com as janelas | Resultado esperado | OK |");
        let _ = writeln!(rep, "| --- | --- | --- | --- | --- |");
        for (i, (name, x, y)) in probes.iter().enumerate() {
            let got = WindowFromPoint(POINT { x: *x, y: *y });
            let is_ours = owners.contains(&got);
            let want_ours = i == 0;
            let ok = is_ours == want_ours;
            let _ = writeln!(
                rep,
                "| {name} | {} | {} | {} | {} |",
                describe(baseline[i]),
                describe(got),
                if want_ours { "cartão (nossa janela)" } else { "a janela de baixo (cliques passam)" },
                if ok { "sim" } else { "**NÃO**" }
            );
        }
        let _ = writeln!(rep);
    };

    // --- A: janela única com margem ---
    let a = layered(&full, wx, wy, false, margin as usize | (cardpx as usize) << 16)?;
    pump();
    table("A — janela única com margem e `HTTRANSPARENT` na margem", &[a], &mut rep);
    click_table(&mut rep, "A", &probes, &log);
    let fg_a = GetForegroundWindow() == fg0;
    let _ = DestroyWindow(a);
    pump();

    // --- B: janela de sombra (WS_EX_TRANSPARENT) atrás do cartão ---
    let sh_w = layered(&shadow, wx, wy, true, 0)?;
    let cd_w = layered(&card, wx + margin, wy + margin, false, 0)?;
    let _ = SetWindowPos(sh_w, Some(cd_w), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    pump();
    table("B — cartão + janela de sombra `WS_EX_TRANSPARENT` atrás", &[cd_w], &mut rep);
    click_table(&mut rep, "B", &probes, &log);
    let fg_b = GetForegroundWindow() == fg0;
    let _ = DestroyWindow(sh_w);
    let _ = DestroyWindow(cd_w);
    pump();
    let _ = target.kill();
    let _ = writeln!(rep, "### Foco (S2)\n\nO primeiro plano não mudou após criar as janelas: A = {}, B = {} (esperado: sim).\n", if fg_a { "sim" } else { "**NÃO**" }, if fg_b { "sim" } else { "**NÃO**" });

    // --- S4: memória com N janelas ---
    let mem_run = |make: &dyn Fn(i32) -> Vec<HWND>| -> (usize, usize) {
        pump();
        let before = private_bytes();
        let t = std::time::Instant::now();
        let wins: Vec<HWND> = (0..N_WINDOWS as i32).flat_map(|i| make(i)).collect();
        pump();
        let after = private_bytes();
        let ms = t.elapsed().as_millis() as usize;
        for w in &wins {
            let _ = DestroyWindow(*w);
        }
        pump();
        (after.saturating_sub(before), ms)
    };
    let x_of = |i: i32| 20 + (i % 10) * 40;
    let y_of = |i: i32| 20 + (i / 10) * 40;
    let (m0, t0) = mem_run(&|i| vec![layered(&card, x_of(i), y_of(i), false, 0).unwrap()]);
    let (ma, ta) = mem_run(&|i| vec![layered(&full, x_of(i), y_of(i), false, margin as usize | (cardpx as usize) << 16).unwrap()]);
    let (mb, tb) = mem_run(&|i| {
        let s = layered(&shadow, x_of(i), y_of(i), true, 0).unwrap();
        let c = layered(&card, x_of(i) + margin, y_of(i) + margin, false, 0).unwrap();
        vec![s, c]
    });
    let kb = |n: usize| n as f64 / 1024.0;
    let _ = writeln!(rep, "### Memória e tempo com {N_WINDOWS} janelas (S4)\n");
    let _ = writeln!(rep, "| Configuração | Janelas | Pixels por cartão | Memória privada (KiB) | Tempo de criação (ms) |");
    let _ = writeln!(rep, "| --- | --- | --- | --- | --- |");
    let _ = writeln!(rep, "| Só cartão (hoje, sem margem) | {N_WINDOWS} | {} | {:.0} | {t0} |", card.w * card.h, kb(m0));
    let _ = writeln!(rep, "| A — janela única com margem | {N_WINDOWS} | {} | {:.0} | {ta} |", full.w * full.h, kb(ma));
    let _ = writeln!(rep, "| B — cartão + sombra | {} | {} | {:.0} | {tb} |", N_WINDOWS * 2, card.w * card.h + shadow.w * shadow.h, kb(mb));
    let _ = writeln!(rep, "\nRazão de pixels A/hoje = {:.2}×; B/hoje = {:.2}×. Só a memória do processo (bitmaps de GDI e estruturas de janela); o custo no compositor (DWM/GPU) **não** foi medido.\n", (full.w * full.h) as f64 / (card.w * card.h) as f64, (card.w * card.h + shadow.w * shadow.h) as f64 / (card.w * card.h) as f64);

    // --- S3: vizinho a 12 DIP ---
    let gap = (GAP_DIP * scale).round() as i32;
    // A janela do cartão 2 fica acima da do cartão 1. Sua margem esquerda tem `margin` px; os primeiros
    // `margin - gap` px dela caem sobre o cartão 1 (o resto cai no intervalo entre os cartões).
    let overlap = (margin - gap).max(0) as usize;
    let mut max_a = 0u32;
    let mut sum_a = 0u64;
    let mut n = 0u64;
    for y in (margin as usize)..(margin as usize + cardpx as usize) {
        for x in 0..overlap {
            let a = shadow.px[y * shadow.w + x] >> 24;
            max_a = max_a.max(a);
            sum_a += a as u64;
            n += 1;
        }
    }
    let mean_a = if n > 0 { sum_a as f64 / n as f64 } else { 0.0 };
    let _ = writeln!(rep, "### Vizinhança a {GAP_DIP} DIP (S3)\n");
    let _ = writeln!(
        rep,
        "A margem de {MARGIN_DIP} DIP do cartão 2 invade o cartão 1 (intervalo de só {GAP_DIP} DIP). Alfa da sombra do cartão 2 sobre a faixa do cartão 1 sob essa margem: **máximo {max_a}/255 ({:.0}%), média {:.1}/255**. Um cartão branco coberto por alfa {max_a}/255 de preto escurece até {} em 255 nessa borda.\n",
        100.0 * max_a as f64 / 255.0,
        mean_a,
        (255.0 * (1.0 - max_a as f64 / 255.0)).round()
    );

    // --- imagem: dois cartões vizinhos sobre fundo claro e escuro ---
    let step_px = cardpx + gap;
    let mut rows: Vec<(usize, usize, Vec<u8>)> = Vec::new();
    for bg in [[0xDD, 0xE1, 0xEA], [0x1E, 0x22, 0x30]] {
        rows.push(composite(bg, &[(&full, 0), (&full, step_px)]));
    }
    let w_img = rows[0].0;
    let h_img = rows[0].1;
    let mut stack = Vec::with_capacity(rows.iter().map(|r| r.2.len()).sum());
    for r in &rows {
        stack.extend_from_slice(&r.2);
    }
    write_png(&dir.join("shadow-proof.png"), w_img, h_img * 2, &stack).map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()))?;
    let _ = writeln!(rep, "Imagem de revisão: `shadow-proof.png` (dois cartões vizinhos sobre fundo claro e escuro, o da direita por cima).");
    std::fs::write(dir.join("report.md"), &rep).map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()))?;
    Ok(())
}


/// Processo-alvo: uma janela comum, de outro processo, que registra os cliques que recebe
/// (`POD_PROOF_LOG`, linhas "x,y"). Roda com `--shadow-proof-target x y w h`.
pub fn run_target(x: i32, y: i32, w: i32, h: i32) -> std::result::Result<(), String> {
    unsafe {
        let hinst = GetModuleHandleW(None).map_err(|e| e.to_string())?.into();
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(target_proc),
            hInstance: hinst,
            lpszClassName: w!("PodShadowTarget"),
            ..Default::default()
        };
        RegisterClassExW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            w!("PodShadowTarget"),
            w!("pod-shadow-target"),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(hinst),
            None,
        )
        .map_err(|e| e.to_string())?;
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(Some(hwnd), 1, 40_000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

unsafe extern "system" fn target_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            if let Ok(path) = std::env::var("POD_PROOF_LOG") {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                    let _ = writeln!(f, "{},{}", (lp.0 & 0xFFFF) as i16, ((lp.0 >> 16) & 0xFFFF) as i16);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Clique esquerdo real (SendInput) em (x, y) de tela; restaura o cursor depois.
pub(crate) unsafe fn click(x: i32, y: i32) {
    let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let mut old = POINT::default();
    let _ = GetCursorPos(&mut old);
    let abs = |v: i32, max: i32| (v as i64 * 65535 / (max - 1) as i64) as i32;
    let mk = |flags: MOUSE_EVENT_FLAGS| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: abs(x, sw), dy: abs(y, sh), mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    };
    let seq = [mk(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE), mk(MOUSEEVENTF_LEFTDOWN | MOUSEEVENTF_ABSOLUTE), mk(MOUSEEVENTF_LEFTUP | MOUSEEVENTF_ABSOLUTE)];
    SendInput(&seq, std::mem::size_of::<INPUT>() as i32);
    std::thread::sleep(std::time::Duration::from_millis(150));
    let _ = SetCursorPos(old.x, old.y);
}

fn log_lines(path: &Path) -> usize {
    std::fs::read_to_string(path).map(|t| t.lines().count()).unwrap_or(0)
}

/// Cliques reais em cada ponto: o alvo (outro processo) deve recebê-los em todos menos no centro do cartão.
unsafe fn click_table(rep: &mut String, label: &str, probes: &[(&str, i32, i32)], log: &Path) {
    let _ = writeln!(rep, "### Cliques reais — {label}\n");
    let _ = writeln!(rep, "| Ponto | Quem recebeu o clique | Esperado | OK |");
    let _ = writeln!(rep, "| --- | --- | --- | --- |");
    for (i, (name, x, y)) in probes.iter().enumerate() {
        let before = log_lines(log);
        click(*x, *y);
        let target_got = log_lines(log) > before;
        let want_target = i != 0;
        let _ = writeln!(
            rep,
            "| {name} | {} | {} | {} |",
            if target_got { "janela de baixo (outro processo)" } else { "janela de prova" },
            if want_target { "janela de baixo" } else { "janela de prova" },
            if target_got == want_target { "sim" } else { "**NÃO**" }
        );
    }
    let _ = writeln!(rep);
}

// ---------------------------------------------------------------------------------------------
// Portao de 40 janelas (ADR-0005, etapa 3): custo de memoria, CPU e GPU da variacao B (cartao +
// janela de sombra), com o mesmo caminho de DirectComposition dos cartoes (`gfx::Surface`).
// ---------------------------------------------------------------------------------------------

use crate::gfx::{Gfx, Surface};

/// Uma janela da fase: cartao, janela inteira (A) ou sombra (B). So cartoes e janelas inteiras redesenham.
struct Item {
    hwnd: HWND,
    surface: Option<Surface>, // None = janela em camadas (sem DirectComposition)
    live: bool,
}

pub(crate) fn paint(surf: &Surface, buf: &Buf, tick: Option<u32>) -> Result<()> {
    surf.draw_custom(96, |ctx| unsafe {
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            ..Default::default()
        };
        let bmp = ctx.CreateBitmap(
            D2D_SIZE_U { width: buf.w as u32, height: buf.h as u32 },
            Some(buf.px.as_ptr() as *const _),
            (buf.w * 4) as u32,
            &props,
        )?;
        ctx.DrawBitmap(&bmp, None, 1.0, D2D1_INTERPOLATION_MODE_LINEAR, None, None);
        if let Some(t) = tick {
            // Um pontinho que alterna a cada quadro (como o ponteiro de um relogio): forca um quadro novo.
            let c = if t % 2 == 0 { D2D1_COLOR_F { r: 1.0, g: 0.4, b: 0.2, a: 1.0 } } else { D2D1_COLOR_F { r: 0.2, g: 0.4, b: 1.0, a: 1.0 } };
            let brush = ctx.CreateSolidColorBrush(&c, None)?;
            let (cx, cy) = (buf.w as f32 / 2.0, buf.h as f32 / 2.0);
            ctx.FillRectangle(&D2D_RECT_F { left: cx - 3.0, top: cy - 3.0, right: cx + 3.0, bottom: cy + 3.0 }, &brush);
        }
        Ok(())
    })
}

unsafe fn dcomp_window(gfx: &Gfx, buf: &Buf, x: i32, y: i32, transparent: bool, live: bool) -> Result<Item> {
    dcomp_window_ex(gfx, buf, x, y, transparent, live, false)
}

unsafe fn dcomp_window_ex(gfx: &Gfx, buf: &Buf, x: i32, y: i32, transparent: bool, live: bool, layered_flag: bool) -> Result<Item> {
    let ex = WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | if transparent { WS_EX_TRANSPARENT } else { WINDOW_EX_STYLE(0) } | if layered_flag { WS_EX_LAYERED } else { WINDOW_EX_STYLE(0) };
    let hinst = GetModuleHandleW(None)?.into();
    let hwnd = CreateWindowExW(ex, w!("PodShadowProof"), w!("pod-shadow-gate"), WS_POPUP, x, y, buf.w as i32, buf.h as i32, None, None, Some(hinst), None)?;
    if layered_flag {
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;
    }
    let surface = Surface::new(gfx, hwnd, buf.w as u32, buf.h as u32)?;
    paint(&surface, buf, None)?;
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    Ok(Item { hwnd, surface: Some(surface), live })
}

fn cpu_100ns() -> u64 {
    unsafe {
        let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        let _ = GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u);
        let v = |f: FILETIME| (f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64;
        v(k) + v(u)
    }
}

fn working_set() -> (usize, usize) {
    unsafe {
        let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let _ = GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb);
        (c.WorkingSetSize, c.PrivateUsage)
    }
}

#[derive(Default, Clone)]
struct Sample {
    ours_cpu_pct: f64,
    ours_ws_mib: f64,
    ours_private_mib: f64,
    vals: std::collections::BTreeMap<String, f64>,
}

/// Mede por GATE_SECONDS: espera o PowerShell (contadores de GPU e CPU do dwm) enquanto bombeia mensagens
/// e, se `redraw`, redesenha as janelas vivas uma vez por segundo.
unsafe fn measure(script: &str, dwm_pid: u32, items: &[Item], bufs: &dyn Fn(&Item) -> Option<&'static Buf>, redraw: bool) -> Sample {
    let mut child = std::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script, "-Seconds", &GATE_SECONDS.to_string(), "-OurPid", &std::process::id().to_string(), "-DwmPid", &dwm_pid.to_string()])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("powershell");
    let start = std::time::Instant::now();
    let cpu0 = cpu_100ns();
    let mut tick = 0u32;
    let mut last = std::time::Instant::now() - std::time::Duration::from_secs(2);
    loop {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if redraw && last.elapsed() >= std::time::Duration::from_secs(1) {
            last = std::time::Instant::now();
            tick += 1;
            for it in items.iter().filter(|i| i.live) {
                if let (Some(s), Some(b)) = (&it.surface, bufs(it)) {
                    let _ = paint(s, b, Some(tick));
                }
            }
        }
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    let wall = start.elapsed().as_secs_f64();
    let cpu = (cpu_100ns() - cpu0) as f64 / 1e7;
    let mut out = String::new();
    if let Some(mut so) = child.stdout.take() {
        use std::io::Read;
        let _ = so.read_to_string(&mut out);
    }
    let (ws, pm) = working_set();
    let mut s = Sample { ours_cpu_pct: 100.0 * cpu / wall, ours_ws_mib: ws as f64 / 1048576.0, ours_private_mib: pm as f64 / 1048576.0, ..Default::default() };
    for line in out.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if let Ok(x) = v.trim().replace(',', ".").parse::<f64>() {
                s.vals.insert(k.trim().to_string(), x);
            }
        }
    }
    s
}

fn dwm_pid() -> u32 {
    let o = std::process::Command::new("powershell").args(["-NoProfile", "-Command", "(Get-Process dwm | Select-Object -First 1).Id"]).output().ok();
    o.and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0)
}

/// `pod-app --shadow-gate <pasta> [script]`
pub fn run_gate(dir: &Path, script: &str) -> std::result::Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    unsafe { gate(dir, script).map_err(|e| e.to_string()) }
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Base,
    Cards,
    A,
    BDcomp,
    BLayered,
}

unsafe fn gate(dir: &Path, script: &str) -> Result<()> {
    register()?;
    let scale = GetDpiForSystem() as f32 / 96.0;
    let (full, shadow, card) = render(scale);
    // As janelas vivem ate o fim da funcao; as referencias 'static sao so para a closure de redesenho.
    let (full, shadow, card): (&'static Buf, &'static Buf, &'static Buf) = (Box::leak(Box::new(full)), Box::leak(Box::new(shadow)), Box::leak(Box::new(card)));
    let gfx = Gfx::new(false)?;
    let dwm = dwm_pid();
    let margin = (MARGIN_DIP * scale).round() as i32;
    let step = card.w as i32 + (GAP_DIP * scale).round() as i32;
    let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let (cols, rows) = (10, 4);
    let (x0, y0) = ((sw - cols * step) / 2, (sh - rows * step) / 2);
    let pos = |i: i32| (x0 + (i % cols) * step, y0 + (i / cols) * step);

    let phases = [
        (Phase::Base, "Sem janelas (linha de base)"),
        (Phase::Cards, "40 cartoes, sem sombra (hoje)"),
        (Phase::A, "A: 40 janelas unicas com margem (DirectComposition; reprovada em clique)"),
        (Phase::BDcomp, "B: 40 cartoes + 40 janelas de sombra, ambas em DirectComposition"),
        (Phase::BLayered, "B2: 40 cartoes em DirectComposition + 40 sombras em camadas (UpdateLayeredWindow)"),
    ];
    let mut rep = String::new();
    let _ = writeln!(rep, "# Portao de 40 janelas: variacao B (ADR-0005, etapa 3)\n");
    // --- clique atravessando a janela de sombra em DirectComposition (sem e com WS_EX_LAYERED) ---
    {
        let (wx, wy) = ((sw - full.w as i32) / 2, (sh - full.h as i32) / 2);
        let cardpx = card.w as i32;
        let (cx, cy) = (wx + margin + cardpx / 2, wy + margin + cardpx / 2);
        let probes = [
            ("centro do cartao", cx, cy),
            ("margem externa (alfa 0)", wx + 2, cy),
            ("margem com sombra (esquerda)", wx + margin - 6, cy),
            ("margem com sombra (abaixo)", cx, wy + margin + cardpx + 6),
        ];
        let log = dir.join("gate-clicks.log");
        let _ = std::fs::remove_file(&log);
        let exe = std::env::current_exe().map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()))?;
        let mut target = std::process::Command::new(exe)
            .args(["--shadow-proof-target", &(wx - 40).to_string(), &(wy - 40).to_string(), &(full.w as i32 + 80).to_string(), &(full.h as i32 + 80).to_string()])
            .env("POD_PROOF_LOG", &log)
            .spawn()
            .map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()))?;
        for _ in 0..40 {
            if FindWindowW(w!("PodShadowTarget"), None).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        pump();
        for (label, layered_flag) in [("janela de sombra em DirectComposition com `WS_EX_TRANSPARENT`", false), ("janela de sombra em DirectComposition com `WS_EX_LAYERED | WS_EX_TRANSPARENT`", true)] {
            let sh_w = dcomp_window_ex(&gfx, shadow, wx, wy, true, false, layered_flag)?;
            let cd_w = dcomp_window(&gfx, card, wx + margin, wy + margin, false, false)?;
            pump();
            click_table(&mut rep, label, &probes, &log);
            let _ = DestroyWindow(cd_w.hwnd);
            let _ = DestroyWindow(sh_w.hwnd);
            drop((cd_w, sh_w));
            pump();
        }
        let _ = target.kill();
    }
    let _ = writeln!(rep, "# Custo com 40 janelas\n");
    let _ = writeln!(rep, "Escala do sistema {scale:.2}; cartao {} px, janela com margem {} px; {N_WINDOWS} cartoes em grade 10x4, todos visiveis (TOPMOST). Cada medida dura {GATE_SECONDS} s, com 2 s de acomodacao antes; \"1 Hz\" redesenha cada cartao/janela viva uma vez por segundo (como um relogio); janelas de sombra nunca redesenham.\n", card.w, full.w);
    let _ = writeln!(rep, "| Fase | Janelas | Carga | CPU do Pod (% de 1 nucleo) | CPU do dwm (% de 1 nucleo) | GPU Pod (%) | GPU dwm (%) | WS Pod (MiB) | Privada Pod (MiB) | WS dwm (MiB) | PM dwm (MiB) | Mem. GPU dwm (MiB) | Mem. GPU Pod (MiB) |");
    let _ = writeln!(rep, "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    let mut rows_data: Vec<(String, usize, &str, Sample)> = Vec::new();
    for (phase, name) in phases {
        let mut items: Vec<Item> = Vec::new();
        match phase {
            Phase::Base => {}
            Phase::Cards => {
                for i in 0..N_WINDOWS as i32 {
                    let (x, y) = pos(i);
                    items.push(dcomp_window(&gfx, card, x, y, false, true)?);
                }
            }
            Phase::A => {
                for i in 0..N_WINDOWS as i32 {
                    let (x, y) = pos(i);
                    items.push(dcomp_window(&gfx, full, x - margin, y - margin, false, true)?);
                }
            }
            Phase::BDcomp | Phase::BLayered => {
                // Todas as sombras primeiro (ficam atras); depois os cartoes por cima.
                for i in 0..N_WINDOWS as i32 {
                    let (x, y) = pos(i);
                    if phase == Phase::BDcomp {
                        items.push(dcomp_window(&gfx, shadow, x - margin, y - margin, true, false)?);
                    } else {
                        let h = layered(shadow, x - margin, y - margin, true, 0)?;
                        items.push(Item { hwnd: h, surface: None, live: false });
                    }
                }
                for i in 0..N_WINDOWS as i32 {
                    let (x, y) = pos(i);
                    items.push(dcomp_window(&gfx, card, x, y, false, true)?);
                }
            }
        }
        let settle = std::time::Instant::now();
        while settle.elapsed() < std::time::Duration::from_secs(2) {
            pump();
        }
        let bufs = |it: &Item| -> Option<&'static Buf> {
            if !it.live {
                return None;
            }
            Some(if phase == Phase::A { full } else { card })
        };
        for (label, redraw) in [("ocioso", false), ("1 Hz", true)] {
            let smp = measure(script, dwm, &items, &bufs, redraw);
            rows_data.push((name.to_string(), items.len(), label, smp));
        }
        for it in &items {
            let _ = DestroyWindow(it.hwnd);
        }
        drop(items);
        pump();
    }
    let g = |s: &Sample, k: &str| s.vals.get(k).copied().unwrap_or(f64::NAN);
    for (name, n, load, s) in &rows_data {
        let _ = writeln!(
            rep,
            "| {name} | {n} | {load} | {:.2} | {:.2} | {:.2} | {:.2} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} |",
            s.ours_cpu_pct,
            g(s, "dwm_cpu_pct"),
            g(s, "ours_gpu_pct"),
            g(s, "dwm_gpu_pct"),
            s.ours_ws_mib,
            s.ours_private_mib,
            g(s, "dwm_ws_mib"),
            g(s, "dwm_pm_mib"),
            g(s, "dwm_gpu_mem_mib"),
            g(s, "ours_gpu_mem_mib")
        );
    }
    let _ = writeln!(rep, "\n`NaN` = o contador nao respondeu.");
    std::fs::write(dir.join("gate-report.md"), &rep).map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()))?;
    Ok(())
}

pub(crate) fn pump() {
    unsafe {
        let mut msg = MSG::default();
        let end = std::time::Instant::now() + std::time::Duration::from_millis(150);
        while std::time::Instant::now() < end {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_is_opaque_in_the_middle_and_the_shadow_fades_out_at_the_margin_edge() {
        let (full, shadow, card) = render(1.0);
        let mid = full.w / 2;
        assert_eq!(full.px[mid * full.w + mid] >> 24, 255, "centro do cartão opaco");
        assert_eq!(card.px[card.w / 2 * card.w + card.w / 2] >> 24, 255);
        assert_eq!(full.px[0] >> 24, 0, "canto da janela transparente");
        let max_shadow = shadow.px.iter().map(|p| p >> 24).max().unwrap();
        assert!(max_shadow as f32 <= SHADOW_ALPHA * 255.0 + 1.0, "{max_shadow}");
        // Na borda externa da margem a sombra já sumiu (não corta em degrau).
        assert!(shadow.px[mid] >> 24 <= 2, "{}", shadow.px[mid] >> 24);
    }

    #[test]
    fn png_has_signature_and_expected_chunks() {
        let dir = std::env::temp_dir().join("pod-shadow-png-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.png");
        write_png(&path, 2, 2, &[255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9]).unwrap();
        let b = std::fs::read(&path).unwrap();
        assert_eq!(&b[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert!(b.windows(4).any(|w| w == b"IHDR") && b.windows(4).any(|w| w == b"IEND"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
