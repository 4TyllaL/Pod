param([Parameter(Mandatory = $true)][string]$Exe, [Parameter(Mandatory = $true)][string]$Work)
# Abre o menu proprio da bandeja no app REAL (dados isolados), passa o mouse em "Ajustes" para abrir o submenu,
# fotografa os paineis (PNG), navega pelo teclado e confere que fecha com Esc e ao clicar fora.
# Move o cursor e envia teclas de verdade.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public static class M {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, IntPtr e);
  delegate bool P(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(P p, IntPtr l);
  public static IntPtr Find(uint pid, string cls, bool visibleOnly) { IntPtr r = IntPtr.Zero; EnumWindows((h, _) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(64); GetClassName(h, sb, 64); if (p == pid && sb.ToString() == cls && (!visibleOnly || IsWindowVisible(h))) { r = h; return false; } return true; }, IntPtr.Zero); return r; }
  public static void Click() { mouse_event(2, 0, 0, 0, IntPtr.Zero); mouse_event(4, 0, 0, 0, IntPtr.Zero); }
  public static void Key(byte vk) { keybd_event(vk, 0, 0, IntPtr.Zero); keybd_event(vk, 0, 2, IntPtr.Zero); }
}
'@
if (Test-Path $Work) { Remove-Item $Work -Recurse -Force }
New-Item -ItemType Directory $Work | Out-Null
$p = Start-Process -FilePath $Exe -ArgumentList @('--data-dir', (Join-Path $Work 'dados')) -PassThru
function Shot($name, $h) { $r = New-Object M+RECT; [void][M]::GetWindowRect($h, [ref]$r); $w = $r.R - $r.L; $hh = $r.B - $r.T; $m2 = 24; $b = New-Object System.Drawing.Bitmap ($w+2*$m2), ($hh+2*$m2); $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen($r.L-$m2, $r.T-$m2, 0, 0, (New-Object System.Drawing.Size ($w+2*$m2), ($hh+2*$m2))); $b.Save((Join-Path $Work "$name.png")); $g.Dispose(); $b.Dispose(); return $r }
function Menu() { return [M]::Find([uint32]$p.Id, 'PodMenu', $true) }
function Sub() { return [M]::Find([uint32]$p.Id, 'PodMenuSub', $true) }
try {
  $ctl = [IntPtr]::Zero
  for ($i = 0; $i -lt 60 -and $ctl -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 250; $ctl = [M]::Find([uint32]$p.Id, 'PodControl', $false) }
  Start-Sleep -Seconds 2
  [void][M]::SetCursorPos(600, 300)
  [M]::Key(0x12); [void][M]::PostMessage($ctl, 0x8001, [IntPtr]::Zero, [IntPtr]0x205)
  Start-Sleep -Milliseconds 700
  $m = Menu
  Write-Output ("1. menu abriu: " + ($m -ne [IntPtr]::Zero) + "; e o primeiro plano (teclado): " + ([M]::GetForegroundWindow() -eq $m))
  $r = Shot 'menu-principal' $m
  Write-Output ("   painel " + ($r.R - $r.L) + "x" + ($r.B - $r.T) + " px em (" + $r.L + "," + $r.T + ")")
  # linhas: Nova nota, Novo widget, Editar, Meus widgets (se houver), sep, Ocultar, Ajustes, sep, Sair  (30 dip = ~30 px a 96 dpi)
  $dpi = ($r.R - $r.L) / 232.0
  $ajY = $r.T + [int](($r.B - $r.T) - (6 + 30 + 11) * $dpi - 15 * $dpi)
  [void][M]::SetCursorPos($r.L + 80, $ajY); Start-Sleep -Milliseconds 100; [void][M]::SetCursorPos($r.L + 82, $ajY + 1); Start-Sleep -Milliseconds 600
  $s = Sub
  Write-Output ("2. passar o mouse em Ajustes abre o submenu: " + ($s -ne [IntPtr]::Zero))
  if ($s -ne [IntPtr]::Zero) { $sr = Shot 'menu-submenu' $s; Write-Output ("   submenu " + ($sr.R - $sr.L) + "x" + ($sr.B - $sr.T)) }
  $fullR = Shot 'menu-principal-hover' $m
  [M]::Key(0x1B); Start-Sleep -Milliseconds 300
  Write-Output ("3. Esc fecha o menu (primeiro Esc fecha so o submenu quando o teclado esta nele): menu aberto = " + ((Menu) -ne [IntPtr]::Zero))
  if ((Menu) -ne [IntPtr]::Zero) { [M]::Key(0x1B); Start-Sleep -Milliseconds 300; Write-Output ("   segundo Esc: menu aberto = " + ((Menu) -ne [IntPtr]::Zero)) }
  [M]::Key(0x12); [void][M]::PostMessage($ctl, 0x8001, [IntPtr]::Zero, [IntPtr]0x205); Start-Sleep -Milliseconds 600
  Write-Output ("4. reabriu: " + ((Menu) -ne [IntPtr]::Zero))
  [void][M]::SetCursorPos(1500, 900); [M]::Click(); Start-Sleep -Milliseconds 400
  Write-Output ("   clicar fora fecha: menu aberto = " + ((Menu) -ne [IntPtr]::Zero))
  [M]::Key(0x12); [void][M]::PostMessage($ctl, 0x8001, [IntPtr]::Zero, [IntPtr]0x205); Start-Sleep -Milliseconds 600
  [M]::Key(0x28); [M]::Key(0x28); Start-Sleep -Milliseconds 200
  $null = Shot 'menu-teclado' (Menu)
  [M]::Key(0x1B); Start-Sleep -Milliseconds 300
  Write-Output ("5. navegacao por teclado (setas) fotografada; fechou com Esc: " + ((Menu) -eq [IntPtr]::Zero))
  # 6. "Iniciar com o Windows" pelo teclado (so se nao houver um valor Pod no Run do usuario; desfaz no fim)
  $rk = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
  if (Get-ItemProperty $rk -Name Pod -ErrorAction SilentlyContinue) { Write-Output '6. ja ha um valor Pod no Run: nao testado (para nao sobrescrever)' } else {
    foreach ($round in 1, 2) {
      [void][M]::SetCursorPos(300, 200)
      [M]::Key(0x12); [void][M]::PostMessage($ctl, 0x8001, [IntPtr]::Zero, [IntPtr]0x205); Start-Sleep -Milliseconds 700
      [M]::Key(0x26); Start-Sleep -Milliseconds 150; [M]::Key(0x26); Start-Sleep -Milliseconds 150; [M]::Key(0x27); Start-Sleep -Milliseconds 150; [M]::Key(0x0D); Start-Sleep -Milliseconds 600
      $v = (Get-ItemProperty $rk -Name Pod -ErrorAction SilentlyContinue).Pod
      Write-Output ("6." + $round + " Iniciar com o Windows " + $(if ($round -eq 1) { 'liga' } else { 'desliga' }) + ": valor Run = [" + $v + "]")
    }
  }
}
finally { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }; Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name Pod -ErrorAction SilentlyContinue }
