param([string]$Exe, [string]$Work, [string]$Out, [string]$HelperScript, [string]$Label = 'teste')
# Modo de edicao e menu da bandeja com cartoes sem ativacao (app REAL, dados isolados em --data-dir).
# Abre o menu da bandeja de verdade (mensagem da bandeja), escolhe itens pelo teclado, arrasta/redimensiona com o mouse,
# exclui pelo menu de contexto e mede foco e ordem Z. Sem interface neste script.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public static class W {
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetTopWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint c);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr hm);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetMenuString(IntPtr hm, uint i, StringBuilder s, int n, uint f);
  [DllImport("user32.dll")] public static extern bool GetMenuItemRect(IntPtr h, IntPtr hm, uint i, out RECT r);
  [DllImport("user32.dll")] public static extern uint GetMenuState(IntPtr hm, uint i, uint f);
  [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GLP(IntPtr h, int i);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  public static string Cls(IntPtr h) { if (h == IntPtr.Zero) return "(nenhuma)"; var sb = new StringBuilder(128); GetClassName(h, sb, 128); uint pid; GetWindowThreadProcessId(h, out pid); return sb.ToString() + " (pid " + pid + ")"; }
  public static string Title(IntPtr h) { var sb = new StringBuilder(256); GetWindowText(h, sb, 256); return sb.ToString(); }
  public static string RootCls(int x, int y) { var p = new POINT(); p.X = x; p.Y = y; var h = WindowFromPoint(p); if (h == IntPtr.Zero) return "-"; var r = GetAncestor(h, 2); var sb = new StringBuilder(128); GetClassName(r, sb, 128); return sb.ToString(); }
  public static string LeafCls(int x, int y) { var p = new POINT(); p.X = x; p.Y = y; var h = WindowFromPoint(p); if (h == IntPtr.Zero) return "-"; var sb = new StringBuilder(128); GetClassName(h, sb, 128); return sb.ToString(); }
  public static IntPtr At(int x, int y) { var p = new POINT(); p.X = x; p.Y = y; return WindowFromPoint(p); }
  public static int Rank(IntPtr h) { var c = GetTopWindow(IntPtr.Zero); int i = 0; while (c != IntPtr.Zero && i < 5000) { if (c == h) return i; c = GetWindow(c, 2); i++; } return -1; }
  public static List<IntPtr> Find(uint pid, string cls) {
    var l = new List<IntPtr>();
    EnumWindows((h, _) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(128); GetClassName(h, sb, 128); if (p == pid && sb.ToString() == cls) l.Add(h); return true; }, IntPtr.Zero);
    return l;
  }
  public static List<string> MenuItems(IntPtr hm) {
    var l = new List<string>(); int n = GetMenuItemCount(hm);
    for (uint i = 0; i < n; i++) { uint st = GetMenuState(hm, i, 0x400); var sb = new StringBuilder(256); GetMenuString(hm, i, sb, 256, 0x400); bool sep = (st & 0x800) != 0; l.Add((sep ? "-" : "+") + ((st & 3) != 0 ? "x" : ((st & 8) != 0 ? "v" : " ")) + sb.ToString()); }
    return l;
  }
}
'@
[void][W]::SetProcessDPIAware()
$log = New-Object System.Collections.Generic.List[string]
function Say($s) { $log.Add($s); Write-Output $s }
function Sleep-Ms($m) { Start-Sleep -Milliseconds $m }
function Click([int]$x, [int]$y) {
  $o = New-Object W+POINT; [void][W]::GetCursorPos([ref]$o)
  [void][W]::SetCursorPos($x, $y); Sleep-Ms 150
  [W]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); Sleep-Ms 90
  [W]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero); Sleep-Ms 450
  [void][W]::SetCursorPos($o.X, $o.Y)
}
function RightClick([int]$x, [int]$y) {
  $o = New-Object W+POINT; [void][W]::GetCursorPos([ref]$o)
  [void][W]::SetCursorPos($x, $y); Sleep-Ms 150
  [W]::mouse_event(8, 0, 0, 0, [UIntPtr]::Zero); Sleep-Ms 90
  [W]::mouse_event(16, 0, 0, 0, [UIntPtr]::Zero); Sleep-Ms 500
  [void][W]::SetCursorPos($o.X, $o.Y)
}
function Drag([int]$x, [int]$y, [int]$dx, [int]$dy) {
  $o = New-Object W+POINT; [void][W]::GetCursorPos([ref]$o)
  [void][W]::SetCursorPos($x, $y); Sleep-Ms 150
  [W]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); Sleep-Ms 150
  for ($i = 1; $i -le 12; $i++) { [void][W]::SetCursorPos($x + [int]($dx * $i / 12), $y + [int]($dy * $i / 12)); Sleep-Ms 35 }
  Sleep-Ms 120
  [W]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero); Sleep-Ms 600
  [void][W]::SetCursorPos($o.X, $o.Y)
}
function Key([byte]$vk) { [W]::keybd_event($vk, 0, 0, [UIntPtr]::Zero); Sleep-Ms 50; [W]::keybd_event($vk, 0, 2, [UIntPtr]::Zero); Sleep-Ms 300 }
function ToFront([IntPtr]$h) {
  [W]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [W]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
  [void][W]::SetForegroundWindow($h); Sleep-Ms 450
  Key 0x1B
}
function Rect($h) { $r = New-Object W+RECT; [void][W]::GetWindowRect($h, [ref]$r); return $r }
function IsTop($h) { return ([int64][W]::GLP($h, -20) -band 0x8) -ne 0 }
function Vis($h) { return [W]::IsWindowVisible($h) }
function MenuPopup() { foreach ($p in [W]::Find([uint32]$app.Id, '#32768')) { if ([W]::IsWindowVisible($p)) { return $p } } return [IntPtr]::Zero }
function WaitMenu([int]$ms) { $t = [Diagnostics.Stopwatch]::StartNew(); while ($t.ElapsedMilliseconds -lt $ms) { $p = MenuPopup; if ($p -ne [IntPtr]::Zero) { return $p }; Sleep-Ms 60 }; return [IntPtr]::Zero }
function OpenTray() {
  $ctl = @([W]::Find([uint32]$app.Id, 'PodControl'))
  if ($ctl.Count -lt 1) { throw 'janela de controle (bandeja) nao encontrada' }
  [void][W]::PostMessage($ctl[0], 0x8001, [IntPtr]::Zero, [IntPtr]0x205)   # WM_APP+1 com WM_RBUTTONUP = clique direito no icone da bandeja
  return (WaitMenu 2500)
}
function Items($popup) { $hm = [W]::SendMessage($popup, 0x1E1, [IntPtr]::Zero, [IntPtr]::Zero); return [W]::MenuItems($hm) }
# escolhe um item do menu CLICANDO nele (retangulo do item informado pelo Windows); `$owner` e a janela dona do menu
function Pick($popup, [string]$pattern, $owner) {
  $hm = [W]::SendMessage($popup, 0x1E1, [IntPtr]::Zero, [IntPtr]::Zero)
  $its = @([W]::MenuItems($hm)); $idx = -1
  for ($i = 0; $i -lt $its.Count; $i++) { if ($its[$i].StartsWith('+') -and $its[$i].Substring(2) -match $pattern) { $idx = $i; break } }
  if ($idx -lt 0) { return $false }
  $r = New-Object W+RECT
  if ([W]::GetMenuItemRect($owner, $hm, [uint32]$idx, [ref]$r)) {
    Click ([int](($r.Left + $r.Right) / 2)) ([int](($r.Top + $r.Bottom) / 2)); Sleep-Ms 400; return $true
  }
  # alternativa: UI Automation (itens de menu nativos expoem o retangulo)
  try {
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($popup)
    $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    foreach ($e in $all) {
      if ($e.Current.ControlType.ProgrammaticName -eq 'ControlType.MenuItem' -and $e.Current.Name -match $pattern) {
        $b = $e.Current.BoundingRectangle; Click ([int]($b.Left + $b.Width / 2)) ([int]($b.Top + $b.Height / 2)); Sleep-Ms 400; return $true
      }
    }
  } catch { }
  return $false
}
function HelperLines() { $f = Join-Path $Work 'helper.log'; if (Test-Path $f) { return @(Get-Content $f) } else { return @() } }

$data = Join-Path $Work 'data'
New-Item -ItemType Directory -Force $data | Out-Null
$ws = @'
{"schema_version":3,"generation":1,"instances":[
{"id":"000000000000000000000000000000c1","type_id":"pod.water","state_version":1,"state":"{\"goal_ml\":2000,\"glass_ml\":250,\"day\":null,\"drunk_ml\":0}","visible":true,"shape":"2x2",
 "placement":{"monitor":null,"offset_x_dip":24.0,"offset_y_dip":24.0,"width_dip":188.0,"height_dip":188.0,"ratio_x":0.01,"ratio_y":0.02,"ref_work_w_dip":1920.0,"ref_work_h_dip":1032.0}},
{"id":"000000000000000000000000000000c2","type_id":"pod.clock","state_version":1,"state":"{\"h24\":true,\"seconds\":false}","visible":true,"shape":"2x1",
 "placement":{"monitor":null,"offset_x_dip":240.0,"offset_y_dip":24.0,"width_dip":188.0,"height_dip":88.0,"ratio_x":0.12,"ratio_y":0.02,"ref_work_w_dip":1920.0,"ref_work_h_dip":1032.0}}]}
'@
Set-Content -Path (Join-Path $data 'workspace.json') -Value $ws -Encoding ascii

$app = $null; $hproc = $null
try {
  Say ("=== " + $Label + " ===")
  $app = Start-Process -FilePath $Exe -ArgumentList @('--data-dir', $data) -PassThru
  $wins = @()
  for ($i = 0; $i -lt 60 -and $wins.Count -lt 2; $i++) { Sleep-Ms 250; $wins = @([W]::Find([uint32]$app.Id, 'PodWidget')) }
  if ($wins.Count -lt 2) { throw 'esperava 2 cartoes' }
  Sleep-Ms 700
  $water = $null; $clock = $null
  foreach ($w in $wins) { $r = Rect $w; if (($r.Bottom - $r.Top) -gt 150) { $water = $w } else { $clock = $w } }
  Say ("app real, pid " + $app.Id + "; cartao de agua 0x" + ('{0:x}' -f [int64]$water) + " e relogio 0x" + ('{0:x}' -f [int64]$clock))

  function FreeSpot([int]$w, [int]$h, $avoid) {
    foreach ($sc in [System.Windows.Forms.Screen]::AllScreens) {
      for ($yy = [int]($sc.Bounds.Top + 30); $yy -lt $sc.Bounds.Bottom - $h - 20; $yy += 40) {
        for ($xx = [int]($sc.Bounds.Left + 30); $xx -lt $sc.Bounds.Right - $w - 20; $xx += 40) {
          $skip = $false
          foreach ($av in $avoid) { if ($xx -lt $av[0] + $av[2] + 30 -and $xx + $w + 30 -gt $av[0] -and $yy -lt $av[1] + $av[3] + 30 -and $yy + $h + 30 -gt $av[1]) { $skip = $true; break } }
          if ($skip) { continue }
          $free = $true
          foreach ($fx in 0, [int]($w / 2), $w) { foreach ($fy in 0, [int]($h / 2), $h) {
            $c = [W]::RootCls($xx + $fx, $yy + $fy); $l = [W]::LeafCls($xx + $fx, $yy + $fy)
            if (($c -ne 'Progman' -and $c -ne 'WorkerW') -or ($l -notin 'SysListView32', 'SHELLDLL_DefView', 'Progman', 'WorkerW')) { $free = $false; break } }
            if (-not $free) { break } }
          if ($free) { return @($xx, $yy) }
        }
      }
    }
    return $null
  }
  $ctrl = @([W]::Find([uint32]$app.Id, 'PodControl'))[0]
  $rw = Rect $water; $rc = Rect $clock
  $ww = $rw.Right - $rw.Left; $wh = $rw.Bottom - $rw.Top; $cw = $rc.Right - $rc.Left; $ch = $rc.Bottom - $rc.Top
  # reserva espaco para o cartao de agua crescer ao redimensionar (formato 4x2 = 388x188)
  $sW = FreeSpot 420 220 @(); if (-not $sW) { throw 'sem area livre para o cartao de agua' }
  $av = @(); $av += ,@($sW[0], $sW[1], 420, 220)
  $sC = FreeSpot $cw $ch $av; if (-not $sC) { throw 'sem area livre para o relogio' }
  [void][W]::SetWindowPos($water, [IntPtr]::Zero, $sW[0], $sW[1], 0, 0, 0x0001 -bor 0x0004 -bor 0x0010)
  [void][W]::SetWindowPos($clock, [IntPtr]::Zero, $sC[0], $sC[1], 0, 0, 0x0001 -bor 0x0004 -bor 0x0010)
  Sleep-Ms 500
  $rw = Rect $water; $rc = Rect $clock
  $wcx = [int](($rw.Left + $rw.Right) / 2); $wcy = [int](($rw.Top + $rw.Bottom) / 2); $ccx = [int](($rc.Left + $rc.Right) / 2); $ccy = [int](($rc.Top + $rc.Bottom) / 2)
  if (([W]::At($wcx, $wcy) -ne $water) -or ([W]::At($ccx, $ccy) -ne $clock)) { throw 'cartoes nao ficaram visiveis' }
  $nf = $clock; $nfStyle = [int64][W]::GLP($water, -20)
  Say ("cartoes visiveis (area de trabalho livre). Estilo do cartao de agua 0x" + ('{0:x}' -f $nfStyle) + " (NOACTIVATE " + $(if ($nfStyle -band 0x08000000) { 'PRESENTE' } else { 'ausente' }) + ")")

  # outro programa em processo separado (em lugar livre)
  $av2 = @(); $av2 += ,@($sW[0], $sW[1], 420, 220); $av2 += ,@($sC[0], $sC[1], $cw, $ch)
  $sH = FreeSpot 420 220 $av2; if ($sH) { $fx = $sH[0]; $fy = $sH[1] } else { $fx = $sW[0] + 900; $fy = $sW[1] + 400 }
  $hproc = Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $HelperScript, '-Dir', $Work, '-X', $fx, '-Y', $fy) -PassThru
  $hw = [IntPtr]::Zero
  for ($i = 0; $i -lt 80; $i++) { Sleep-Ms 250; $f = Join-Path $Work 'hwnd.txt'; if (Test-Path $f) { $hw = [IntPtr][int64](Get-Content $f -Raw).Trim(); break } }
  if ($hw -eq [IntPtr]::Zero) { throw 'janela do outro programa nao apareceu' }
  $hr = Rect $hw; $hcx = [int](($hr.Left + $hr.Right) / 2); $hcy = [int](($hr.Top + $hr.Bottom) / 2)

  # ================= A. MENU DA BANDEJA =================
  Say 'A. MENU DA BANDEJA (aberto como pelo icone: botao direito na bandeja)'
  Click $hcx $hcy; ToFront $hw
  $pop = OpenTray
  if ($pop -eq [IntPtr]::Zero) { Say '   ERRO: o menu da bandeja nao abriu' } else {
    Say ("   o menu abriu; itens: " + ((Items $pop) -join ' ; '))
    Key 0x1B
    Say ("   Esc: o menu fechou = " + ((MenuPopup) -eq [IntPtr]::Zero) + "  (o teclado so chega ao menu se o app tomar o primeiro plano; aqui o primeiro plano e " + [W]::Cls([W]::GetForegroundWindow()) + ")")
  }
  $pop = OpenTray
  Click ($hr.Left - 200) ($hr.Top + 20)
  Say ("   clique fora: o menu fechou = " + ((MenuPopup) -eq [IntPtr]::Zero))

  # ================= B. MODO DE EDICAO =================
  Say 'B. MODO DE EDICAO (Editar widgets pela bandeja)'
  Click $hcx $hcy; ToFront $hw
  $pop = OpenTray
  $ok = $false; if ($pop -ne [IntPtr]::Zero) { $ok = Pick $pop 'Editar widgets' $ctrl }
  $gal = @([W]::Find([uint32]$app.Id, 'PodGallery'))
  Say ("   escolheu 'Editar widgets...' = " + $ok + "; galeria (Pod Studio) aberta = " + ($gal.Count -gt 0 -and (Vis $gal[0])))
  Say ("   cartoes TOPMOST: agua = " + (IsTop $water) + ", relogio = " + (IsTop $clock) + "; primeiro plano agora = " + [W]::Cls([W]::GetForegroundWindow()))
  Say ("   cartoes continuam SEM ativacao (NOACTIVATE): agua = " + ([bool]([int64][W]::GLP($water, -20) -band 0x08000000)))

  # B2: o cartao inteiro arrasta (modo de edicao)
  $r0 = Rect $clock
  Drag $ccx $ccy 90 60
  $r1 = Rect $clock
  Say ("   arrastar o relogio em edicao: de " + $r0.Left + "," + $r0.Top + " para " + $r1.Left + "," + $r1.Top + " (moveu = " + (($r1.Left -ne $r0.Left) -or ($r1.Top -ne $r0.Top)) + "); primeiro plano = " + [W]::Cls([W]::GetForegroundWindow()))
  # B3: borda redimensiona (cartao de agua tem varios formatos)
  $r0 = Rect $water
  Drag ($r0.Right - 2) ([int](($r0.Top + $r0.Bottom) / 2)) 200 0
  $r1 = Rect $water
  Say ("   puxar a borda direita do cartao de agua: " + ($r0.Right - $r0.Left) + "x" + ($r0.Bottom - $r0.Top) + " -> " + ($r1.Right - $r1.Left) + "x" + ($r1.Bottom - $r1.Top) + " px (mudou de formato = " + ((($r1.Right - $r1.Left) -ne ($r0.Right - $r0.Left)) -or (($r1.Bottom - $r1.Top) -ne ($r0.Bottom - $r0.Top))) + ")")
  # B4: tray enquanto em edicao mostra "Concluir edicao"
  $pop = OpenTray
  if ($pop -ne [IntPtr]::Zero) {
    $its = @(Items $pop); Say ("   bandeja em edicao: item de edicao = " + (($its | Where-Object { $_ -match 'edi' }) -join ' / ') + "  (v = marcado)")
    $ok = Pick $pop 'Editar widgets' $ctrl
    Sleep-Ms 500
    $gal = @([W]::Find([uint32]$app.Id, 'PodGallery'))
    Say ("   desligar a edicao pela bandeja (item marcado): escolheu = " + $ok + "; galeria fechou = " + (-not ($gal.Count -gt 0 -and (Vis $gal[0]))) + "; cartoes voltaram ao fundo (sem TOPMOST): agua = " + (-not (IsTop $water)) + ", relogio = " + (-not (IsTop $clock)))
  } else { Say '   ERRO: bandeja nao abriu em edicao' }
  # sair tambem pelo Esc da galeria
  Click $hcx $hcy; ToFront $hw
  $pop = OpenTray; if ($pop -ne [IntPtr]::Zero) { [void](Pick $pop 'Editar widgets' $ctrl) }
  $gal = @([W]::Find([uint32]$app.Id, 'PodGallery'))
  if ($gal.Count -gt 0) { [void][W]::SetForegroundWindow($gal[0]); Sleep-Ms 400; Key 0x1B; Sleep-Ms 500 }
  $gal = @([W]::Find([uint32]$app.Id, 'PodGallery'))
  Say ("   sair com Esc na galeria: galeria fechou = " + (-not ($gal.Count -gt 0 -and (Vis $gal[0]))) + "; cartoes sem TOPMOST: " + ((-not (IsTop $water)) -and (-not (IsTop $clock))))
  # depois da edicao os cartoes ficam abaixo das outras janelas (ordem Z) e o outro programa pode voltar ao foco
  Click $hcx $hcy; ToFront $hw
  Say ("   depois da edicao, o cartao de agua esta ABAIXO da janela do outro programa (ordem Z) = " + ([W]::Rank($water) -gt [W]::Rank($hw)))

  # ================= C. BANDEJA: ocultar tudo e revelar =================
  Say 'C. BANDEJA: ocultar/mostrar tudo e revelar um cartao pela lista'
  $pop = OpenTray
  if ($pop -ne [IntPtr]::Zero) { [void](Pick $pop 'cultar|ostrar' $ctrl) ; Sleep-Ms 500 }
  Say ("   depois de 'ocultar/mostrar': agua visivel = " + (Vis $water) + ", relogio visivel = " + (Vis $clock))
  $pop = OpenTray
  if ($pop -ne [IntPtr]::Zero) { [void](Pick $pop 'cultar|ostrar' $ctrl) ; Sleep-Ms 500 }
  Say ("   alternando de novo: agua visivel = " + (Vis $water) + ", relogio visivel = " + (Vis $clock))
  # revelar: cobre o relogio com o outro programa e escolhe o relogio na lista da bandeja
  [void][W]::SetWindowPos($hw, [IntPtr]::Zero, $rc.Left - 100, $rc.Top - 60, 0, 0, 0x0001 -bor 0x0004 -bor 0x0010)
  Sleep-Ms 300
  Click $hcx $hcy
  $hr = Rect $hw; $ccx2 = [int](($r1.Left + $r1.Right) / 2)
  $rcn = Rect $clock; $ccx = [int](($rcn.Left + $rcn.Right) / 2); $ccy = [int](($rcn.Top + $rcn.Bottom) / 2)
  [void][W]::SetWindowPos($hw, [IntPtr]::Zero, $ccx - 210, $ccy - 110, 0, 0, 0x0001 -bor 0x0004 -bor 0x0010)   # centraliza o outro programa SOBRE o relogio
  Sleep-Ms 400; ToFront $hw
  Say ("   o relogio esta coberto pelo outro programa = " + ([W]::At($ccx, $ccy) -ne $clock))
  $pop = OpenTray
  if ($pop -ne [IntPtr]::Zero) {
    $its = @(Items $pop)
    Say ("   itens da lista de cartoes na bandeja: " + (($its | Where-Object { $_ -match 'Rel|Agua|gua|Nota' }) -join ' ; '))
    $ok = Pick $pop 'Rel' $ctrl
    Sleep-Ms 700
    Say ("   revelar o relogio: escolheu = " + $ok + "; o relogio ficou na frente do outro programa = " + ([W]::At($ccx, $ccy) -eq $clock) + "; primeiro plano = " + [W]::Cls([W]::GetForegroundWindow()))
  }

  # ================= D. EXCLUIR pelo menu de contexto (com confirmacao) =================
  Say 'D. EXCLUIR o relogio pelo menu de contexto (so notas pedem confirmacao: widget sem conteudo e apagado na hora)'
  $rcn = Rect $clock; $ccx = [int](($rcn.Left + $rcn.Right) / 2); $ccy = [int](($rcn.Top + $rcn.Bottom) / 2)
  [void][W]::SetWindowPos($hw, [IntPtr]::Zero, $fx, $fy, 0, 0, 0x0001 -bor 0x0004 -bor 0x0010); Sleep-Ms 300
  $hr = Rect $hw; $hcx = [int](($hr.Left + $hr.Right) / 2); $hcy = [int](($hr.Top + $hr.Bottom) / 2)
  Click $hcx $hcy; ToFront $hw
  RightClick $ccx $ccy
  $pop = WaitMenu 1500
  if ($pop -eq [IntPtr]::Zero) { Say '   ERRO: o menu de contexto nao abriu' } else {
    Say ("   menu de contexto aberto sobre o cartao; itens: " + ((Items $pop) -join ' ; '))
    $ok = Pick $pop 'Excluir' $clock
    Sleep-Ms 1200
    $dlg = @([W]::Find([uint32]$app.Id, '#32770') | Where-Object { [W]::IsWindowVisible($_) })
    $left = @([W]::Find([uint32]$app.Id, 'PodWidget') | Where-Object { [W]::IsWindowVisible($_) })
    Say ("   clicou em 'Excluir widget...' = " + $ok + "; janela de confirmacao = " + ($dlg.Count -gt 0) + "; cartoes visiveis restantes = " + $left.Count + " (o relogio foi excluido = " + ($left.Count -eq 1) + ")")
    $wsf = Join-Path $data 'workspace.json'
    $n = -1; for ($i = 0; $i -lt 20; $i++) { Sleep-Ms 250; $n = ([regex]::Matches((Get-Content $wsf -Raw), '"type_id"')).Count; if ($n -eq 1) { break } }
    Say ("   workspace gravado sem o relogio (instancias no arquivo = " + $n + ") = " + ($n -eq 1))
  }
}
finally {
  if ($hproc -and -not $hproc.HasExited) { Set-Content -Path (Join-Path $Work 'quit.txt') -Value 'q'; Sleep-Ms 900; if (-not $hproc.HasExited) { Stop-Process -Id $hproc.Id -Force } }
  if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
  Sleep-Ms 400
  $log | Set-Content -Path $Out -Encoding utf8
}
