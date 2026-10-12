# Saves a PNG of a process's window by asking Windows for the window's own
# pixels (PrintWindow), not by copying the screen: it works while the window
# sits behind others, never brings it to the front, and never captures anything
# else on the person's screen.
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
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
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
$r = New-Object Win+RECT
[Win]::GetClientRect($match, [ref]$r) | Out-Null
$bmp = New-Object System.Drawing.Bitmap ([int]($r.R - $r.L)), ([int]($r.B - $r.T))
$g = [System.Drawing.Graphics]::FromImage($bmp)
$dc = $g.GetHdc()
# PW_CLIENTONLY (1) | PW_RENDERFULLCONTENT (2): the client area, drawn by DWM even when covered.
$ok = [Win]::PrintWindow($match, $dc, 3)
$g.ReleaseHdc($dc)
if (-not $ok) { throw "Windows wouldn't print the likh window." }
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$bmp.Save((Join-Path (Get-Location) $Out), [System.Drawing.Imaging.ImageFormat]::Png)
"saved $Out ($($bmp.Width)x$($bmp.Height)), without bringing it forward"
