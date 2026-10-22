<#
.SYNOPSIS
  Verifica, com o tema REAL do Windows, que o Pod troca de claro para escuro (e volta).

.DESCRIPTION
  ATENÇÃO: este script ALTERA uma configuração do seu Windows (Personalização > Cores > "Escolha seu
  modo": o valor AppsUseLightTheme do usuário atual) e a restaura no final. Rode-o você mesmo; o
  assistente que escreveu o Pod não alterou nenhuma configuração do sistema.

  O que faz:
    1. Inicia o Pod de debug com dados em uma pasta temporária e o log ligado.
    2. Lê o tema atual do sistema.
    3. Inverte AppsUseLightTheme e envia o broadcast WM_SETTINGCHANGE "ImmersiveColorSet", como o
       próprio Windows faz.
    4. Espera e mostra as linhas "tema" do log do Pod, que devem mostrar a mudança de modo.
    5. Restaura o valor original e confere que o Pod volta ao modo anterior.

  Também grava capturas de tela (tema_antes.png, tema_depois.png) ao lado do script, da nota de teste.

.EXAMPLE
  .\tools\verify-theme.ps1
#>
param(
  [string]$PodExe = (Join-Path $PSScriptRoot "..\target\debug\pod-app.exe")
)
$ErrorActionPreference = "Stop"
if (-not (Test-Path $PodExe)) { throw "Compile antes: cargo build (esperado em $PodExe)" }

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public class PodThemeCheck {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageTimeoutW(IntPtr h, uint m, IntPtr w, string l, uint f, uint t, out IntPtr r);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  public struct R { public int l, t, r, b; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
}
'@
[PodThemeCheck]::SetProcessDPIAware() | Out-Null

$key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"
$original = (Get-ItemProperty $key).AppsUseLightTheme
$dataDir = Join-Path $env:TEMP "pod-verify-theme"
$log = Join-Path $dataDir "pod.log"
Remove-Item $dataDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $dataDir | Out-Null
$seed = '{"schema_version":2,"generation":1,"instances":[{"id":"000000000000000000000000000000a1","type_id":"pod.quick-notes","state_version":1,"state":"Teste de tema","visible":true,"placement":{"monitor":null,"offset_x_dip":0,"offset_y_dip":0,"width_dip":320.0,"height_dip":240.0,"ratio_x":0,"ratio_y":0,"ref_work_w_dip":0,"ref_work_h_dip":0,"legacy_abs":{"x":200,"y":200}}}]}'
[IO.File]::WriteAllText((Join-Path $dataDir "workspace.json"), $seed, (New-Object Text.UTF8Encoding $false))
$env:POD_DATA_DIR = $dataDir
$env:POD_LOG = $log

function Broadcast {
  $res = [IntPtr]::Zero
  # HWND_BROADCAST = 0xFFFF, WM_SETTINGCHANGE = 0x1A, SMTO_ABORTIFHUNG = 2
  [PodThemeCheck]::SendMessageTimeoutW([IntPtr]0xFFFF, 0x1A, [IntPtr]::Zero, "ImmersiveColorSet", 2, 5000, [ref]$res) | Out-Null
}
function Shot($name, $proc) {
  $h = $proc.MainWindowHandle
  $r = New-Object PodThemeCheck+R
  if (-not [PodThemeCheck]::GetWindowRect($h, [ref]$r)) { return }
  $bm = New-Object Drawing.Bitmap 400, 320
  $g = [Drawing.Graphics]::FromImage($bm)
  $g.CopyFromScreen($r.l - 20, $r.t - 20, 0, 0, $bm.Size)
  $bm.Save((Join-Path $PSScriptRoot "$name.png")); $g.Dispose(); $bm.Dispose()
}

$pod = Start-Process $PodExe -PassThru
try {
  Start-Sleep -Seconds 3
  $pod.Refresh()
  Write-Host "Tema do sistema agora: AppsUseLightTheme=$original ($(if ($original -eq 0) {'escuro'} else {'claro'}))"
  Shot "tema_antes" $pod
  Write-Host "`nInvertendo o tema do Windows (será restaurado no final)..."
  Set-ItemProperty $key -Name AppsUseLightTheme -Value ([int](-not [bool]$original))
  Broadcast
  Start-Sleep -Seconds 2
  Shot "tema_depois" $pod
  Write-Host "`nLog do Pod (linhas de tema):"
  Get-Content $log -Encoding UTF8 | Where-Object { $_ -like "tema*" } | ForEach-Object { "  $_" }
}
finally {
  Set-ItemProperty $key -Name AppsUseLightTheme -Value $original
  Broadcast
  Start-Sleep -Seconds 1
  Write-Host "`nTema restaurado: AppsUseLightTheme=$((Get-ItemProperty $key).AppsUseLightTheme)"
  Write-Host "Log depois de restaurar:"
  Get-Content $log -Encoding UTF8 | Where-Object { $_ -like "tema*" } | Select-Object -Last 2 | ForEach-Object { "  $_" }
  Stop-Process -Id $pod.Id -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 500
  Remove-Item $dataDir -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Host "`nEsperado: 'em uso' muda de Dark para Light (ou o inverso) depois da inversão e volta ao"
Write-Host "restaurar; tema_antes.png e tema_depois.png devem mostrar a nota com cores diferentes."
