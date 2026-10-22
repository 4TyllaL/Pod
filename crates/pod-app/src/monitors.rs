//! Enumeração de monitores: geometria e DPI via `EnumDisplayMonitors`/`GetDpiForMonitor`, e
//! identidade via `QueryDisplayConfig` + `DisplayConfigGetDeviceInfo`, correlacionadas pelo nome
//! GDI (`\\.\DISPLAYn`). `HMONITOR`, índice e `DISPLAYn` não são identidade persistente.

use std::collections::HashMap;

use windows::core::BOOL;
use windows::Win32::Devices::Display::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

use pod_model::geometry::RectPx;

const MONITORINFOF_PRIMARY: u32 = 1;
/// Bit `edidIdsValid` (terceiro campo de 1 bit) de `DISPLAYCONFIG_TARGET_DEVICE_NAME_FLAGS`.
const EDID_IDS_VALID: u32 = 0x4;
use pod_model::placement::Monitor;

/// Identidade vinda do `DisplayConfig` para um nome GDI.
#[derive(Default, Clone)]
struct Identity {
    device_path: String,
    edid_id: Option<String>,
    name: String,
    connection: Option<String>,
}

pub fn enumerate() -> Vec<Monitor> {
    let identities = display_identities();
    let mut handles: Vec<HMONITOR> = Vec::new();
    unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
        let v = &mut *(lp.0 as *mut Vec<HMONITOR>);
        v.push(h);
        BOOL(1)
    }
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut handles as *mut _ as isize));
    }

    let mut out = Vec::new();
    for h in handles {
        let mut mi = MONITORINFOEXW::default();
        mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if !unsafe { GetMonitorInfoW(h, &mut mi as *mut _ as *mut MONITORINFO) }.as_bool() {
            continue;
        }
        let gdi_name = wide_to_string(&mi.szDevice);
        let (mut dx, mut dy) = (96u32, 96u32);
        let dpi = match unsafe { GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) } {
            Ok(()) if dx > 0 => dx,
            _ => 96,
        };
        let rc = |r: RECT| RectPx { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
        let id = identities.get(&gdi_name).cloned().unwrap_or_default();
        out.push(Monitor {
            // Sem identidade do DisplayConfig, o nome GDI ainda separa monitores nesta sessão, mas
            // não é gravado como se fosse permanente: fica só em `name`.
            device_path: id.device_path,
            edid_id: id.edid_id,
            name: if id.name.is_empty() { gdi_name } else { id.name },
            connection: id.connection,
            bounds: rc(mi.monitorInfo.rcMonitor),
            work: rc(mi.monitorInfo.rcWork),
            dpi,
            primary: mi.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    out
}

fn wide_to_string(w: &[u16]) -> String {
    let n = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..n])
}

/// Mapeia nome GDI do monitor → identidade. Falhas deixam o mapa vazio (a enumeração continua).
fn display_identities() -> HashMap<String, Identity> {
    let mut map = HashMap::new();
    unsafe {
        // A configuração pode mudar entre pedir o tamanho e ler: tenta algumas vezes.
        for _ in 0..3 {
            let (mut n_paths, mut n_modes) = (0u32, 0u32);
            if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut n_paths, &mut n_modes) != ERROR_SUCCESS {
                return map;
            }
            let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); n_paths as usize];
            let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); n_modes as usize];
            let rc = QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut n_paths,
                paths.as_mut_ptr(),
                &mut n_modes,
                modes.as_mut_ptr(),
                None,
            );
            if rc == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            if rc != ERROR_SUCCESS {
                return map;
            }
            for p in paths.iter().take(n_paths as usize) {
                let mut src = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
                src.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
                src.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
                src.header.adapterId = p.sourceInfo.adapterId;
                src.header.id = p.sourceInfo.id;
                if DisplayConfigGetDeviceInfo(&mut src.header) != 0 {
                    continue;
                }
                let gdi = wide_to_string(&src.viewGdiDeviceName);

                let mut tgt = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
                tgt.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
                tgt.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
                tgt.header.adapterId = p.targetInfo.adapterId;
                tgt.header.id = p.targetInfo.id;
                if DisplayConfigGetDeviceInfo(&mut tgt.header) != 0 {
                    continue;
                }
                // `edidIdsValid` é o bit 0x2 dos flags.
                let edid_valid = tgt.flags.Anonymous.value & EDID_IDS_VALID != 0;
                map.insert(
                    gdi,
                    Identity {
                        device_path: wide_to_string(&tgt.monitorDevicePath),
                        edid_id: edid_valid.then(|| format!("{:04X}:{:04X}", tgt.edidManufactureId, tgt.edidProductCodeId)),
                        name: wide_to_string(&tgt.monitorFriendlyDeviceName),
                        connection: Some(format!("{}", p.targetInfo.outputTechnology.0)),
                    },
                );
            }
            return map;
        }
    }
    map
}
