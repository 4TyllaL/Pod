param([string]$Exe, [string]$Work, [string]$Out, [string]$HelperScript, [string]$Label = 'teste')
# Teclado, foco e UI Automation dos cartoes no app REAL (dados isolados em --data-dir; o workspace do usuario nao e tocado).
# Mede: (1) UIA Invoke no botao do cartao, (2) clique real no cartao com outro programa em foco, (3) Espaco/Enter e UIA SetFocus,
# (4) a nota (F6) para provar que ela nao mudou. Sem interface neste script; cliques e teclas reais (mouse_event/keybd_event).
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
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  public static string Cls(IntPtr h) { if (h == IntPtr.Zero) return "(nenhuma)"; var sb = new StringBuilder(128); GetClassName(h, sb, 128); uint pid; GetWindowThreadProcessId(h, out pid); return sb.ToString() + " (pid " + pid + ")"; }
  public static string RootCls(int x, int y) { var p = new POINT(); p.X = x; p.Y = y; var h = WindowFromPoint(p); if (h == IntPtr.Zero) return "-"; var r = GetAncestor(h, 2); var sb = new StringBuilder(128); GetClassName(r, sb, 128); return sb.ToString(); }
  public static string LeafCls(int x, int y) { var p = new POINT(); p.X = x; p.Y = y; var h = WindowFromPoint(p); if (h == IntPtr.Zero) return "-"; var sb = new StringBuilder(128); GetClassName(h, sb, 128); return sb.ToString(); }
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public static bool IsVisible(IntPtr h) { return IsWindowVisible(h); }
  public static IntPtr At(int x, int y) { var p = new POINT(); p.X = x; p.Y = y; return WindowFromPoint(p); }
  public static List<IntPtr> Find(uint pid, string cls) {
    var l = new List<IntPtr>();
    EnumWindows((h, _) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(128); GetClassName(h, sb, 128); if (p == pid && sb.ToString() == cls) l.Add(h); return true; }, IntPtr.Zero);
    return l;
  }
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
function RightClick([int]$x, [int]$y) {
  $o = New-Object W+POINT; [void][W]::GetCursorPos([ref]$o)
  [void][W]::SetCursorPos($x, $y); Start-Sleep -Milliseconds 150
  [W]::mouse_event(8, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 90
  [W]::mouse_event(16, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 500
  [void][W]::SetCursorPos($o.X, $o.Y)
}
function Drag([int]$x, [int]$y, [int]$dx, [int]$dy) {
  $o = New-Object W+POINT; [void][W]::GetCursorPos([ref]$o)
  [void][W]::SetCursorPos($x, $y); Start-Sleep -Milliseconds 150
  [W]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 150
  for ($i = 1; $i -le 12; $i++) { [void][W]::SetCursorPos($x + [int]($dx * $i / 12), $y + [int]($dy * $i / 12)); Start-Sleep -Milliseconds 35 }
  Start-Sleep -Milliseconds 120
  [W]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 500
  [void][W]::SetCursorPos($o.X, $o.Y)
}
function MenuOpen($pid2) { return (@([W]::Find([uint32]$pid2, '#32768') | Where-Object { [W]::IsVisible($_) }).Count -gt 0) }
function Key([byte]$vk) { [W]::keybd_event($vk, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 50; [W]::keybd_event($vk, 0, 2, [UIntPtr]::Zero); Start-Sleep -Milliseconds 350 }
function ToFront([IntPtr]$h) {
  [W]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [W]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
  [void][W]::SetForegroundWindow($h); Start-Sleep -Milliseconds 450
  Key 0x1B
}
function HelperLines() { $f = Join-Path $Work 'helper.log'; if (Test-Path $f) { return @(Get-Content $f) } else { return @() } }
function HelperText() { $f = Join-Path $Work 'text.txt'; if (Test-Path $f) { return (Get-Content $f -Raw) -replace "[\r\n]", '' } else { return '' } }
function Names($hwnd) {
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
  $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  $n = @(); foreach ($e in $all) { $n += $e.Current.Name }
  return ($n -join ' | ')
}
function Invokables($hwnd) {
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
  $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  $r = @(); foreach ($e in $all) { $p = $null; if ($e.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $r += ,@($e, $p) } }
  return $r
}
function FocusedDesc() {
  try { $f = [System.Windows.Automation.AutomationElement]::FocusedElement; if ($f -eq $null) { return '(nenhum)' }; return ($f.Current.ControlType.ProgrammaticName + ' "' + $f.Current.Name + '" (pid ' + $f.Current.ProcessId + ')') } catch { return '(erro: ' + $_.Exception.Message + ')' }
}

$data = Join-Path $Work 'data'
New-Item -ItemType Directory -Force $data | Out-Null
$ws = @'
{"schema_version":3,"generation":1,"instances":[
{"id":"000000000000000000000000000000b1","type_id":"pod.water","state_version":1,"state":"{\"goal_ml\":2000,\"glass_ml\":250,\"day\":null,\"drunk_ml\":0}","visible":true,"shape":"2x2",
 "placement":{"monitor":null,"offset_x_dip":24.0,"offset_y_dip":24.0,"width_dip":188.0,"height_dip":188.0,"ratio_x":0.01,"ratio_y":0.02,"ref_work_w_dip":1920.0,"ref_work_h_dip":1032.0}},
{"id":"000000000000000000000000000000b2","type_id":"pod.quick-notes","state_version":1,"state":"nota de teste","visible":true,
 "placement":{"monitor":null,"offset_x_dip":300.0,"offset_y_dip":24.0,"width_dip":320.0,"height_dip":240.0,"ratio_x":0.2,"ratio_y":0.02,"ref_work_w_dip":1920.0,"ref_work_h_dip":1032.0}}]}
'@
Set-Content -Path (Join-Path $data 'workspace.json') -Value $ws -Encoding ascii

$app = $null; $hproc = $null
try {
  Say ("=== " + $Label + " ===")
  $app = Start-Process -FilePath $Exe -ArgumentList @('--data-dir', $data) -PassThru
  $wins = @()
  for ($i = 0; $i -lt 60 -and $wins.Count -lt 2; $i++) { Start-Sleep -Milliseconds 250; $wins = @([W]::Find([uint32]$app.Id, 'PodWidget')) }
  if ($wins.Count -lt 2) { throw 'esperava 2 janelas (cartao e nota)' }
  Start-Sleep -Milliseconds 600
  $tile = $null; $note = $null
  foreach ($w in $wins) { $r = New-Object W+RECT; [void][W]::GetWindowRect($w, [ref]$r); if (($r.Right - $r.Left) -lt 250) { $tile = $w } else { $note = $w } }
  $ex = { param($h) '{0:x}' -f ([int64][W]::GetWindowRect) }  # (so para manter o bloco curto)
  Say ("app real, pid " + $app.Id + "; cartao 0x" + ('{0:x}' -f [int64]$tile) + " e nota 0x" + ('{0:x}' -f [int64]$note))
  $exs = Add-Type -MemberDefinition '[DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GLP(IntPtr h, int i);' -Name 'X' -Namespace 'N' -PassThru
  $es = [int64]($exs::GLP($tile, -20))
  Say ("estilo estendido do cartao: 0x" + ('{0:x}' -f $es) + " -> WS_EX_NOACTIVATE (0x08000000) " + $(if ($es -band 0x08000000) { 'PRESENTE' } else { 'ausente' }))
  $en = [int64]($exs::GLP($note, -20))
  Say ("estilo estendido da nota:   0x" + ('{0:x}' -f $en) + " -> WS_EX_NOACTIVATE " + $(if ($en -band 0x08000000) { 'PRESENTE' } else { 'ausente' }))

  # ---------------- 1. UIA Invoke ----------------
  Say '1. UI Automation: Invoke no botao do cartao (nao usa mouse nem teclado)'
  $before = Names $tile
  $inv = @(Invokables $tile)
  Say ("   elementos invocaveis no cartao: " + $inv.Count + " (" + (($inv | ForEach-Object { $_[0].Current.Name }) -join ', ') + ")")
  if ($inv.Count -ge 1) {
    try { $inv[0][1].Invoke(); Start-Sleep -Milliseconds 700; $after = Names $tile; Say ("   Invoke executou sem erro; o conteudo do cartao mudou: " + ($before -ne $after)); Say ("      antes: " + $before); Say ("      depois: " + $after) }
    catch { Say ("   Invoke FALHOU: " + $_.Exception.Message) }
  }

  # ---------------- onde deixar cartao e nota visiveis (lugares livres da area de trabalho, separados) ----------------
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
  $rt0 = New-Object W+RECT; [void][W]::GetWindowRect($tile, [ref]$rt0); $rn0 = New-Object W+RECT; [void][W]::GetWindowRect($note, [ref]$rn0)
  $tw = $rt0.Right - $rt0.Left; $th = $rt0.Bottom - $rt0.Top; $nw = $rn0.Right - $rn0.Left; $nh = $rn0.Bottom - $rn0.Top
  $spotT = FreeSpot $tw $th @()
  if (-not $spotT) { throw 'sem area de trabalho livre para o cartao' }
  $spotN = FreeSpot $nw $nh @(,@($spotT[0], $spotT[1], $tw, $th))
  if (-not $spotN) { throw 'sem area de trabalho livre para a nota' }
  $spot = $spotT
  [void][W]::SetWindowPos($tile, [IntPtr]::Zero, $spotT[0], $spotT[1], 0, 0, 0x0001 -bor 0x0004 -bor 0x0010)
  [void][W]::SetWindowPos($note, [IntPtr]::Zero, $spotN[0], $spotN[1], 0, 0, 0x0001 -bor 0x0004 -bor 0x0010)
  Start-Sleep -Milliseconds 500
  $rt = New-Object W+RECT; [void][W]::GetWindowRect($tile, [ref]$rt); $rn = New-Object W+RECT; [void][W]::GetWindowRect($note, [ref]$rn)
  $tcx = [int](($rt.Left + $rt.Right) / 2); $tcy = [int](($rt.Top + $rt.Bottom) / 2); $ncx = [int](($rn.Left + $rn.Right) / 2); $ncy = [int](($rn.Top + $rn.Bottom) / 2)
  if (([W]::At($tcx, $tcy) -ne $tile) -or ([W]::At($ncx, $ncy) -ne $note)) { throw 'cartao ou nota nao ficaram visiveis' }
  Say ("cartao visivel em " + $spotT[0] + "," + $spotT[1] + " e nota em " + $spotN[0] + "," + $spotN[1] + " (area de trabalho livre)")

  # ---------------- outro programa (processo separado) ----------------
  $avoidH = @(); $avoidH += ,@($spotT[0], $spotT[1], $tw, $th); $avoidH += ,@($spotN[0], $spotN[1], $nw, $nh)
  $spotH = FreeSpot 420 220 $avoidH
  if ($spotH) { $fx = $spotH[0]; $fy = $spotH[1] } else { $fx = $spotT[0] + 900; $fy = $spotT[1] + 400 }
  $hproc = Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $HelperScript, '-Dir', $Work, '-X', $fx, '-Y', $fy) -PassThru
  $hw = [IntPtr]::Zero
  for ($i = 0; $i -lt 80; $i++) { Start-Sleep -Milliseconds 250; $f = Join-Path $Work 'hwnd.txt'; if (Test-Path $f) { $hw = [IntPtr][int64](Get-Content $f -Raw).Trim(); break } }
  if ($hw -eq [IntPtr]::Zero) { throw 'janela do outro programa nao apareceu' }
  $hr = New-Object W+RECT; [void][W]::GetWindowRect($hw, [ref]$hr)
  $hcx = [int](($hr.Left + $hr.Right) / 2); $hcy = [int](($hr.Top + $hr.Bottom) / 2)

  # ---------------- 2. clique real no cartao com outro programa em foco ----------------
  Say '2. CLIQUE REAL no cartao (5 vezes) com outro programa em primeiro plano'
  $keep = 0; $nul = 0; $deact = 0
  for ($k = 1; $k -le 5; $k++) {
    Click $hcx $hcy   # foco no campo de texto do outro programa
    ToFront $hw
    $n0 = (HelperLines).Count
    Click $tcx $tcy
    $fg = [W]::GetForegroundWindow()
    if ($fg -eq $hw) { $keep++ } elseif ($fg -eq [IntPtr]::Zero) { $nul++ }
    $deact += @((HelperLines) | Select-Object -Skip $n0 | Where-Object { $_ -match 'DESATIVADO' }).Count
  }
  Say ("   o outro programa MANTEVE o primeiro plano em " + $keep + "/5; 'nenhuma janela' em " + $nul + "/5; desativacoes registradas pelo outro programa: " + $deact + "/5")

  # ---------------- 3. Espaco/Enter no cartao ----------------
  Say '3. TECLADO no cartao (Espaco/Enter acionam o primeiro botao quando o cartao tem o foco do teclado)'
  # 3a. depois de um clique real no cartao (caminho do mouse)
  $b3 = Names $tile
  Click $hcx $hcy; ToFront $hw
  Click $tcx $tcy
  Key 0x20
  $a3 = Names $tile
  Say ("   3a. clique real no cartao + tecla Espaco: o cartao reagiu (mudou o conteudo) = " + ($b3 -ne $a3) + "; primeiro plano agora = " + [W]::Cls([W]::GetForegroundWindow()))
  # 3b. UIA SetFocus na raiz do cartao e Espaco
  Click $hcx $hcy; ToFront $hw
  $b3 = Names $tile
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($tile)
  $sf = 'ok'; try { $root.SetFocus() } catch { $sf = 'ERRO: ' + $_.Exception.Message }
  Start-Sleep -Milliseconds 400
  $fdesc = FocusedDesc
  Key 0x20
  $a3 = Names $tile
  Say ("   3b. UIA SetFocus na raiz do cartao: " + $sf + "; elemento com foco depois: " + $fdesc)
  Say ("       Espaco depois do SetFocus: o cartao reagiu = " + ($b3 -ne $a3) + "; primeiro plano = " + [W]::Cls([W]::GetForegroundWindow()))
  # 3c. UIA Invoke continua funcionando com outro programa em foco
  Click $hcx $hcy; ToFront $hw
  $inv = @(Invokables $tile); $b3 = Names $tile
  if ($inv.Count -ge 1) { try { $inv[0][1].Invoke(); Start-Sleep -Milliseconds 700; $a3 = Names $tile; Say ("   3c. UIA Invoke com outro programa em foco: o cartao reagiu = " + ($b3 -ne $a3) + "; o outro programa continuou em primeiro plano = " + ([W]::GetForegroundWindow() -eq $hw)) } catch { Say ("   3c. Invoke FALHOU: " + $_.Exception.Message) } }

  # ---------------- 4. nota (F6) - nao deve mudar ----------------
  Say '4. NOTA: ativa ao clicar e F6 alterna o foco entre o editor e o botao de menu (nao deve mudar com a correcao)'
  Click $hcx $hcy; ToFront $hw
  Click $ncx $ncy
  $fgn = [W]::GetForegroundWindow()
  Say ("   clique na nota: a nota virou o primeiro plano = " + ($fgn -eq $note))
  $f0 = FocusedDesc
  Key 0x75; $f1 = FocusedDesc
  Key 0x75; $f2 = FocusedDesc
  Say ("   foco inicial: " + $f0)
  Say ("   depois de F6: " + $f1)
  Say ("   depois de F6 de novo: " + $f2)
  Say ("   F6 alterna: " + (($f0 -ne $f1) -and ($f1 -ne $f2)))

  # ---------------- 5. menu de contexto do cartao (usa SetForegroundWindow no proprio cartao) ----------------
  Say '5. MENU DE CONTEXTO do cartao (botao direito): abre, teclado, Esc e clique fora'
  Click $hcx $hcy; ToFront $hw
  RightClick $tcx $tcy
  Say ("   botao direito no cartao: o menu abriu = " + (MenuOpen $app.Id) + "; primeiro plano = " + [W]::Cls([W]::GetForegroundWindow()))
  Key 0x1B
  Say ("   Esc: o menu fechou = " + (-not (MenuOpen $app.Id)))
  RightClick $tcx $tcy
  $opened = MenuOpen $app.Id
  Click ($rn.Left - 150) ($rn.Top + 50)   # clique fora: area de trabalho ao lado
  Say ("   clique fora do menu: o menu fechou = " + ($opened -and -not (MenuOpen $app.Id)))
  Click $hcx $hcy; ToFront $hw
  $b5 = Names $tile
  RightClick $tcx $tcy
  Key 0x28; Key 0x0D   # Seta para baixo + Enter: executa o primeiro item do menu
  Start-Sleep -Milliseconds 600
  $a5 = Names $tile
  Say ("   teclado no menu (Seta para baixo + Enter): o menu fechou = " + (-not (MenuOpen $app.Id)) + "; um item foi executado (o cartao mudou) = " + ($b5 -ne $a5))
  if (MenuOpen $app.Id) { Key 0x1B }

  # ---------------- 6. arrastar o cartao com o mouse ----------------
  Say '6. ARRASTAR o cartao com o mouse (captura de mouse; o cartao continua sem ativar)'
  Click $hcx $hcy; ToFront $hw
  $r0 = New-Object W+RECT; [void][W]::GetWindowRect($tile, [ref]$r0)
  Drag $tcx $tcy 130 70
  $r1 = New-Object W+RECT; [void][W]::GetWindowRect($tile, [ref]$r1)
  Say ("   o cartao se moveu: de " + $r0.Left + "," + $r0.Top + " para " + $r1.Left + "," + $r1.Top + " (deslocamento " + ($r1.Left - $r0.Left) + "," + ($r1.Top - $r0.Top) + "); o outro programa continuou em primeiro plano = " + ([W]::GetForegroundWindow() -eq $hw))
}
finally {
  if ($hproc -and -not $hproc.HasExited) { Set-Content -Path (Join-Path $Work 'quit.txt') -Value 'q'; Start-Sleep -Milliseconds 900; if (-not $hproc.HasExited) { Stop-Process -Id $hproc.Id -Force } }
  if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
  Start-Sleep -Milliseconds 400
  $log | Set-Content -Path $Out -Encoding utf8
}
