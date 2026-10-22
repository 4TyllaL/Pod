<#
.SYNOPSIS
  Gera THIRD-PARTY-LICENSES.txt com as dependências que entram no binário (build do pod-app para
  Windows x64) e o texto das licenças que acompanham cada crate. Falha se alguma licença estiver fora
  da lista permitida ou se um crate não trouxer arquivo de licença.
#>
param(
  [Parameter(Mandatory)] [string]$Out,
  [string]$Root
)
$ErrorActionPreference = "Stop"
if (-not $Root) { $Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path) }
Set-Location $Root

# Licenças aceitas (expressões SPDX compostas só por estas, combinadas com OR/AND/WITH).
$allowed = @("MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "Unlicense", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unicode-3.0", "0BSD")

# Só dependências normais que entram no executável (sem dev/build), no alvo do pacote.
$meta = cargo metadata --format-version 1 --locked --filter-platform x86_64-pc-windows-msvc | ConvertFrom-Json
$tree = cargo tree -p pod-app -e normal --target x86_64-pc-windows-msvc --prefix none --locked --format "{p}" 2>$null
$names = $tree | ForEach-Object { ($_ -split " ")[0] } | Sort-Object -Unique
$workspace = @("pod-app", "pod-model")
$pkgs = $meta.packages | Where-Object { $names -contains $_.name -and $workspace -notcontains $_.name } | Sort-Object name, version

$bad = @()
$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine("Licenças de terceiros incluídas no Pod")
[void]$sb.AppendLine("======================================")
[void]$sb.AppendLine("Gerado a partir do Cargo.lock por tools/third-party.ps1. O Pod não inclui nem carrega DLLs externas:")
[void]$sb.AppendLine("as dependências abaixo são usadas na compilação e ficam dentro do executável. Algumas (syn, quote,")
[void]$sb.AppendLine("proc-macro2, serde_derive, windows-implement, windows-interface) são macros procedurais que só rodam")
[void]$sb.AppendLine("durante a build; estão listadas por serem parte da árvore de dependências.")
[void]$sb.AppendLine()
foreach ($p in $pkgs) {
  $lic = $p.license
  if (-not $lic) { $bad += "$($p.name) $($p.version): sem licença declarada"; continue }
  $parts = ($lic -replace "[()]", "") -split "\s+(?:OR|AND)\s+" | ForEach-Object { $_.Trim() }
  foreach ($part in $parts) { if ($allowed -notcontains $part) { $bad += "$($p.name) $($p.version): licença fora da lista permitida: $part" } }
  [void]$sb.AppendLine("* $($p.name) $($p.version) - $lic")
}
if ($bad.Count) { $bad | ForEach-Object { Write-Error $_ -ErrorAction Continue }; throw "Revisão de licenças falhou ($($bad.Count) problema(s))." }

[void]$sb.AppendLine()
[void]$sb.AppendLine("Textos das licenças")
[void]$sb.AppendLine("-------------------")
$seen = @{}
foreach ($p in $pkgs) {
  $dir = Split-Path $p.manifest_path
  $files = Get-ChildItem $dir -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -match "^(LICENSE|LICENCE|COPYING)" } | Sort-Object Name
  if (-not $files) { throw "$($p.name) $($p.version): nenhum arquivo de licença no crate ($dir)." }
  foreach ($f in $files) {
    $text = (Get-Content $f.FullName -Raw -Encoding UTF8).TrimEnd()
    $key = [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($text)))
    if ($seen.ContainsKey($key)) { $seen[$key] += ", $($p.name)"; continue }
    $seen[$key] = $p.name
    [void]$sb.AppendLine()
    [void]$sb.AppendLine("=== $($p.name) $($p.version) ($($f.Name)) ===")
    [void]$sb.AppendLine($text)
    [void]$sb.AppendLine("=== (mesmo texto também em outros crates com o mesmo arquivo de licença) ===")
  }
}
[IO.File]::WriteAllText($Out, ($sb.ToString() -replace "`r?`n", "`r`n"), (New-Object Text.UTF8Encoding $true))
"licenças: $($pkgs.Count) crates revisados, $($seen.Count) textos distintos -> $Out"
