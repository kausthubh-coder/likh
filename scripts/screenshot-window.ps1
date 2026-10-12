# Saves a PNG of a process's window, as it looks on screen (glass and all).
# Usage: powershell -File scripts/screenshot-window.ps1 -ProcessId 1234 -Out .agents/qa/shot.png [-Title likh]
param(
  [Parameter(Mandatory)] [int] $ProcessId,
  [Parameter(Mandatory)] [string] $Out,
  [string] $Title = "likh"
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices; using System.Collections.Generic;
public static class Win {
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
  public struct RECT { public int L, T, R, B; }
  public static List<IntPtr> For(uint want) {
    var found = new List<IntPtr>();
    EnumWindows((h, p) => { uint pid; GetWindowThreadProcessId(h, out pid); if (pid == want && IsWindowVisible(h)) found.Add(h); return true; }, IntPtr.Zero);
    return found;
  }
}
"@
[Win]::SetProcessDPIAware() | Out-Null
$match = [Win]::For([uint32]$ProcessId) | Where-Object {
  $sb = New-Object System.Text.StringBuilder 256; [Win]::GetWindowText($_, $sb, 256) | Out-Null; $sb.ToString() -eq $Title
} | Select-Object -First 1
if (-not $match) { throw "No visible window titled '$Title' for process $ProcessId" }
[Win]::SetForegroundWindow($match) | Out-Null
Start-Sleep -Milliseconds 500
if ([Win]::GetForegroundWindow() -ne $match) {
  throw "The likh window did not gain focus, so no screenshot was taken."
}
$r = New-Object Win+RECT
# DWMWA_EXTENDED_FRAME_BOUNDS (9): the visible frame, without the invisible resize border.
[Win]::DwmGetWindowAttribute($match, 9, [ref]$r, 16) | Out-Null
$bmp = New-Object System.Drawing.Bitmap ([int]($r.R - $r.L)), ([int]($r.B - $r.T))
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$bmp.Save((Join-Path (Get-Location) $Out), [System.Drawing.Imaging.ImageFormat]::Png)
"saved $Out ($($bmp.Width)x$($bmp.Height))"
