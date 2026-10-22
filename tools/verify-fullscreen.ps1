param([Parameter(Mandatory = $true)][string]$Exe, [Parameter(Mandatory = $true)][string]$Data)
# Abre o Pod com dados isolados e uma janela sem borda que cobre o monitor (como um jogo em tela cheia sem borda);
# confere que os cartoes somem enquanto ela esta em primeiro plano e voltam quando ela fecha. Nao move o cursor.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public static class F {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  delegate bool P(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(P p, IntPtr l);
  public static int Visible(uint pid) { int n = 0; EnumWindows((h, _) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(64); GetClassName(h, sb, 64); if (p == pid && sb.ToString() == "PodWidget" && IsWindowVisible(h)) n++; return true; }, IntPtr.Zero); return n; }
}
'@
if (Test-Path $Data) { Remove-Item $Data -Recurse -Force }
New-Item -ItemType Directory $Data | Out-Null
$p = Start-Process -FilePath $Exe -ArgumentList @('--data-dir', $Data) -PassThru
try {
  for ($i = 0; $i -lt 60 -and [F]::Visible([uint32]$p.Id) -lt 3; $i++) { Start-Sleep -Milliseconds 250 }
  Start-Sleep -Seconds 1
  Write-Output ("cartoes visiveis antes: " + [F]::Visible([uint32]$p.Id))
  $f = New-Object System.Windows.Forms.Form
  $f.FormBorderStyle = 'None'; $f.StartPosition = 'Manual'
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $f.Bounds = $b; $f.TopMost = $false; $f.BackColor = 'Black'
  $f.Show(); $f.Activate()
  $t = [Diagnostics.Stopwatch]::StartNew(); $hidden = -1
  while ($t.Elapsed.TotalSeconds -lt 6) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 100; if ([F]::Visible([uint32]$p.Id) -eq 0) { $hidden = [int]$t.Elapsed.TotalMilliseconds; break } }
  Write-Output ("cartoes sumiram em tela cheia: " + ($hidden -ge 0) + " (apos " + $hidden + " ms)")
  $f.Close(); $f.Dispose()
  $t.Restart(); $back = -1
  while ($t.Elapsed.TotalSeconds -lt 6) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 100; if ([F]::Visible([uint32]$p.Id) -ge 3) { $back = [int]$t.Elapsed.TotalMilliseconds; break } }
  Write-Output ("cartoes voltaram ao sair: " + ($back -ge 0) + " (apos " + $back + " ms); visiveis: " + [F]::Visible([uint32]$p.Id))
}
finally { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force } }
