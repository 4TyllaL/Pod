param([string]$Exe, [string]$Work, [string]$Out, [string]$HelperScript)
# Investiga no app REAL se clicar num cartao deixa "nenhuma janela" em primeiro plano.
# Dados isolados (--data-dir); janela de outro programa em processo separado; cliques reais; sem interface neste script.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public static class W {
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct GUITHREADINFO { public int cbSize; public int flags; public IntPtr hwndActive, hwndFocus, hwndCapture, hwndMenuOwner, hwndMoveSize, hwndCaret; public RECT rcCaret; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint tid, ref GUITHREADINFO i);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  public static string Cls(IntPtr h) { if (h == IntPtr.Zero) return "(nenhuma)"; var sb = new StringBuilder(128); GetClassName(h, sb, 128); uint pid; GetWindowThreadProcessId(h, out pid); return sb.ToString() + " (pid " + pid + ")"; }
  public static string RootCls(int x, int y) { var p = new POINT(); p.X = x; p.Y = y; var h = WindowFromPoint(p); if (h == IntPtr.Zero) return "-"; var r = GetAncestor(h, 2); var sb = new StringBuilder(128); GetClassName(r, sb, 128); return sb.ToString(); }
  public static List<IntPtr> Find(uint pid, string cls) {
    var l = new List<IntPtr>();
    EnumWindows((h, _) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(128); GetClassName(h, sb, 128); if (p == pid && sb.ToString() == cls) l.Add(h); return true; }, IntPtr.Zero);
    return l;
  }
  public static string Gui(IntPtr fg) { if (fg == IntPtr.Zero) return "sem janela em primeiro plano"; uint pid; uint tid = GetWindowThreadProcessId(fg, out pid); var i = new GUITHREADINFO(); i.cbSize = Marshal.SizeOf(i); GetGUIThreadInfo(tid, ref i); return "ativa=" + Cls(i.hwndActive) + " foco=" + Cls(i.hwndFocus); }
}
'@
[void][W]::SetProcessDPIAware()
$log = New-Object System.Collections.Generic.List[string]
function Say($s) { $log.Add($s); Write-Output $s }
function Click([int]$x, [int]$y) {
  $o = New-Object W+POINT; [void][W]::GetCursorPos([ref]$o)
  [void][W]::SetCursorPos($x, $y); Start-Sleep -Milliseconds 150
  [W]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 90
  [W]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 450
  [void][W]::SetCursorPos($o.X, $o.Y)
}
function Key([byte]$vk) { [W]::keybd_event($vk, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 50; [W]::keybd_event($vk, 0, 2, [UIntPtr]::Zero); Start-Sleep -Milliseconds 350 }
function ToFront([IntPtr]$h) {
  [W]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [W]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
  [void][W]::SetForegroundWindow($h); Start-Sleep -Milliseconds 450
}
function HelperLines() { $f = Join-Path $Work 'helper.log'; if (Test-Path $f) { return @(Get-Content $f) } else { return @() } }
function TextNow() { $f = Join-Path $Work 'text.txt'; if (Test-Path $f) { return ((Get-Content $f -Raw) -replace "[\r\n]", '').ToLower() } else { return '' } }

$data = Join-Path $Work 'data'
New-Item -ItemType Directory -Force $data | Out-Null
$ws = @'
{"schema_version":3,"generation":1,"instances":[
{"id":"000000000000000000000000000000a1","type_id":"pod.clock","state_version":1,"state":"{\"h24\":true,\"seconds\":false}","visible":true,"shape":"2x1",
 "placement":{"monitor":null,"offset_x_dip":24.0,"offset_y_dip":24.0,"width_dip":188.0,"height_dip":88.0,"ratio_x":0.01,"ratio_y":0.02,"ref_work_w_dip":1920.0,"ref_work_h_dip":1032.0}}]}
'@
Set-Content -Path (Join-Path $data 'workspace.json') -Value $ws -Encoding ascii

$app = $null; $helper = $null
try {
  $app = Start-Process -FilePath $Exe -ArgumentList @('--data-dir', $data) -PassThru
  Say ("app real iniciado, pid " + $app.Id)
  $cards = @()
  for ($i = 0; $i -lt 60 -and $cards.Count -lt 1; $i++) { Start-Sleep -Milliseconds 250; $cards = @([W]::Find([uint32]$app.Id, 'PodWidget')) }
  if ($cards.Count -lt 1) { throw 'nenhuma janela PodWidget' }
  $card = $cards[0]
  $r = New-Object W+RECT; [void][W]::GetWindowRect($card, [ref]$r); $cw = $r.Right - $r.Left; $ch = $r.Bottom - $r.Top

  # lugar livre da area de trabalho (so Progman/WorkerW por baixo) para o cartao ficar visivel no modo normal
  $placed = $false
  foreach ($sc in [System.Windows.Forms.Screen]::AllScreens) {
    for ($yy = [int]($sc.Bounds.Top + 40); $yy -lt $sc.Bounds.Bottom - 140 -and -not $placed; $yy += 60) {
      for ($xx = [int]($sc.Bounds.Left + 40); $xx -lt $sc.Bounds.Right - 260 -and -not $placed; $xx += 80) {
        $free = $true
        foreach ($d in @(@(0,0),@($cw,0),@(0,$ch),@($cw,$ch),@([int]($cw/2),[int]($ch/2)))) { $c = [W]::RootCls($xx + $d[0], $yy + $d[1]); if ($c -ne 'Progman' -and $c -ne 'WorkerW') { $free = $false; break } }
        if (-not $free) { continue }
        [void][W]::SetWindowPos($card, [IntPtr]::Zero, $xx, $yy, 0, 0, 0x0001 -bor 0x0004 -bor 0x0010)
        Start-Sleep -Milliseconds 350
        $p = New-Object W+POINT; $p.X = $xx + [int]($cw / 2); $p.Y = $yy + [int]($ch / 2)
        if ([W]::WindowFromPoint($p) -eq $card) { $placed = $true; $cx = $p.X; $cy = $p.Y; $cardX = $xx; $cardY = $yy; Say ("cartao visivel em " + $xx + "," + $yy + " (" + $cw + "x" + $ch + " px), area de trabalho livre, monitor " + $sc.DeviceName) }
      }
    }
    if ($placed) { break }
  }
  if (-not $placed) { throw 'cartao coberto em todos os lugares' }

  # janela de "outro programa" num lugar longe do cartao
  $fx = $cardX + 700; $fy = $cardY + 300
  if ($fx + 420 -gt [System.Windows.Forms.SystemInformation]::VirtualScreen.Right) { $fx = $cardX }
  $helper = Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $HelperScript, '-Dir', $Work, '-X', $fx, '-Y', $fy) -PassThru
  $hw = [IntPtr]::Zero
  for ($i = 0; $i -lt 80; $i++) { Start-Sleep -Milliseconds 250; $f = Join-Path $Work 'hwnd.txt'; if (Test-Path $f) { $hw = [IntPtr][int64](Get-Content $f -Raw).Trim(); break } }
  if ($hw -eq [IntPtr]::Zero) { throw 'janela de foco nao apareceu' }
  Say ("janela de foco (outro programa): " + [W]::Cls($hw) + " em " + $fx + "," + $fy)
  $hr = New-Object W+RECT; [void][W]::GetWindowRect($hw, [ref]$hr)

  function Trial($label, [int]$x, [int]$y) {
    Remove-Item (Join-Path $Work 'text.txt') -ErrorAction SilentlyContinue
    ToFront $hw
    $before = [W]::GetForegroundWindow(); $n0 = (HelperLines).Count
    if ($x -ge -100000) { Click $x $y }
    $after = [W]::GetForegroundWindow()
    $gui = [W]::Gui($after)
    Key 0x58
    $typed = ((TextNow) -eq "x")
    $ev = @((HelperLines) | Select-Object -Skip $n0)
    Say ("   " + $label)
    Say ("      primeiro plano: antes=" + [W]::Cls($before) + "  depois=" + [W]::Cls($after))
    Say ("      " + $gui)
    Say ("      tecla X chegou ao campo de texto do outro programa: " + $typed)
    Say ("      eventos do outro programa: " + ($(if ($ev.Count) { ($ev -join ' | ') } else { '(nenhum)' })))
  }

  Say '1. SEM CLIQUE (verifica o caminho do teclado)'
  Trial 'so a tecla X, sem clicar em nada' -200000 0
  Say '2. CONTROLE: clique na propria janela do outro programa'
  Trial 'clique no outro programa' ([int](($hr.Left + $hr.Right) / 2)) ([int](($hr.Top + $hr.Bottom) / 2))
  Say '3. CLIQUE REAL NO CARTAO DO POD (modo normal, camada desktop)'
  for ($k = 1; $k -le 4; $k++) { Trial ("clique no cartao, tentativa " + $k) $cx $cy }
  Say '4. CONTROLE: clique na area de trabalho vazia, ao lado do cartao'
  Trial 'clique na area de trabalho' ($cardX + $cw + 60) ($cardY + 30)
}
finally {
  if ($helper -and -not $helper.HasExited) { Set-Content -Path (Join-Path $Work 'quit.txt') -Value 'q'; Start-Sleep -Milliseconds 900; if (-not $helper.HasExited) { Stop-Process -Id $helper.Id -Force } }
  if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
  Start-Sleep -Milliseconds 400
  $log | Set-Content -Path $Out -Encoding utf8
}
