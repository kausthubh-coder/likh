# Clicks inside a process's window at logical (DPI-independent) client coordinates,
# the same numbers iced layouts use. Usage:
#   powershell -File scripts/click-window.ps1 -ProcessId 1234 -X 280 -Y 600 [-Title likh]
# Agents: prefer `likh ctl` once it exists; this is for screens it can't reach.
param(
  [Parameter(Mandatory)] [int] $ProcessId,
  [Parameter(Mandatory)] [double] $X,
  [Parameter(Mandatory)] [double] $Y,
  [string] $Title = "likh"
)
$ErrorActionPreference = "Stop"
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices; using System.Collections.Generic;
public static class Clk {
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
  public struct POINT { public int X, Y; }
  public static List<IntPtr> For(uint want) {
    var found = new List<IntPtr>();
    EnumWindows((h, p) => { uint pid; GetWindowThreadProcessId(h, out pid); if (pid == want && IsWindowVisible(h)) found.Add(h); return true; }, IntPtr.Zero);
    return found;
  }
}
"@
[Clk]::SetProcessDPIAware() | Out-Null
$h = [Clk]::For([uint32]$ProcessId) | Where-Object {
  $sb = New-Object System.Text.StringBuilder 256; [Clk]::GetWindowText($_, $sb, 256) | Out-Null; $sb.ToString() -eq $Title
} | Select-Object -First 1
if (-not $h) { throw "No visible window titled '$Title' for process $ProcessId" }
$scale = [Clk]::GetDpiForWindow($h) / 96.0
$pt = New-Object Clk+POINT
$pt.X = [int]($X * $scale); $pt.Y = [int]($Y * $scale)
[Clk]::ClientToScreen($h, [ref]$pt) | Out-Null
[Clk]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 150
if ([Clk]::GetForegroundWindow() -ne $h) {
  throw "The likh window did not gain focus, so no click was sent."
}
[uint32]$targetPid = 0
[Clk]::GetWindowThreadProcessId([Clk]::WindowFromPoint($pt), [ref]$targetPid) | Out-Null
if ($targetPid -ne $ProcessId) {
  throw "Another window covers the click target, so no click was sent."
}
[Clk]::SetCursorPos($pt.X, $pt.Y) | Out-Null
[Clk]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero) # left down
Start-Sleep -Milliseconds 40
[Clk]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero) # left up
"clicked ($X, $Y) -> screen ($($pt.X), $($pt.Y)) at scale $scale"
