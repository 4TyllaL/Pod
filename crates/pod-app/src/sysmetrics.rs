//! Provedor `SystemMetrics`: lê o Windows só para o que algum assinante pediu. Cada leitura é
//! contada (`reads`) para provar nos testes e na medição que, sem assinante, nada é lido.

use std::sync::atomic::{AtomicU64, Ordering};

use windows::core::PCWSTR;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;

use pod_model::metrics::{net_rate, Battery, CpuTimes, MetricsNeeds, NetCounters, Sample};

static READS: [AtomicU64; 5] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];

/// Leituras feitas desde o início: (cpu, memória, disco, bateria).
pub fn reads() -> [u64; 5] {
    [0, 1, 2, 3, 4].map(|i| READS[i].load(Ordering::Relaxed))
}

fn count(i: usize) {
    READS[i].fetch_add(1, Ordering::Relaxed);
}

fn ticks(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

fn read_cpu_times() -> Option<CpuTimes> {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
    Some(CpuTimes { idle: ticks(idle), kernel: ticks(kernel), user: ticks(user) })
}

fn read_memory() -> Option<(u64, u64)> {
    let mut st = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe { GlobalMemoryStatusEx(&mut st) }.ok()?;
    Some((st.ullTotalPhys.saturating_sub(st.ullAvailPhys), st.ullTotalPhys))
}

fn read_disk() -> Option<(u64, u64)> {
    let root = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let wide: Vec<u16> = format!("{root}\\").encode_utf16().chain(std::iter::once(0)).collect();
    let (mut total, mut free) = (0u64, 0u64);
    unsafe { GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), None, Some(&mut total), Some(&mut free)) }.ok()?;
    (total > 0).then(|| (total.saturating_sub(free), total))
}

fn read_battery() -> Option<Battery> {
    let mut st = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut st) }.ok()?;
    Some(Battery::from_status(st.ACLineStatus, st.BatteryFlag, st.BatteryLifePercent, st.BatteryLifeTime, st.SystemStatusFlag == 1))
}

/// Soma de bytes das interfaces ativas que não são loopback (tipo 24) nem túnel de software (131).
fn read_net_counters() -> Option<NetCounters> {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    unsafe { GetIfTable2(&mut table) }.ok().ok()?;
    if table.is_null() {
        return None;
    }
    let mut total = NetCounters { rx: 0, tx: 0 };
    unsafe {
        let n = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
        for r in rows {
            if r.OperStatus == IfOperStatusUp && r.Type != 24 && r.Type != 131 {
                total.rx = total.rx.saturating_add(r.InOctets);
                total.tx = total.tx.saturating_add(r.OutOctets);
            }
        }
        FreeMibTable(table as *const _);
    }
    Some(total)
}

/// Estado do provedor: a CPU precisa do instante anterior para calcular o uso.
#[derive(Default)]
pub struct Provider {
    prev_cpu: Option<CpuTimes>,
    prev_net: Option<(NetCounters, std::time::Instant)>,
    /// Leituras deste provedor (cpu, memória, disco, bateria, rede): o contador global soma todos.
    local: [u64; 5],
}

impl Provider {
    /// Marca o ponto de partida do uso de CPU (ao começar a ter assinante).
    pub fn baseline(&mut self) {
        self.bump(0);
        self.prev_cpu = read_cpu_times();
    }

    fn bump(&mut self, i: usize) {
        self.local[i] += 1;
        count(i);
    }

    pub fn local_reads(&self) -> [u64; 5] {
        self.local
    }

    /// Esquece o ponto de partida (sem assinantes de CPU): o próximo intervalo não incluiria o
    /// tempo em que ninguém olhava.
    pub fn forget_cpu(&mut self) {
        self.prev_cpu = None;
        self.prev_net = None;
    }

    pub fn has_baseline(&self) -> bool {
        self.prev_cpu.is_some()
    }

    pub fn read(&mut self, needs: MetricsNeeds) -> Sample {
        let mut s = Sample::default();
        if needs.cpu {
            self.bump(0);
            let now = read_cpu_times();
            if let (Some(p), Some(n)) = (self.prev_cpu, now) {
                s.cpu = pod_model::metrics::cpu_usage(p, n);
            }
            self.prev_cpu = now;
        }
        if needs.memory {
            self.bump(1);
            s.memory = read_memory();
        }
        if needs.disk {
            self.bump(2);
            s.disk = read_disk();
        }
        if needs.battery {
            self.bump(3);
            s.battery = read_battery();
        }
        if needs.network {
            self.bump(4);
            let now = read_net_counters().map(|c| (c, std::time::Instant::now()));
            if let (Some((pc, pt)), Some((nc, nt))) = (self.prev_net, now) {
                s.net = net_rate(pc, nc, nt.duration_since(pt).as_millis() as u64);
            }
            self.prev_net = now;
        } else {
            self.prev_net = None;
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_what_is_needed_and_counts_it() {
        let mut p = Provider::default();
        let s = p.read(MetricsNeeds::default());
        assert_eq!(s, Sample::default());
        assert_eq!(p.local_reads(), [0; 5], "sem pedido, nenhuma chamada ao Windows");

        let s = p.read(MetricsNeeds::MEMORY);
        let (used, total) = s.memory.expect("memória");
        assert!(total > 0 && used <= total);
        assert!(s.cpu.is_none() && s.disk.is_none() && s.battery.is_none());
        assert_eq!(p.local_reads(), [0, 1, 0, 0, 0]);
        assert!(reads()[1] >= 1, "o contador global também soma");
    }

    #[test]
    fn disk_reports_the_system_drive() {
        let (used, total) = Provider::default().read(MetricsNeeds::DISK).disk.expect("disco");
        assert!(total > 0 && used <= total);
    }

    #[test]
    fn cpu_needs_two_readings_and_forgetting_resets() {
        let mut p = Provider::default();
        assert_eq!(p.read(MetricsNeeds::CPU).cpu, None, "primeira leitura só marca o ponto de partida");
        assert!(p.has_baseline());
        std::thread::sleep(std::time::Duration::from_millis(60));
        if let Some(v) = p.read(MetricsNeeds::CPU).cpu {
            assert!((0.0..=1.0).contains(&v));
        }
        p.forget_cpu();
        assert!(!p.has_baseline());
    }
}
