param([int]$Seconds = 6, [int]$OurPid, [int]$DwmPid)
# Amostra GPU (por processo) e CPU do dwm.exe durante $Seconds segundos; escreve linhas chave=valor.
# Usado por `pod-app --shadow-gate` (ADR-0005, etapa 3). Nao altera nada no sistema.
# CPU do dwm vem de WMI (nomes de classe fixos); GPU vem de "GPU Engine" (nomes que nao sao localizados).
$gpuOurs = 0.0; $gpuDwm = 0.0; $cpuDwm = 0.0; $n = 0
for ($i = 0; $i -lt $Seconds; $i++) {
  $t0 = Get-Date
  try {
    $g = Get-Counter '\GPU Engine(*)\Utilization Percentage' -ErrorAction Stop
    $o = 0.0; $d = 0.0
    foreach ($c in $g.CounterSamples) {
      if ($c.InstanceName -like "pid_${OurPid}_*") { $o += $c.CookedValue }
      elseif ($c.InstanceName -like "pid_${DwmPid}_*") { $d += $c.CookedValue }
    }
    $gpuOurs += $o; $gpuDwm += $d
  } catch { }
  try {
    $p = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$DwmPid" -ErrorAction Stop
    $cpuDwm += [double]$p.PercentProcessorTime
  } catch { }
  $n++
  $left = 1000 - ((Get-Date) - $t0).TotalMilliseconds
  if ($left -gt 0) { Start-Sleep -Milliseconds ([int]$left) }
}
function GpuMem([int]$p) {
  try {
    $m = Get-Counter "\GPU Process Memory(pid_${p}_*)\Dedicated Usage" -ErrorAction Stop
    [math]::Round((($m.CounterSamples | Measure-Object CookedValue -Sum).Sum) / 1MB, 1)
  } catch { -1 }
}
$d = Get-Process -Id $DwmPid
"dwm_cpu_pct=" + [math]::Round($cpuDwm / $n, 3)
"dwm_gpu_pct=" + [math]::Round($gpuDwm / $n, 3)
"ours_gpu_pct=" + [math]::Round($gpuOurs / $n, 3)
"dwm_ws_mib=" + [math]::Round($d.WorkingSet64 / 1MB, 1)
"dwm_pm_mib=" + [math]::Round($d.PrivateMemorySize64 / 1MB, 1)
"dwm_gpu_mem_mib=" + (GpuMem $DwmPid)
"ours_gpu_mem_mib=" + (GpuMem $OurPid)
