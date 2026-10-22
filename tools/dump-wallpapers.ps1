param([string]$Out, [string[]]$Extra = @())
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory $Out -Force | Out-Null
$files = @()
$files += Get-ChildItem 'C:\Windows\Web\Wallpaper' -Recurse -Include *.jpg, *.png -ErrorAction SilentlyContinue
$files += Get-ChildItem 'C:\Windows\Web\4K\Wallpaper' -Recurse -Include *.jpg, *.png -ErrorAction SilentlyContinue
foreach ($e in $Extra) { if (Test-Path $e) { $files += Get-Item $e } }
$W = 960; $H = 540
$rows = @()
foreach ($f in $files) {
  try {
    $src = [System.Drawing.Image]::FromFile($f.FullName)
    $bmp = New-Object System.Drawing.Bitmap $W, $H, ([System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = 'HighQualityBicubic'
    $g.DrawImage($src, 0, 0, $W, $H)
    $g.Dispose(); $src.Dispose()
    $rect = New-Object System.Drawing.Rectangle 0, 0, $W, $H
    $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $stride = $data.Stride
    $buf = New-Object byte[] ($stride * $H)
    [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $buf, 0, $buf.Length)
    $bmp.UnlockBits($data); $bmp.Dispose()
    $name = ($f.Directory.Name + '_' + $f.BaseName) -replace '[^A-Za-z0-9_]', ''
    $ms = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter $ms
    $bw.Write([uint32]$W); $bw.Write([uint32]$H)
    $sum = 0.0
    for ($y = 0; $y -lt $H; $y++) {
      for ($x = 0; $x -lt $W; $x++) {
        $i = $y * $stride + $x * 3
        $bw.Write($buf[$i + 2]); $bw.Write($buf[$i + 1]); $bw.Write($buf[$i])
        $sum += 0.2126 * $buf[$i + 2] + 0.7152 * $buf[$i + 1] + 0.0722 * $buf[$i]
      }
    }
    $bw.Flush()
    [IO.File]::WriteAllBytes((Join-Path $Out ($name + '.rgb')), $ms.ToArray())
    $rows += "{0},{1:N0}" -f $name, ($sum / ($W * $H))
  } catch { $rows += "{0},ERRO {1}" -f $f.Name, $_.Exception.Message }
}
$rows | Set-Content (Join-Path $Out 'stats.csv')
$rows
