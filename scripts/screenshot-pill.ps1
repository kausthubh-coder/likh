# Saves the bottom centre of the primary screen, where the pill lives, as PNGs.
# Run it alongside a dictation. Fails on a locked screen (nothing to capture).
# Usage: powershell -File scripts/screenshot-pill.ps1 -Out .agents/qa/pill/shot [-Count 8] [-EveryMs 250] [-DelayMs 0]
param(
  [Parameter(Mandatory)] [string] $Out,
  [int] $Count = 1,
  [int] $EveryMs = 250,
  [int] $DelayMs = 0
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Dpi {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
}
"@
# Real pixels, not the scaled ones a DPI-unaware process gets.
[Dpi]::SetProcessDPIAware() | Out-Null
$width = [Dpi]::GetSystemMetrics(0); $height = [Dpi]::GetSystemMetrics(1)
$box = New-Object System.Drawing.Rectangle ([int]($width / 2 - 500)), ($height - 320), 1000, 320
New-Item -ItemType Directory -Force (Split-Path (Join-Path (Get-Location) $Out)) | Out-Null
Start-Sleep -Milliseconds $DelayMs
for ($i = 0; $i -lt $Count; $i++) {
  $bmp = New-Object System.Drawing.Bitmap $box.Width, $box.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($box.X, $box.Y, 0, 0, $bmp.Size)
  $path = Join-Path (Get-Location) "$Out-$i.png"
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  "saved $Out-$i.png"
  Start-Sleep -Milliseconds $EveryMs
}
