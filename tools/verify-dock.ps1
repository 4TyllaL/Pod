param([Parameter(Mandatory = $true)][string]$Exe, [Parameter(Mandatory = $true)][string]$Work)
# Modo de edicao com a galeria na base (app REAL, dados isolados): abre pelo menu da bandeja, fotografa a tela inteira,
# clica num widget da galeria (deve criar um cartao), troca de categoria, arrasta um widget para o desktop e fecha com Esc.
# Move o cursor e envia teclas/cliques de verdade.
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
  public static int CountClass(uint pid, string cls) { int n = 0; EnumWindows((h, _) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(64); GetClassName(h, sb, 64); if (p == pid && sb.ToString() == cls && IsWindowVisible(h)) n++; return true; }, IntPtr.Zero); return n; }
  public static List<int[]> Rects(uint pid, string cls) { var l = new List<int[]>(); EnumWindows((h, _) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(64); GetClassName(h, sb, 64); if (p == pid && sb.ToString() == cls && IsWindowVisible(h)) { RECT r; GetWindowRect(h, out r); l.Add(new int[] { r.L, r.T, r.R, r.B }); } return true; }, IntPtr.Zero); return l; }
  public static void ClickAsync() { var t = new System.Threading.Thread(() => { mouse_event(2, 0, 0, 0, IntPtr.Zero); mouse_event(4, 0, 0, 0, IntPtr.Zero); }); t.Start(); }
  public static void Click() { mouse_event(2, 0, 0, 0, IntPtr.Zero); mouse_event(4, 0, 0, 0, IntPtr.Zero); }
  public static void Key(byte vk) { keybd_event(vk, 0, 0, IntPtr.Zero); keybd_event(vk, 0, 2, IntPtr.Zero); }
}
'@
if (Test-Path $Work) { Remove-Item $Work -Recurse -Force }
New-Item -ItemType Directory $Work | Out-Null
$p = Start-Process -FilePath $Exe -ArgumentList @('--data-dir', (Join-Path $Work 'dados')) -PassThru
Add-Type -AssemblyName System.Windows.Forms
function ShotScreen($name) { $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height; $g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $b.Size); $bmp.Save((Join-Path $Work "$name.png")); $g.Dispose(); $bmp.Dispose() }
function Dock() { return [M]::Find([uint32]$p.Id, 'PodDock', $true) }
function CountTiles() { return [M]::CountClass([uint32]$p.Id, 'PodWidget') }
try {
  $ctl = [IntPtr]::Zero
  for ($i = 0; $i -lt 60 -and $ctl -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 250; $ctl = [M]::Find([uint32]$p.Id, 'PodControl', $false) }
  Start-Sleep -Seconds 2
  $before = CountTiles
  [void][M]::SetCursorPos(300, 200)
  [M]::Key(0x12); [void][M]::PostMessage($ctl, 0x8001, [IntPtr]::Zero, [IntPtr]0x205); Start-Sleep -Milliseconds 700
  foreach ($k in 1..3) { [M]::Key(0x28); Start-Sleep -Milliseconds 120 }
  [M]::Key(0x0D); Start-Sleep -Milliseconds 1200
  $d = Dock
  Write-Output ("1. Editar widgets abre a galeria na base: " + ($d -ne [IntPtr]::Zero))
  if ($d -eq [IntPtr]::Zero) { throw 'galeria nao abriu' }
  $slots = @([M]::Rects([uint32]$p.Id, 'PodSlot'))
  Write-Output ("   espacos + visiveis: " + $slots.Count + " (esperado 3) em " + (($slots | ForEach-Object { '(' + $_[0] + ',' + $_[1] + ')' }) -join ' '))
  $r = New-Object M+RECT; [void][M]::GetWindowRect($d, [ref]$r)
  $scr = [System.Windows.Forms.Screen]::PrimaryScreen
  Write-Output ("   galeria " + ($r.R - $r.L) + "x" + ($r.B - $r.T) + " px; base da galeria a " + ($scr.WorkingArea.Bottom - $r.B) + " px da borda util; centralizada: " + ([math]::Abs((($r.L + $r.R) / 2) - (($scr.WorkingArea.Left + $scr.WorkingArea.Right) / 2)) -lt 3))
  ShotScreen 'dock-edicao'
  $dpi = ($r.R - $r.L) / 920.0; if ($dpi -gt 1.6) { $dpi = ($r.B - $r.T) / 196.0 } else { $dpi = ($r.B - $r.T) / 196.0 }
  # primeiro widget da galeria: x = 22 + 46 dip, y = 70 + 32 dip
  $cx = $r.L + [int]((22 + 46) * $dpi); $cy = $r.T + [int]((70 + 32) * $dpi)
  # escolhe o 2o espaco + e depois adiciona pela galeria: o cartao deve nascer ali
  $sl = $slots[1]; [void][M]::SetCursorPos([int](($sl[0] + $sl[2]) / 2), [int](($sl[1] + $sl[3]) / 2)); Start-Sleep -Milliseconds 200; [M]::Click(); Start-Sleep -Milliseconds 300
  ShotScreen 'dock-slot-escolhido'
  $known = @([M]::Rects([uint32]$p.Id, 'PodWidget') | ForEach-Object { "$($_[0]),$($_[1])" })
  [void][M]::SetCursorPos($cx, $cy); Start-Sleep -Milliseconds 300
  ShotScreen 'dock-hover'
  [M]::Click(); Start-Sleep -Milliseconds 900
  $after = CountTiles
  $now = @([M]::Rects([uint32]$p.Id, 'PodWidget') | ForEach-Object { "$($_[0]),$($_[1])" })
  $novo = @($now | Where-Object { $known -notcontains $_ })
  Write-Output ("   o cartao novo nasceu em " + ($novo -join ' ') + "; espaco escolhido em " + $sl[0] + "," + $sl[1] + "; coincide: " + ($novo -contains ("" + $sl[0] + "," + $sl[1])))
  Write-Output ("2. clicar num widget da galeria cria um cartao: antes " + $before + ", depois " + $after)
  # Restaurar padrao: recusar nao muda nada; confirmar volta aos 3 iniciais
  $rx = $r.L + [int]((900 - 22 - 104 - 8 - 60) * $dpi); $ry = $r.T + [int](32 * $dpi)
  function Box() { return [M]::Find([uint32]$p.Id, '#32770', $true) }
  [void][M]::SetCursorPos($rx, $ry); Start-Sleep -Milliseconds 200
  $t = [System.Diagnostics.Stopwatch]::StartNew()
  $job = Start-Job { }  # (so para manter o fluxo simples)
  Remove-Job $job -Force
  [M]::ClickAsync(); Start-Sleep -Milliseconds 900
  $bx = Box
  Write-Output ("3a. Restaurar padrao pede confirmacao: " + ($bx -ne [IntPtr]::Zero))
  if ($bx -ne [IntPtr]::Zero) { [void][M]::PostMessage($bx, 0x111, [IntPtr]7, [IntPtr]::Zero); Start-Sleep -Milliseconds 500 }
  Write-Output ("3b. responder Nao nao muda nada: cartoes = " + (CountTiles) + " (esperado " + $after + ")")
  [M]::ClickAsync(); Start-Sleep -Milliseconds 900
  $bx = Box
  if ($bx -ne [IntPtr]::Zero) { [void][M]::PostMessage($bx, 0x111, [IntPtr]6, [IntPtr]::Zero); Start-Sleep -Milliseconds 3000 }
  Write-Output ("3c. responder Sim restaura os 3 iniciais: cartoes = " + (CountTiles) + " (esperado 3); em " + ((@([M]::Rects([uint32]$p.Id, 'PodWidget') | ForEach-Object { "$($_[0]),$($_[1])" })) -join ' ') + "")
  Write-Output ("   processo vivo: " + (-not $p.HasExited) + "; galeria aberta: " + ((Dock) -ne [IntPtr]::Zero))
  ShotScreen 'dock-restaurado'
  $tab = $r.L + [int]((214 + 190) * $dpi); $ty = $r.T + [int](32 * $dpi)
  [void][M]::SetCursorPos($tab, $ty); Start-Sleep -Milliseconds 200; [M]::Click(); Start-Sleep -Milliseconds 400
  ShotScreen 'dock-categoria'
  [M]::Key(0x1B); Start-Sleep -Milliseconds 600
  Write-Output ("4. Esc fecha a galeria: " + ((Dock) -eq [IntPtr]::Zero))
}
finally { if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force } }
