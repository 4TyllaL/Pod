param([Parameter(Mandatory = $true)][string]$Gallery, [string]$Out)
# Monta as imagens do README a partir da galeria offscreen (`Pod.exe --gallery <pasta>`): cartões reais do motor sobre um fundo em degradê.
# Uso: Pod.exe --gallery .\gal ; powershell -File tools\make-screenshots.ps1 -Gallery .\gal
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $Out) { $Out = Join-Path $here '..\docs\screenshots' }
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force $Out | Out-Null
$names = @('clock-2x2', 'calendar-2x2', 'overview-2x2', 'weather-2x2', 'music-2x2', 'reminders-2x2')
foreach ($theme in 'dark', 'light') {
  $imgs = $names | ForEach-Object { [System.Drawing.Image]::FromFile((Join-Path $Gallery "$_-$theme.png")) }
  $cw = $imgs[0].Width; $ch = $imgs[0].Height; $gap = [int]($cw * 0.12); $m = [int]($cw * 0.2)
  $W = 3 * $cw + 2 * $gap + 2 * $m; $H = 2 * $ch + $gap + 2 * $m
  $bmp = New-Object System.Drawing.Bitmap $W, $H
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = 'HighQuality'; $g.InterpolationMode = 'HighQualityBicubic'
  $rect = New-Object System.Drawing.Rectangle 0, 0, $W, $H
  if ($theme -eq 'dark') { $c1 = [System.Drawing.Color]::FromArgb(255, 30, 27, 75); $c2 = [System.Drawing.Color]::FromArgb(255, 124, 58, 237) }
  else { $c1 = [System.Drawing.Color]::FromArgb(255, 125, 170, 255); $c2 = [System.Drawing.Color]::FromArgb(255, 255, 175, 205) }
  $br = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect, $c1, $c2, 35.0
  $g.FillRectangle($br, $rect)
  for ($i = 0; $i -lt $imgs.Count; $i++) {
    $x = $m + ($i % 3) * ($cw + $gap); $y = $m + [math]::Floor($i / 3) * ($ch + $gap)
    $g.DrawImage($imgs[$i], $x, $y, $cw, $ch)
  }
  $g.Dispose(); $br.Dispose()
  $bmp.Save((Join-Path $Out "widgets-$theme.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose(); $imgs | ForEach-Object { $_.Dispose() }
}
# logo: a camada de 256 px do .ico (guardada como PNG) vira assets\brand\logo.png
$ico = [System.IO.File]::ReadAllBytes((Join-Path $here '..\assets\pod.ico'))
$count = [BitConverter]::ToUInt16($ico, 4)
for ($i = 0; $i -lt $count; $i++) {
  $e = 6 + 16 * $i
  if ($ico[$e] -eq 0) {
    $size = [BitConverter]::ToUInt32($ico, $e + 8); $off = [BitConverter]::ToUInt32($ico, $e + 12)
    $brand = Join-Path $here '..\assets\brand'
    New-Item -ItemType Directory -Force $brand | Out-Null
    $png = New-Object byte[] $size; [Array]::Copy($ico, $off, $png, 0, $size)
    [System.IO.File]::WriteAllBytes((Join-Path $brand 'logo.png'), $png)
  }
}
Write-Output "ok: $Out"
