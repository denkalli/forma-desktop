# Renders the app icon (packaging/flatpak/io.github.denkalli.Forma.svg) to the PNG used for the
# window icon (assets/icon-256.png) and to the Windows icon (packaging/windows/forma.ico).
# The results are committed, so this only needs to run again when the SVG changes.
#
# Needs resvg: cargo install resvg
$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
$desktop = Resolve-Path (Join-Path $here '..\..')
$svg = Join-Path $desktop 'packaging\flatpak\io.github.denkalli.Forma.svg'
$work = Join-Path $desktop 'target\icons'
New-Item -ItemType Directory -Force $work | Out-Null

$sizes = 16, 20, 24, 32, 40, 48, 64, 128, 256
foreach ($size in $sizes) {
    resvg --width $size --height $size $svg (Join-Path $work "$size.png")
    if ($LASTEXITCODE -ne 0) { throw "resvg failed for size $size" }
}
Copy-Item (Join-Path $work '256.png') (Join-Path $desktop 'assets\icon-256.png')

# An .ico file is a directory of images; Windows accepts PNG data for every entry.
$images = $sizes | ForEach-Object { , [IO.File]::ReadAllBytes((Join-Path $work "$_.png")) }
$stream = New-Object IO.MemoryStream
$writer = New-Object IO.BinaryWriter $stream
$writer.Write([UInt16]0)              # reserved
$writer.Write([UInt16]1)              # type: icon
$writer.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $size = $sizes[$i]
    $dim = if ($size -ge 256) { 0 } else { $size }  # 0 means 256
    $writer.Write([Byte]$dim)
    $writer.Write([Byte]$dim)
    $writer.Write([Byte]0)            # no palette
    $writer.Write([Byte]0)            # reserved
    $writer.Write([UInt16]1)          # colour planes
    $writer.Write([UInt16]32)         # bits per pixel
    $writer.Write([UInt32]$images[$i].Length)
    $writer.Write([UInt32]$offset)
    $offset += $images[$i].Length
}
foreach ($image in $images) { $writer.Write($image) }
$writer.Flush()
[IO.File]::WriteAllBytes((Join-Path $here 'forma.ico'), $stream.ToArray())

Write-Host "Wrote assets\icon-256.png and packaging\windows\forma.ico"
