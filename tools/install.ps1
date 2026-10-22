<#
.SYNOPSIS
  Instala (ou atualiza) o Pod para o usuário atual, sem privilégios de administrador.

.DESCRIPTION
  Copia Pod.exe para %LOCALAPPDATA%\Programs\Pod, cria o atalho no Menu Iniciar e registra a entrada
  de desinstalação do usuário (aparece em Configurações > Aplicativos). NÃO toca nas notas: os dados
  ficam em %LOCALAPPDATA%\Pod e sobrevivem a atualização e desinstalação.

  Início com o Windows só é configurado com -Autostart (consentimento explícito); sem o parâmetro, uma
  escolha anterior é mantida e nada novo é criado. -NoAutostart remove.

  Atualização: com o Pod em execução, o script se recusa a continuar, a menos que -CloseRunning seja
  usado: nesse caso envia o pedido padrão de fechar a janela de controle do Pod (com snapshot final) e
  espera até 20 s. Nunca encerra o processo à força.

  Os parâmetros de caminho existem para testes em pasta isolada.

.EXAMPLE
  .\install.ps1 -Launch
.EXAMPLE
  .\install.ps1 -CloseRunning -Autostart
#>
[CmdletBinding()]
param(
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Pod"),
  [string]$StartMenuDir = [Environment]::GetFolderPath("Programs"),
  [string]$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Pod",
  [string]$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run",
  [switch]$Autostart,
  [switch]$NoAutostart,
  [switch]$CloseRunning,
  [switch]$NoStartMenu,
  [switch]$Launch
)
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = Join-Path $here "Pod.exe"
if (-not (Test-Path $src)) { throw "Pod.exe não encontrado ao lado do instalador ($here)." }
if ($Autostart -and $NoAutostart) { throw "Use -Autostart ou -NoAutostart, não os dois." }

Add-Type -TypeDefinition @'
using System;using System.Text;using System.Runtime.InteropServices;
public class PodInstallWin {
  public delegate bool EP(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EP cb, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  // Pede ao Pod (pela janela de controle oculta) que encerre normalmente: WM_CLOSE.
  public static int RequestClose(uint pid) {
    int n = 0;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(64); GetClassNameW(h, sb, 64);
      if (p == pid && sb.ToString() == "PodControl") { PostMessage(h, 0x10, IntPtr.Zero, IntPtr.Zero); n++; } return true; }, IntPtr.Zero);
    return n;
  }
}
'@

function Get-RunningPod([string]$exe) {
  Get-Process -Name Pod -ErrorAction SilentlyContinue | Where-Object { $_.Path -and ($_.Path -ieq $exe) }
}

$dstExe = Join-Path $InstallDir "Pod.exe"
$restartArgs = ""
$wasRunning = @(Get-RunningPod $dstExe)
if ($wasRunning.Count -gt 0) {
  if (-not $CloseRunning) {
    throw "O Pod está em execução ($($wasRunning.Count) processo). Feche-o pela bandeja (Sair) ou rode com -CloseRunning. Nada foi alterado."
  }
  # Guarda os argumentos com que o Pod estava rodando (por exemplo --data-dir) para reiniciar igual.
  $restartArgs = ""
  $cmdLine = (Get-CimInstance Win32_Process -Filter "ProcessId=$($wasRunning[0].Id)" -ErrorAction SilentlyContinue).CommandLine
  if ($cmdLine) {
    $m = [regex]::Match($cmdLine, '^\s*(?:"[^"]*"|\S+)\s*(.*)$')
    if ($m.Success) { $restartArgs = $m.Groups[1].Value.Trim() }
  }
  foreach ($p in $wasRunning) { [void][PodInstallWin]::RequestClose([uint32]$p.Id) }
  $deadline = (Get-Date).AddSeconds(20)
  while ((Get-Date) -lt $deadline -and @(Get-RunningPod $dstExe).Count -gt 0) { Start-Sleep -Milliseconds 200 }
  if (@(Get-RunningPod $dstExe).Count -gt 0) { throw "O Pod não encerrou em 20 s. Feche-o pela bandeja e rode de novo. Nada foi alterado." }
  Write-Host "Pod encerrado normalmente (snapshot final gravado)."
}

New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
# Substituição atômica do executável: copia para um temporário no mesmo diretório e troca.
$tmp = Join-Path $InstallDir "Pod.exe.new"
Copy-Item $src $tmp -Force
Move-Item $tmp $dstExe -Force
foreach ($f in "uninstall.ps1", "README.txt", "THIRD-PARTY-LICENSES.txt") {
  $p = Join-Path $here $f
  if (Test-Path $p) { Copy-Item $p (Join-Path $InstallDir $f) -Force }
}
$version = (Get-Item $dstExe).VersionInfo.ProductVersion

if (-not $NoStartMenu) {
  New-Item -ItemType Directory -Path $StartMenuDir -Force | Out-Null
  $sh = New-Object -ComObject WScript.Shell
  $lnk = $sh.CreateShortcut((Join-Path $StartMenuDir "Pod.lnk"))
  $lnk.TargetPath = $dstExe; $lnk.WorkingDirectory = $InstallDir; $lnk.Description = "Pod: widgets para o desktop"
  $lnk.IconLocation = "$dstExe,0"; $lnk.Save()
}

# Entrada de desinstalação do usuário (os parâmetros de caminho vão junto, para o desinstalador achar tudo).
$ps = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$(Join-Path $InstallDir 'uninstall.ps1')`" -InstallDir `"$InstallDir`" -StartMenuDir `"$StartMenuDir`" -UninstallKey `"$UninstallKey`" -RunKey `"$RunKey`" -CloseRunning"
New-Item -Path $UninstallKey -Force | Out-Null
$sizeKb = [int]((Get-Item $dstExe).Length / 1KB)
$entries = @{
  DisplayName = "Pod"; DisplayVersion = $version; Publisher = "Pod"; InstallLocation = $InstallDir
  DisplayIcon = "$dstExe,0"; UninstallString = $ps; QuietUninstallString = $ps
}
foreach ($k in $entries.Keys) { Set-ItemProperty -Path $UninstallKey -Name $k -Value $entries[$k] }
Set-ItemProperty -Path $UninstallKey -Name NoModify -Value 1 -Type DWord
Set-ItemProperty -Path $UninstallKey -Name NoRepair -Value 1 -Type DWord
Set-ItemProperty -Path $UninstallKey -Name EstimatedSize -Value $sizeKb -Type DWord

# Início com o Windows: só com consentimento explícito.
if (-not (Test-Path $RunKey)) { New-Item -Path $RunKey -Force | Out-Null }
if ($Autostart) { Set-ItemProperty -Path $RunKey -Name "Pod" -Value "`"$dstExe`"" }
elseif ($NoAutostart) { Remove-ItemProperty -Path $RunKey -Name "Pod" -ErrorAction SilentlyContinue }

Write-Host "Pod $version instalado em $InstallDir"
Write-Host "As notas ficam em %LOCALAPPDATA%\Pod e não foram tocadas."
if ($Launch -or $wasRunning.Count -gt 0) {
  if ($restartArgs) { Start-Process $dstExe -ArgumentList $restartArgs } else { Start-Process $dstExe }
  Write-Host "Pod iniciado."
}
