param([Parameter(Mandatory = $true)][string]$Zip, [Parameter(Mandatory = $true)][string]$Work, [string]$Out)
# Verifica o PACOTE de distribuicao de ponta a ponta, em um ambiente isolado (pastas e chaves de registro de teste;
# o perfil real do usuario nao e tocado): extrai, confere checksums, instala, abre com dados novos (primeira execucao),
# confere os widgets padrao pela UI Automation, reabre (os padroes nao se repetem), atualiza com o app aberto e desinstala.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public static class V {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GLP(IntPtr h, int i);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  public static List<IntPtr> Find(uint pid, string cls) {
    var l = new List<IntPtr>();
    EnumWindows((h, _) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(128); GetClassName(h, sb, 128); if (p == pid && sb.ToString() == cls) l.Add(h); return true; }, IntPtr.Zero);
    return l;
  }
}
'@
$log = New-Object System.Collections.Generic.List[string]
function Say($s) { $log.Add($s); Write-Output $s }
function Names($h) {
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($h)
  $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  $n = @(); foreach ($e in $all) { if ($e.Current.Name) { $n += $e.Current.Name } }
  return ($n -join ' | ')
}
function StartPod($exe, $data) { return (Start-Process -FilePath $exe -ArgumentList @('--data-dir', $data) -PassThru) }
function WaitCards($proc, [int]$want) {
  $w = @()
  for ($i = 0; $i -lt 80; $i++) { Start-Sleep -Milliseconds 250; $w = @([V]::Find([uint32]$proc.Id, 'PodWidget') | Where-Object { [V]::IsWindowVisible($_) }); if ($w.Count -ge $want) { break } }
  Start-Sleep -Milliseconds 800
  return @([V]::Find([uint32]$proc.Id, 'PodWidget') | Where-Object { [V]::IsWindowVisible($_) })
}
function CountInstances($data) {
  $f = Join-Path $data 'workspace.json'
  for ($i = 0; $i -lt 40; $i++) { if (Test-Path $f) { $t = Get-Content $f -Raw; $c = ([regex]::Matches($t, '"type_id"')).Count; if ($c -gt 0) { return $c } }; Start-Sleep -Milliseconds 250 }
  return 0
}

$stage = Join-Path $Work 'extraido'; $inst = Join-Path $Work 'instalado'; $menu = Join-Path $Work 'menu'; $data = Join-Path $Work 'dados'
$ukey = 'HKCU:\Software\PodTestePacote\Uninstall\Pod'; $rkey = 'HKCU:\Software\PodTestePacote\Run'
$proc = $null
try {
  foreach ($d in $stage, $inst, $menu, $data) { if (Test-Path $d) { Remove-Item $d -Recurse -Force } }
  New-Item -ItemType Directory $menu, $data | Out-Null
  $realPod = @(Get-Process -Name 'Pod', 'pod-app' -ErrorAction SilentlyContinue)
  if ($realPod.Count -gt 0) { throw 'ha um Pod aberto no sistema; feche-o antes (o teste nao mexe no seu Pod)' }

  Say '1. EXTRAIR e conferir'
  Expand-Archive -Path $Zip -DestinationPath $stage
  $pkg = Get-ChildItem $stage -Directory | Select-Object -First 1
  $names = (Get-ChildItem $pkg.FullName -File | ForEach-Object Name) -join ', '
  Say ("   pasta " + $pkg.Name + ": " + $names)
  $bad = 0
  foreach ($line in (Get-Content (Join-Path $pkg.FullName 'SHA256SUMS.txt'))) { if (-not $line.Trim()) { continue }; $h, $n = $line -split ' \*', 2; if ((Get-FileHash (Join-Path $pkg.FullName $n) -Algorithm SHA256).Hash.ToLower() -ne $h) { $bad++ } }
  Say ("   checksums divergentes: " + $bad)
  $vi = (Get-Item (Join-Path $pkg.FullName 'Pod.exe')).VersionInfo
  Say ("   Pod.exe: versao " + $vi.FileVersion + ", produto " + $vi.ProductName)
  Say ("   LICENSE do pacote: " + ((Get-Content (Join-Path $pkg.FullName 'LICENSE') -TotalCount 1)))

  Say '2. INSTALAR (pastas e chaves de teste)'
  & (Join-Path $pkg.FullName 'install.ps1') -InstallDir $inst -StartMenuDir $menu -UninstallKey $ukey -RunKey $rkey | ForEach-Object { Say ("   " + $_) }
  Say ("   Pod.exe instalado: " + (Test-Path (Join-Path $inst 'Pod.exe')) + "; atalho no menu: " + (@(Get-ChildItem $menu -Recurse -Filter *.lnk).Count -gt 0) + "; entrada em Aplicativos: " + (Test-Path $ukey) + "; inicio com o Windows (deve ser False): " + ($null -ne (Get-ItemProperty $rkey -Name Pod -ErrorAction SilentlyContinue)))
  Say ("   entrada em Aplicativos: versao " + (Get-ItemProperty $ukey -ErrorAction SilentlyContinue).DisplayVersion)

  Say '3. PRIMEIRA EXECUCAO (dados novos): widgets padrao'
  $proc = StartPod (Join-Path $inst 'Pod.exe') $data
  $cards = WaitCards $proc 3
  Say ("   cartoes visiveis: " + $cards.Count + " (esperado 3: Relogio, Calendario e Sistema)")
  foreach ($c in $cards) {
    $ex = [int64][V]::GLP($c, -20)
    Say ("   - cartao 0x" + ('{0:x}' -f [int64]$c) + ": sem ativacao = " + [bool]($ex -band 0x08000000) + "; conteudo (UIA): " + (Names $c))
  }
  $n1 = CountInstances $data
  Say ("   workspace gravado com " + $n1 + " instancia(s)")
  Say ("   o processo esta rodando sem elevacao e sem janela de erro: " + (-not $proc.HasExited))
  Stop-Process -Id $proc.Id -Force; Start-Sleep -Milliseconds 700; $proc = $null

  Say '4. REABRIR com os mesmos dados: os padroes nao se repetem'
  $proc = StartPod (Join-Path $inst 'Pod.exe') $data
  $cards2 = WaitCards $proc 3
  Say ("   cartoes visiveis: " + $cards2.Count + " (esperado 3, nao 6); instancias no arquivo: " + (CountInstances $data))

  Say '5. ATUALIZAR com o Pod aberto (reinstala com -CloseRunning)'
  & (Join-Path $pkg.FullName 'install.ps1') -InstallDir $inst -StartMenuDir $menu -UninstallKey $ukey -RunKey $rkey -CloseRunning | ForEach-Object { Say ("   " + $_) }
  Start-Sleep -Milliseconds 1500
  $again = @(Get-Process -Name 'Pod' -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$inst*" })
  Say ("   o Pod fechou normalmente e voltou a rodar (instancias do Pod instalado: " + $again.Count + ")")
  foreach ($p in $again) { $proc = $p }
  $cards3 = if ($proc) { WaitCards $proc 3 } else { @() }
  Say ("   cartoes visiveis depois da atualizacao: " + $cards3.Count + "; instancias no arquivo: " + (CountInstances $data))
  if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force; Start-Sleep -Milliseconds 700 }; $proc = $null

  Say '6. DESINSTALAR (preserva os dados)'
  & (Join-Path $inst 'uninstall.ps1') -InstallDir $inst -StartMenuDir $menu -UninstallKey $ukey -RunKey $rkey | ForEach-Object { Say ("   " + $_) }
  Say ("   executavel removido: " + (-not (Test-Path (Join-Path $inst 'Pod.exe'))) + "; atalho removido: " + (@(Get-ChildItem $menu -Recurse -Filter *.lnk -ErrorAction SilentlyContinue).Count -eq 0) + "; entrada removida: " + (-not (Test-Path $ukey)) + "; dados preservados: " + (Test-Path (Join-Path $data 'workspace.json')))
}
finally {
  if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
  Get-Process -Name 'Pod' -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$Work*" } | Stop-Process -Force
  Remove-Item 'HKCU:\Software\PodTestePacote' -Recurse -Force -ErrorAction SilentlyContinue
  if ($Out) { $log | Set-Content -Path $Out -Encoding utf8 }
}
