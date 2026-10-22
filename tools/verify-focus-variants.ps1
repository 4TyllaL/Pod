param([string]$Exe, [string]$Work, [string]$Out, [string]$HelperScript)
# Variantes do cartao (contrato do host): qual detalhe faz o clique deixar "nenhuma janela" em primeiro plano?
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
  public static string LeafCls(int x, int y) { var p = new POINT(); p.X = x; p.Y = y; var h = WindowFromPoint(p); if (h == IntPtr.Zero) return "-"; var sb = new StringBuilder(128); GetClassName(h, sb, 128); return sb.ToString(); }
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
  [W]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 40; [W]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero); Start-Sleep -Milliseconds 250   # Esc: sai do modo de menu que o Alt abriu
}
function HelperLines() { $f = Join-Path $Work 'helper.log'; if (Test-Path $f) { return @(Get-Content $f) } else { return @() } }
function TextNow() { $f = Join-Path $Work 'text.txt'; if (Test-Path $f) { return ((Get-Content $f -Raw) -replace "[\r\n]", '').ToLower() } else { return '' } }


$variants = @('base', 'nocapture', 'exnoactivate', 'ncdefault', 'nocapture_exnoactivate', 'activate')
$hproc = $null
try {
  # lugar livre da area de trabalho para o cartao (88x88): so Progman/WorkerW por baixo
  $cw = 88; $ch = 88; $spot = $null
  foreach ($sc in [System.Windows.Forms.Screen]::AllScreens) {
    for ($yy = [int]($sc.Bounds.Top + 40); $yy -lt $sc.Bounds.Bottom - 140 -and -not $spot; $yy += 60) {
      for ($xx = [int]($sc.Bounds.Left + 40); $xx -lt $sc.Bounds.Right - 260 -and -not $spot; $xx += 80) {
        $free = $true
        foreach ($d in @(@(0,0),@($cw,0),@(0,$ch),@($cw,$ch),@(44,44))) { $c = [W]::RootCls($xx + $d[0], $yy + $d[1]); $l = [W]::LeafCls($xx + $d[0], $yy + $d[1]); if (($c -ne 'Progman' -and $c -ne 'WorkerW') -or ($l -notin 'SysListView32', 'SHELLDLL_DefView', 'Progman', 'WorkerW')) { $free = $false; break } }
        if ($free) { $spot = @($xx, $yy) }
      }
    }
    if ($spot) { break }
  }
  if (-not $spot) { throw 'sem lugar livre na area de trabalho' }
  Say ("lugar livre da area de trabalho para o cartao: " + $spot[0] + "," + $spot[1])
  $fx = $spot[0] + 700; $fy = $spot[1] + 300
  if ($fx + 420 -gt [System.Windows.Forms.SystemInformation]::VirtualScreen.Right) { $fx = $spot[0] }
  $hproc = Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $HelperScript, '-Dir', $Work, '-X', $fx, '-Y', $fy) -PassThru
  $hw = [IntPtr]::Zero
  for ($i = 0; $i -lt 80; $i++) { Start-Sleep -Milliseconds 250; $f = Join-Path $Work 'hwnd.txt'; if (Test-Path $f) { $hw = [IntPtr][int64](Get-Content $f -Raw).Trim(); break } }
  if ($hw -eq [IntPtr]::Zero) { throw 'janela de foco nao apareceu' }
  Say ("outro programa: " + [W]::Cls($hw))
  $summary = @()
  foreach ($v in $variants) {
    $app = Start-Process -FilePath $Exe -ArgumentList @('--focus-card', $v, $spot[0], $spot[1], 40) -PassThru
    try {
      $cards = @()
      for ($i = 0; $i -lt 40 -and $cards.Count -lt 1; $i++) { Start-Sleep -Milliseconds 250; $cards = @([W]::Find([uint32]$app.Id, 'PodLayerCard')) }
      if ($cards.Count -lt 1) { Say ("[" + $v + "] ERRO: sem janela"); continue }
      Start-Sleep -Milliseconds 500
      $cx = $spot[0] + 44; $cy = $spot[1] + 44
      $p = New-Object W+POINT; $p.X = $cx; $p.Y = $cy
      if ([W]::WindowFromPoint($p) -ne $cards[0]) { Say ("[" + $v + "] ERRO: cartao nao esta visivel no ponto (" + [W]::Cls([W]::WindowFromPoint($p)) + ")"); continue }
      $res = @()
      for ($k = 1; $k -le 3; $k++) {
        Remove-Item (Join-Path $Work 'text.txt') -ErrorAction SilentlyContinue
        ToFront $hw
        $before = [W]::GetForegroundWindow(); $n0 = (HelperLines).Count
        Click $cx $cy
        $after = [W]::GetForegroundWindow()
        Key 0x58
        $typed = ((TextNow) -eq 'x')
        $deact = @((HelperLines) | Select-Object -Skip $n0 | Where-Object { $_ -match 'DESATIVADO' }).Count
        $res += ,@($after, $typed, ($before -eq $hw), $deact)
      }
      $nullCount = @($res | Where-Object { $_[0] -eq [IntPtr]::Zero }).Count
      $keepCount = @($res | Where-Object { $_[0] -eq $hw }).Count
      $typedCount = @($res | Where-Object { $_[1] }).Count
      $cardFg = @($res | Where-Object { $_[0] -eq $cards[0] }).Count
      $lastAfter = [W]::Cls($res[2][0]); $deactTotal = ($res | ForEach-Object { $_[3] } | Measure-Object -Sum).Sum
      Say ("[" + $v + "] 3 cliques reais: 'nenhuma janela' em " + $nullCount + "/3; outro programa continuou em primeiro plano em " + $keepCount + "/3; cartao virou primeiro plano em " + $cardFg + "/3; o outro programa registrou DESATIVACAO em " + $deactTotal + "/3; tecla X chegou a ele em " + $typedCount + "/3; ultimo primeiro plano = " + $lastAfter)
    } finally {
      if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
      Start-Sleep -Milliseconds 400
    }
  }
}
finally {
  if ($hproc -and -not $hproc.HasExited) { Set-Content -Path (Join-Path $Work 'quit.txt') -Value 'q'; Start-Sleep -Milliseconds 900; if (-not $hproc.HasExited) { Stop-Process -Id $hproc.Id -Force } }
  Get-Process pod-app -ErrorAction SilentlyContinue | Where-Object { $_.StartTime -gt (Get-Date).AddMinutes(-5) } | Stop-Process -Force -ErrorAction SilentlyContinue
  $log | Set-Content -Path $Out -Encoding utf8
}
