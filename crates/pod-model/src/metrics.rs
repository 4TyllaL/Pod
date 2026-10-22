//! Métricas do sistema (serviço `SystemMetrics`, ADR-0001): tipos, cálculo de uso de CPU a partir
//! de dois instantes e formatação em pt-BR. Puro: o app lê o Windows e entrega um `Sample`.

/// O que um widget quer receber. O provedor só lê o que alguém pediu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetricsNeeds {
    pub cpu: bool,
    pub memory: bool,
    pub disk: bool,
    pub battery: bool,
    pub network: bool,
}

impl MetricsNeeds {
    pub const CPU: Self = Self { cpu: true, memory: false, disk: false, battery: false, network: false };
    pub const MEMORY: Self = Self { cpu: false, memory: true, disk: false, battery: false, network: false };
    pub const DISK: Self = Self { cpu: false, memory: false, disk: true, battery: false, network: false };
    pub const NETWORK: Self = Self { cpu: false, memory: false, disk: false, battery: false, network: true };
    pub const BATTERY: Self = Self { cpu: false, memory: false, disk: false, battery: true, network: false };

    pub fn is_empty(self) -> bool {
        self == Self::default()
    }

    pub fn union(self, o: Self) -> Self {
        Self {
            cpu: self.cpu || o.cpu,
            memory: self.memory || o.memory,
            disk: self.disk || o.disk,
            battery: self.battery || o.battery,
            network: self.network || o.network,
        }
    }

    /// Intervalo entre leituras: 2 s se algo muda rápido (CPU, memória), 30 s se só disco/bateria.
    pub fn interval_ms(self) -> u64 {
        if self.cpu || self.memory || self.network {
            2_000
        } else {
            30_000
        }
    }
}

/// Contadores cumulativos de `GetSystemTimes` (unidades de 100 ns). `kernel` inclui `idle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub idle: u64,
    pub kernel: u64,
    pub user: u64,
}

/// Uso total de CPU entre duas leituras (0..=1). `None` se o intervalo é vazio ou o contador voltou.
pub fn cpu_usage(prev: CpuTimes, now: CpuTimes) -> Option<f32> {
    let total = now.kernel.checked_sub(prev.kernel)?.checked_add(now.user.checked_sub(prev.user)?)?;
    let idle = now.idle.checked_sub(prev.idle)?;
    if total == 0 {
        return None;
    }
    Some((1.0 - idle.min(total) as f64 / total as f64).clamp(0.0, 1.0) as f32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    Battery,
    /// Na tomada e carregando.
    Charging,
    /// Na tomada, sem carregar (cheia ou desktop).
    PluggedIn,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Battery {
    /// `None` quando não há bateria (desktop).
    pub percent: Option<u8>,
    pub source: PowerSource,
    pub minutes_left: Option<u32>,
    pub saver: bool,
}

impl Battery {
    /// Interpreta os campos de `SYSTEM_POWER_STATUS`: `ac` (0 fora, 1 na tomada, 255 ?), `flag`
    /// (bit 8 carregando, bit 128 sem bateria, 255 ?), `percent` (0..=100, 255 ?), `secs`
    /// (restantes, `u32::MAX` ?) e `saver` (economia de energia ligada).
    pub fn from_status(ac: u8, flag: u8, percent: u8, secs: u32, saver: bool) -> Self {
        let none = flag == 128;
        let charging = flag != 255 && flag & 8 != 0;
        let source = if charging {
            PowerSource::Charging
        } else if ac == 1 {
            PowerSource::PluggedIn
        } else {
            PowerSource::Battery
        };
        Self {
            percent: (!none && percent <= 100).then_some(percent),
            source: if none && ac != 0 { PowerSource::PluggedIn } else { source },
            minutes_left: (!none && source == PowerSource::Battery && secs != u32::MAX).then_some(secs / 60),
            saver,
        }
    }
}

/// Contadores cumulativos de bytes das interfaces ativas (sem loopback).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetCounters {
    pub rx: u64,
    pub tx: u64,
}

/// Velocidade em bytes por segundo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetRate {
    pub down: f32,
    pub up: f32,
}

/// Taxa entre duas leituras separadas por `elapsed_ms`. `None` se o intervalo é vazio ou um
/// contador voltou (interface removida ou reiniciada): melhor sem ponto do que um pico falso.
pub fn net_rate(prev: NetCounters, now: NetCounters, elapsed_ms: u64) -> Option<NetRate> {
    if elapsed_ms == 0 {
        return None;
    }
    let secs = elapsed_ms as f64 / 1000.0;
    let rx = now.rx.checked_sub(prev.rx)?;
    let tx = now.tx.checked_sub(prev.tx)?;
    Some(NetRate { down: (rx as f64 / secs) as f32, up: (tx as f64 / secs) as f32 })
}

/// "1,2 MB/s", "340 KB/s", "0 KB/s".
pub fn format_rate(bytes_per_sec: f32) -> String {
    format!("{}/s", format_bytes(bytes_per_sec.max(0.0) as u64))
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sample {
    pub net: Option<NetRate>,
    pub cpu: Option<f32>,
    /// (usada, total) em bytes.
    pub memory: Option<(u64, u64)>,
    /// (usado, total) em bytes do disco do sistema.
    pub disk: Option<(u64, u64)>,
    pub battery: Option<Battery>,
}

/// "8,4 GB", "512 MB", "1,2 TB" (vírgula decimal; 1024 por degrau; uma casa a partir de GB).
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    let (v, unit) = if b >= KB * KB * KB * KB {
        (b / (KB * KB * KB * KB), "TB")
    } else if b >= KB * KB * KB {
        (b / (KB * KB * KB), "GB")
    } else if b >= KB * KB {
        return format!("{} MB", (b / (KB * KB)).round() as u64);
    } else {
        return format!("{} KB", (b / KB).round() as u64);
    };
    format!("{v:.1} {unit}").replace('.', ",")
}

/// "2 h 10 min", "45 min".
pub fn format_minutes(total: u32) -> String {
    match (total / 60, total % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m:02} min"),
    }
}

/// Percentual inteiro sem arredondar para cima (99,9% ainda é 99%).
pub fn percent(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * 100.0).floor() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(idle: u64, kernel: u64, user: u64) -> CpuTimes {
        CpuTimes { idle, kernel, user }
    }

    #[test]
    fn cpu_usage_from_two_readings() {
        // Em 1000 unidades (kernel+user): 750 ociosas -> 25% de uso.
        assert_eq!(cpu_usage(t(0, 0, 0), t(750, 800, 200)), Some(0.25));
        // Tudo ocioso e tudo ocupado.
        assert_eq!(cpu_usage(t(0, 0, 0), t(1000, 1000, 0)), Some(0.0));
        assert_eq!(cpu_usage(t(0, 0, 0), t(0, 500, 500)), Some(1.0));
        // Deltas, não valores absolutos.
        assert_eq!(cpu_usage(t(5000, 6000, 4000), t(5750, 6800, 4200)), Some(0.25));
    }

    #[test]
    fn cpu_usage_rejects_empty_or_reversed_intervals() {
        assert_eq!(cpu_usage(t(1, 2, 3), t(1, 2, 3)), None);
        assert_eq!(cpu_usage(t(10, 20, 30), t(5, 25, 35)), None, "contador voltou");
        // Ociosidade maior que o total (arredondamento do sistema) não passa de 100% nem fica negativa.
        assert_eq!(cpu_usage(t(0, 0, 0), t(120, 100, 0)), Some(0.0));
    }

    #[test]
    fn needs_union_and_interval() {
        assert!(MetricsNeeds::default().is_empty());
        let both = MetricsNeeds::CPU.union(MetricsNeeds::DISK);
        assert!(both.cpu && both.disk && !both.memory && !both.battery);
        assert_eq!(both.interval_ms(), 2_000);
        assert_eq!(MetricsNeeds::DISK.union(MetricsNeeds::BATTERY).interval_ms(), 30_000);
        assert_eq!(MetricsNeeds::MEMORY.interval_ms(), 2_000);
    }

    #[test]
    fn bytes_use_a_decimal_comma() {
        assert_eq!(format_bytes(0), "0 KB");
        assert_eq!(format_bytes(512 * 1024), "512 KB");
        assert_eq!(format_bytes(300 * 1024 * 1024), "300 MB");
        assert_eq!(format_bytes(8_589_934_592 + 429_496_730), "8,4 GB");
        assert_eq!(format_bytes(16 * 1024 * 1024 * 1024), "16,0 GB");
        assert_eq!(format_bytes(1_319_413_953_331), "1,2 TB");
    }

    #[test]
    fn battery_status_mapping() {
        let on_battery = Battery::from_status(0, 1, 63, 7_800, false);
        assert_eq!((on_battery.percent, on_battery.source, on_battery.minutes_left), (Some(63), PowerSource::Battery, Some(130)));
        let charging = Battery::from_status(1, 9, 40, u32::MAX, false);
        assert_eq!((charging.source, charging.minutes_left), (PowerSource::Charging, None));
        let full = Battery::from_status(1, 1, 100, u32::MAX, false);
        assert_eq!(full.source, PowerSource::PluggedIn);
        let desktop = Battery::from_status(1, 128, 255, u32::MAX, false);
        assert_eq!((desktop.percent, desktop.source), (None, PowerSource::PluggedIn));
        let unknown = Battery::from_status(255, 255, 255, u32::MAX, true);
        assert_eq!((unknown.percent, unknown.minutes_left, unknown.saver), (None, None, true));
    }

    #[test]
    fn net_rate_from_counters() {
        let c = |rx, tx| NetCounters { rx, tx };
        let r = net_rate(c(1_000, 500), c(3_048, 500), 2_000).unwrap();
        assert_eq!((r.down, r.up), (1_024.0, 0.0));
        assert_eq!(net_rate(c(5, 5), c(4, 9), 2_000), None, "contador voltou");
        assert_eq!(net_rate(c(1, 1), c(2, 2), 0), None);
        assert_eq!(format_rate(1_258_291.0), "1 MB/s");
        assert_eq!(format_rate(340.0 * 1024.0), "340 KB/s");
        assert!(MetricsNeeds::NETWORK.union(MetricsNeeds::DISK).network);
        assert_eq!(MetricsNeeds::NETWORK.interval_ms(), 2_000);
    }

    #[test]
    fn minutes_and_percent() {
        assert_eq!(format_minutes(45), "45 min");
        assert_eq!(format_minutes(130), "2 h 10 min");
        assert_eq!(format_minutes(120), "2 h");
        assert_eq!(percent(0.999), 99);
        assert_eq!(percent(1.5), 100);
        assert_eq!(percent(-1.0), 0);
    }
}
