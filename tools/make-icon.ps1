<#
.SYNOPSIS
  Gera assets\pod.ico (cápsula com três grãos sobre gradiente violeta, rosa e coral), com várias resoluções em PNG.
  Rode só quando o desenho mudar; o .ico gerado é versionado para a build não depender do GDI+.
#>
param([string]$Out = (Join-Path $PSScriptRoot "..\assets\pod.ico"))
Add-Type -AssemblyName System.Drawing

function New-RoundedPath([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
  $p = New-Object System.Drawing.Drawing2D.GraphicsPath
  $d = $r * 2
  $p.AddArc($x, $y, $d, $d, 180, 90); $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
  $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90); $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
  $p.CloseFigure(); $p
}

function New-Png([int]$size) {
  $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = 'AntiAlias'; $g.Clear([System.Drawing.Color]::Transparent)
  $m = [float]($size * 0.04); $s = [float]($size - 2 * $m); $r = [float]($size * 0.23)
  $card = New-RoundedPath $m $m $s $s $r
  # Fundo: gradiente diagonal violeta -> rosa -> coral
  $br = New-Object System.Drawing.Drawing2D.LinearGradientBrush ([System.Drawing.PointF]::new($m, $m)), ([System.Drawing.PointF]::new($m + $s, $m + $s)), ([System.Drawing.Color]::FromArgb(255, 0x8A, 0x5C, 0xFF)), ([System.Drawing.Color]::FromArgb(255, 0xFF, 0x7A, 0x3D))
  $blend = New-Object System.Drawing.Drawing2D.ColorBlend 3
  $blend.Colors = @([System.Drawing.Color]::FromArgb(255, 0x8A, 0x5C, 0xFF), [System.Drawing.Color]::FromArgb(255, 0xFF, 0x5F, 0x8F), [System.Drawing.Color]::FromArgb(255, 0xFF, 0x7A, 0x3D))
  $blend.Positions = @(0.0, 0.55, 1.0)
  $br.InterpolationColors = $blend
  $g.FillPath($br, $card)
  # Realce suave no topo (alpha 70 -> 0 até a metade do cartão)
  $hl = New-Object System.Drawing.Drawing2D.LinearGradientBrush ([System.Drawing.PointF]::new(0, $m)), ([System.Drawing.PointF]::new(0, $m + $s)), ([System.Drawing.Color]::FromArgb(70, 255, 255, 255)), ([System.Drawing.Color]::FromArgb(0, 255, 255, 255))
  $hb = New-Object System.Drawing.Drawing2D.ColorBlend 3
  $hb.Colors = @([System.Drawing.Color]::FromArgb(70, 255, 255, 255), [System.Drawing.Color]::FromArgb(0, 255, 255, 255), [System.Drawing.Color]::FromArgb(0, 255, 255, 255))
  $hb.Positions = @(0.0, 0.5, 1.0)
  $hl.InterpolationColors = $hb
  $g.FillPath($hl, $card)
  # Capsula branca inclinada com tres graos
  $g.TranslateTransform([float]($size / 2), [float]($size / 2)); $g.RotateTransform(-38)
  $cw = [float]($s * 0.74); $ch = [float]($s * 0.34)
  $cap = New-RoundedPath ([float](-$cw / 2)) ([float](-$ch / 2)) $cw $ch ([float]($ch / 2))
  $g.FillPath([System.Drawing.Brushes]::White, $cap)
  $dr = [float]($ch * 0.25)
  $dots = @(@(-0.26, 0xFF, 0x5F, 0x8F), @(0.0, 0xFF, 0x7A, 0x3D), @(0.26, 0x8A, 0x5C, 0xFF))
  foreach ($d in $dots) {
    $b = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, $d[1], $d[2], $d[3]))
    $g.FillEllipse($b, [float]($cw * $d[0] - $dr), [float](-$dr), [float](2 * $dr), [float](2 * $dr))
  }
  $g.ResetTransform()
  $g.Dispose()
  $ms = New-Object System.IO.MemoryStream; $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
  , $ms.ToArray()
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$images = foreach ($s in $sizes) { , (New-Png $s) }
$ms = New-Object System.IO.MemoryStream; $bw = New-Object System.IO.BinaryWriter $ms
$bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$sizes.Count)   # ICONDIR
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
  $s = $sizes[$i]; $data = $images[$i]
  $bw.Write([byte]($(if ($s -ge 256) { 0 } else { $s }))); $bw.Write([byte]($(if ($s -ge 256) { 0 } else { $s })))
  $bw.Write([byte]0); $bw.Write([byte]0); $bw.Write([uint16]1); $bw.Write([uint16]32)
  $bw.Write([uint32]$data.Length); $bw.Write([uint32]$offset); $offset += $data.Length
}
foreach ($data in $images) { $bw.Write($data) }
$bw.Flush()
New-Item -ItemType Directory (Split-Path $Out) -Force | Out-Null
[IO.File]::WriteAllBytes($Out, $ms.ToArray())
"ícone gravado: $((Resolve-Path $Out).Path) ($($ms.Length) bytes, $($sizes.Count) resoluções)"
