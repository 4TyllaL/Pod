<#
.SYNOPSIS
  Desinstala o Pod do usuário atual. As notas são PRESERVADAS, a menos que -RemoveData seja usado.

.DESCRIPTION
  Remove o executável, o atalho do Menu Iniciar, o início com o Windows e a entrada de desinstalação.
  Os dados (%LOCALAPPDATA%\Pod) só são apagados com -RemoveData, que é uma escolha explícita e
  irreversível. Com o Pod em execução, use -CloseRunning (fecha normalmente e espera até 20 s).
#>
[CmdletBinding()]
param(
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Pod"),
  [string]$StartMenuDir = [Environment]::GetFolderPath("Programs"),
  [string]$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Pod",
  [string]$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run",
  [string]$DataDir = (Join-Path $env:LOCALAPPDATA "Pod"),
  [switch]$RemoveData,
  [switch]$CloseRunning
)
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @'
using System;using System.Text;using System.Runtime.InteropServices;
public class PodUninstallWin {
  public delegate bool EP(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EP cb, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static int RequestClose(uint pid) {
    int n = 0;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(64); GetClassNameW(h, sb, 64);
      if (p == pid && sb.ToString() == "PodControl") { PostMessage(h, 0x10, IntPtr.Zero, IntPtr.Zero); n++; } return true; }, IntPtr.Zero);
    return n;
  }
}
'@

function Get-RunningPod { Get-Process -Name Pod -ErrorAction SilentlyContinue | Where-Object { $_.Path -and ($_.Path -ieq (Join-Path $InstallDir "Pod.exe")) } }
$running = @(Get-RunningPod)
if ($running.Count -gt 0) {
  if (-not $CloseRunning) { throw "O Pod está em execução. Feche-o pela bandeja (Sair) ou use -CloseRunning. Nada foi alterado." }
  foreach ($p in $running) { [void][PodUninstallWin]::RequestClose([uint32]$p.Id) }
  $deadline = (Get-Date).AddSeconds(20)
  while ((Get-Date) -lt $deadline -and @(Get-RunningPod).Count -gt 0) { Start-Sleep -Milliseconds 200 }
  if (@(Get-RunningPod).Count -gt 0) { throw "O Pod não encerrou em 20 s. Feche-o pela bandeja e tente de novo. Nada foi alterado." }
}

# Dados: decide antes de mexer em qualquer coisa, para falhar sem desinstalar pela metade.
if ($RemoveData) {
  if ((Split-Path $DataDir -Leaf) -ne "Pod" -and -not $env:POD_UNINSTALL_ALLOW_ANY_DATADIR) {
    throw "Por segurança, -RemoveData só apaga uma pasta chamada 'Pod' (recebido: $DataDir)."
  }
}

Remove-ItemProperty -Path $RunKey -Name "Pod" -ErrorAction SilentlyContinue
Remove-Item (Join-Path $StartMenuDir "Pod.lnk") -Force -ErrorAction SilentlyContinue
Remove-Item -Path $UninstallKey -Recurse -Force -ErrorAction SilentlyContinue
foreach ($f in "Pod.exe", "Pod.exe.new", "README.txt", "THIRD-PARTY-LICENSES.txt") { Remove-Item (Join-Path $InstallDir $f) -Force -ErrorAction SilentlyContinue }
Write-Host "Pod removido (executável, atalho, início com o Windows e entrada de desinstalação)."

if ($RemoveData) {
  if (Test-Path $DataDir) { Remove-Item $DataDir -Recurse -Force; Write-Host "Dados APAGADOS: $DataDir" }
  else { Write-Host "Nenhuma pasta de dados em $DataDir." }
} else {
  Write-Host "Suas notas foram PRESERVADAS em $DataDir (use -RemoveData para apagá-las)."
}

# O desinstalador está dentro da pasta de instalação: remove-se por último e a pasta só se ficar vazia.
$self = $MyInvocation.MyCommand.Path
if ($self -and ((Split-Path $self -Parent) -ieq $InstallDir)) {
  $cleanup = "Start-Sleep -Seconds 2; Remove-Item -LiteralPath '$self' -Force -ErrorAction SilentlyContinue; if (-not (Get-ChildItem -LiteralPath '$InstallDir' -Force -ErrorAction SilentlyContinue)) { Remove-Item -LiteralPath '$InstallDir' -Force -ErrorAction SilentlyContinue }"
  Start-Process powershell.exe -WindowStyle Hidden -ArgumentList "-NoProfile", "-Command", $cleanup
} else {
  Remove-Item (Join-Path $InstallDir "uninstall.ps1") -Force -ErrorAction SilentlyContinue
  if (-not (Get-ChildItem $InstallDir -Force -ErrorAction SilentlyContinue)) { Remove-Item $InstallDir -Force -ErrorAction SilentlyContinue }
}
