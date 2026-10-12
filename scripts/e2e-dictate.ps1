# One dictation end to end, without touching anyone's real apps: opens a
# throwaway text box, focuses it, drives the running murmur with `murmur ctl`
# (start, wait, stop), then reads back what got pasted.
#
# Start murmur first with the audio and transcriber you want, for example:
#   MURMUR_AUDIO=.agents/qa/speech.wav MURMUR_TRANSCRIBER=echo:hello target/release/murmur.exe run --skip-onboarding
# Usage: powershell -File scripts/e2e-dictate.ps1 -Murmur target/release/murmur.exe -Seconds 3.5 [-Expected "hello"]
param(
  [Parameter(Mandatory)] [string] $Murmur,
  [Parameter(Mandatory)] [double] $Seconds,
  [string] $Expected = ""
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Fg {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[Fg]::SetProcessDPIAware() | Out-Null

$form = New-Object System.Windows.Forms.Form
$form.Text = "murmur e2e target"
$form.Width = 640; $form.Height = 200; $form.StartPosition = "CenterScreen"; $form.TopMost = $true
$box = New-Object System.Windows.Forms.TextBox
$box.Multiline = $true; $box.Dock = "Fill"; $box.Font = New-Object System.Drawing.Font("Segoe UI", 12)
$form.Controls.Add($box)
$form.Show(); $form.Activate(); $box.Focus() | Out-Null

function Pump([int] $ms) {
  $until = [DateTime]::Now.AddMilliseconds($ms)
  while ([DateTime]::Now -lt $until) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 15 }
}
function Ctl([string] $command) { (& $Murmur ctl $command) | ConvertFrom-Json }

Pump 400
# Windows won't hand the foreground to a script, but a click always focuses what
# it lands on, so click into our own box. Then refuse to go on unless it really
# is in front: a paste must never land in someone's real app.
$centre = $box.PointToScreen((New-Object System.Drawing.Point ([int]($box.Width / 2)), ([int]($box.Height / 2))))
for ($try = 0; $try -lt 5 -and [Fg]::GetForegroundWindow() -ne $form.Handle; $try++) {
  [Fg]::SetCursorPos($centre.X, $centre.Y) | Out-Null
  [Fg]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero); [Fg]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero)
  Pump 200
}
if ([Fg]::GetForegroundWindow() -ne $form.Handle) {
  $form.Close()
  throw "The test box never came to the front, so no dictation was started."
}
$before = (Ctl status).dictation.dictations
$started = [DateTime]::Now
Ctl start | Out-Null
Pump ([int]($Seconds * 1000))
if ([Fg]::GetForegroundWindow() -ne $form.Handle) {
  # Someone clicked away mid-recording: cancel rather than paste into their app.
  Ctl cancel | Out-Null
  $form.Close()
  throw "The test box lost focus while recording, so the dictation was cancelled."
}
Ctl stop | Out-Null
$released = [DateTime]::Now

# Wait for the paste: the dictation count goes up once the text is in.
$deadline = [DateTime]::Now.AddSeconds(30)
do { Pump 100; $status = Ctl status } while ($status.dictation.dictations -eq $before -and -not $status.dictation.last.error -and [DateTime]::Now -lt $deadline)
Pump 300
$text = $box.Text
$form.Close()

[pscustomobject]@{
  pasted      = $text
  expected    = $Expected
  matches     = ($Expected -eq "" -or $text.Trim() -eq $Expected.Trim())
  error       = $status.dictation.last.error
  latency_ms  = $status.dictation.last.latency_ms
  audio_s     = $status.dictation.last.audio_seconds
  release_to_seen_ms = [int]([DateTime]::Now - $released).TotalMilliseconds
} | ConvertTo-Json -Compress
