<#
.SYNOPSIS
  Gera o pacote de distribuição do Pod: build release determinística + ZIP versionado e reproduzível.

.DESCRIPTION
  1. Roda os testes (a menos que -SkipTests).
  2. Compila o release com --locked, caminhos remapeados e /Brepro, em uma pasta de build própria.
  3. Assina o executável SE POD_SIGN_CERT_THUMBPRINT estiver definido (Authenticode com signtool); sem
     certificado, o pacote sai NÃO ASSINADO e isso é dito no README e na saída.
  4. Monta Pod.exe + install/uninstall + README + LICENSE + licenças de terceiros + SHA256SUMS.txt.
  5. Gera Pod-<versão>-win-x64.zip com datas e ordem fixas: o mesmo código dá o mesmo ZIP.

  Variáveis opcionais de assinatura: POD_SIGN_CERT_THUMBPRINT, POD_SIGN_TIMESTAMP_URL.
#>
[CmdletBinding()]
param(
  [string]$OutDir,
  [string]$TargetDir,
  [switch]$SkipTests
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
if (-not $OutDir) { $OutDir = Join-Path $root "dist" }
if (-not $TargetDir) { $TargetDir = Join-Path $root "target\dist" }

$toml = Get-Content (Join-Path $root "Cargo.toml") -Raw
$m = [regex]::Match($toml, '(?s)\[workspace\.package\].*?version\s*=\s*"([^"]+)"')
if (-not $m.Success) { throw "Versão não encontrada em [workspace.package] do Cargo.toml." }
$version = $m.Groups[1].Value
Write-Host "Pod $version"

if (-not $SkipTests) {
  Write-Host "== testes"
  cargo test --locked --quiet
  if ($LASTEXITCODE -ne 0) { throw "Os testes falharam; pacote não gerado." }
}

Write-Host "== build release determinística"
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE ".rustup" }
$env:RUSTFLAGS = "--remap-path-prefix=$root=/pod --remap-path-prefix=$cargoHome=/cargo --remap-path-prefix=$rustupHome=/rustup -C target-feature=+crt-static -C link-arg=/Brepro"
$env:CARGO_INCREMENTAL = "0"
cargo build --release --locked --target-dir $TargetDir
if ($LASTEXITCODE -ne 0) { throw "A build release falhou." }
$built = Join-Path $TargetDir "release\pod-app.exe"
if (-not (Test-Path $built)) { throw "Executável não encontrado em $built." }

$stage = Join-Path $OutDir "Pod-$version"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory $stage -Force | Out-Null
Copy-Item $built (Join-Path $stage "Pod.exe")

$signState = "NÃO está assinado digitalmente (Authenticode)"
if ($env:POD_SIGN_CERT_THUMBPRINT) {
  $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -like "*\x64\*" } | Sort-Object FullName -Descending | Select-Object -First 1
  if (-not $signtool) { throw "signtool.exe não encontrado (Windows SDK)." }
  $tsUrl = if ($env:POD_SIGN_TIMESTAMP_URL) { $env:POD_SIGN_TIMESTAMP_URL } else { "" }
  $args = @("sign", "/sha1", $env:POD_SIGN_CERT_THUMBPRINT, "/fd", "SHA256")
  if ($tsUrl) { $args += @("/tr", $tsUrl, "/td", "SHA256") }
  & $signtool.FullName @args (Join-Path $stage "Pod.exe")
  if ($LASTEXITCODE -ne 0) { throw "signtool falhou." }
  $signState = "está assinado digitalmente (Authenticode)"
}

Copy-Item (Join-Path $root "tools\install.ps1") $stage
Copy-Item (Join-Path $root "tools\uninstall.ps1") $stage
Copy-Item (Join-Path $root "LICENSE") $stage
$readme = (Get-Content (Join-Path $root "tools\README.template.txt") -Raw -Encoding UTF8).Replace("@VERSION@", $version).Replace("@SIGNSTATE@", $signState)
[IO.File]::WriteAllText((Join-Path $stage "README.txt"), ($readme -replace "`r?`n", "`r`n"), (New-Object Text.UTF8Encoding $true))
& (Join-Path $root "tools\third-party.ps1") -Out (Join-Path $stage "THIRD-PARTY-LICENSES.txt") -Root $root

# Checksums dos arquivos do pacote (ordem fixa, CRLF).
$names = Get-ChildItem $stage -File | ForEach-Object Name | Sort-Object { $_ } -CaseSensitive
$sums = ($names | ForEach-Object { "{0} *{1}" -f (Get-FileHash (Join-Path $stage $_) -Algorithm SHA256).Hash.ToLower(), $_ }) -join "`r`n"
[IO.File]::WriteAllText((Join-Path $stage "SHA256SUMS.txt"), $sums + "`r`n", (New-Object Text.UTF8Encoding $false))

# ZIP determinístico: pasta raiz Pod-<versão>, ordem alfabética, datas fixas, sem atributos de arquivo.
Add-Type -AssemblyName System.IO.Compression
$zipPath = Join-Path $OutDir "Pod-$version-win-x64.zip"
if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
$fs = [IO.File]::Create($zipPath)
$zip = New-Object IO.Compression.ZipArchive($fs, [IO.Compression.ZipArchiveMode]::Create)
$fixed = [DateTimeOffset]::new(2026, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
foreach ($n in (Get-ChildItem $stage -File | ForEach-Object Name | Sort-Object { $_ } -CaseSensitive)) {
  $entry = $zip.CreateEntry("Pod-$version/$n", [IO.Compression.CompressionLevel]::Optimal)
  $entry.LastWriteTime = $fixed
  $es = $entry.Open(); $bytes = [IO.File]::ReadAllBytes((Join-Path $stage $n)); $es.Write($bytes, 0, $bytes.Length); $es.Dispose()
}
$zip.Dispose(); $fs.Dispose()
$zipHash = (Get-FileHash $zipPath -Algorithm SHA256).Hash.ToLower()
[IO.File]::WriteAllText("$zipPath.sha256", "$zipHash *$(Split-Path $zipPath -Leaf)`r`n", (New-Object Text.UTF8Encoding $false))

Write-Host ""
Write-Host "Pacote:  $zipPath ($([int]((Get-Item $zipPath).Length / 1KB)) KiB)"
Write-Host "ZIP SHA256: $zipHash"
Write-Host "Pod.exe SHA256: $((Get-FileHash (Join-Path $stage 'Pod.exe') -Algorithm SHA256).Hash.ToLower())"
Write-Host "Assinatura: $signState"
