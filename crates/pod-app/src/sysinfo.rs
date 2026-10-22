//! Preferências do sistema que o visual respeita: tema claro/escuro, alto contraste e redução de
//! animações. Só leitura; nada é alterado.

use std::ffi::c_void;

use windows::core::{w, BOOL};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Graphics::Gdi::{
    GetSysColor, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT,
    SYS_COLOR_INDEX,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

use crate::ui::theme::{Mode, Rgb, Theme};

/// `SPI_GETCLIENTAREAANIMATION`.
const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;

fn sys_color(i: SYS_COLOR_INDEX) -> Rgb {
    let c = unsafe { GetSysColor(i) }; // 0x00BBGGRR
    [(c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8]
}

/// O aplicativo deve usar o tema claro? (`AppsUseLightTheme`; claro se a chave não existir.)
pub fn apps_use_light_theme() -> bool {
    let mut data = 1u32;
    let mut len = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut len),
        )
    };
    rc != ERROR_SUCCESS || data != 0
}

pub fn high_contrast_on() -> bool {
    let mut hc = HIGHCONTRASTW { cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32, ..Default::default() };
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            Some(&mut hc as *mut _ as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_ok() && hc.dwFlags.0 & HCF_HIGHCONTRASTON.0 != 0
}

/// "Efeitos de transparência" do Windows (Configurações > Personalização > Cores). Ligado se a chave
/// não existir. Desligado, o vidro do Pod volta ao cartão opaco.
pub fn transparency_enabled() -> bool {
    let mut data = 1u32;
    let mut len = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("EnableTransparency"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut len),
        )
    };
    rc != ERROR_SUCCESS || data != 0
}

fn high_contrast_theme() -> Theme {
    Theme::high_contrast(
        sys_color(COLOR_WINDOW),
        sys_color(COLOR_WINDOWTEXT),
        sys_color(COLOR_GRAYTEXT),
        sys_color(COLOR_HIGHLIGHT),
        sys_color(COLOR_HIGHLIGHTTEXT),
    )
}

/// Tema pedido pelo sistema: alto contraste manda; senão claro/escuro do aplicativo.
pub fn system_theme() -> Theme {
    if high_contrast_on() {
        high_contrast_theme()
    } else if apps_use_light_theme() {
        Theme::light()
    } else {
        Theme::dark()
    }
}

/// Tema forçado (só para teste visual em builds de debug) ou o do sistema.
pub fn theme_for(forced: Option<Mode>, glass: bool) -> Theme {
    let base = base_theme_for(forced);
    if glass && transparency_enabled() {
        base.glass()
    } else {
        base
    }
}

fn base_theme_for(forced: Option<Mode>) -> Theme {
    match forced {
        Some(Mode::Light) => Theme::light(),
        Some(Mode::Dark) => Theme::dark(),
        Some(Mode::HighContrast) => high_contrast_theme(),
        None => system_theme(),
    }
}

/// O usuário desligou as animações do sistema ("Mostrar animações" nas configurações)?
pub fn reduce_motion() -> bool {
    let mut enabled = BOOL(1);
    let ok = unsafe {
        SystemParametersInfoW(
            windows::Win32::UI::WindowsAndMessaging::SYSTEM_PARAMETERS_INFO_ACTION(SPI_GETCLIENTAREAANIMATION),
            0,
            Some(&mut enabled as *mut BOOL as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_ok() && !enabled.as_bool()
}

/// Hora local do sistema, decomposta.
/// Hora agora no fuso do Windows `key` (ex.: "Tokyo Standard Time"), com horário de verão. `None` se
/// o Windows não conhece a chave.
pub fn zone_time(key: &str) -> Option<pod_model::worldclock::ZoneTime> {
    use windows::Win32::System::Time::{EnumDynamicTimeZoneInformation, SystemTimeToTzSpecificLocalTimeEx};
    use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    use windows::Win32::System::Time::DYNAMIC_TIME_ZONE_INFORMATION;
    unsafe {
        let mut found: Option<DYNAMIC_TIME_ZONE_INFORMATION> = None;
        for i in 0..512 {
            let mut z = DYNAMIC_TIME_ZONE_INFORMATION::default();
            if EnumDynamicTimeZoneInformation(i, &mut z) != 0 {
                break;
            }
            let len = z.TimeZoneKeyName.iter().position(|&c| c == 0).unwrap_or(z.TimeZoneKeyName.len());
            if String::from_utf16_lossy(&z.TimeZoneKeyName[..len]) == key {
                found = Some(z);
                break;
            }
        }
        let z = found?;
        let utc = GetSystemTime();
        let mut there = Default::default();
        SystemTimeToTzSpecificLocalTimeEx(Some(&z), &utc, &mut there).ok()?;
        let here = GetLocalTime();
        Some(pod_model::worldclock::ZoneTime {
            hour: there.wHour as u8,
            minute: there.wMinute as u8,
            day_diff: pod_model::worldclock::day_diff(
                (here.wYear as i32, here.wMonth as u8, here.wDay as u8),
                (there.wYear as i32, there.wMonth as u8, there.wDay as u8),
            ),
        })
    }
}

pub fn local_time() -> pod_model::time::TimeInfo {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    pod_model::time::TimeInfo {
        year: t.wYear as i32,
        month: t.wMonth as u8,
        day: t.wDay as u8,
        weekday: t.wDayOfWeek as u8,
        hour: t.wHour as u8,
        minute: t.wMinute as u8,
        second: t.wSecond as u8,
        millis: t.wMilliseconds as u16,
    }
}

#[cfg(test)]
mod zone_tests {
    #[test]
    fn every_catalog_city_resolves_on_this_windows_and_unknown_keys_do_not() {
        for (id, _, key) in pod_model::worldclock::CITIES {
            let z = super::zone_time(key).unwrap_or_else(|| panic!("{id}: fuso '{key}' desconhecido"));
            assert!(z.hour < 24 && z.minute < 60, "{id}");
        }
        assert!(super::zone_time("Atlantida Standard Time").is_none());
        // Tóquio (UTC+9, sem horário de verão) e Nova York não podem coincidir.
        let (tokyo, ny) = (super::zone_time("Tokyo Standard Time").unwrap(), super::zone_time("Eastern Standard Time").unwrap());
        assert_ne!((tokyo.hour, tokyo.minute), (ny.hour, ny.minute));
    }
}
