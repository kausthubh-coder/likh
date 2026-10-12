# One dictation end to end, driven with `likh ctl` (start, wait, stop), then
# reads back what got pasted. Two ways to receive the paste:
#
# -PasteFile <file> (prefer it): likh was started with LIKH_PASTE=<the same file>,
#   so it writes the paste and key presses there instead of using the clipboard
#   and keyboard. No window, no focus, no clicks: the person at the computer
#   never notices.
# Without -PasteFile: opens a throwaway text box, clicks into it (this moves the
#   real cursor), and likh really pastes. Use it to prove the clipboard path.
#
# Start likh first with the audio and transcriber you want, for example:
#   LIKH_PASTE=.agents/qa/pasted.jsonl LIKH_AUDIO=.agents/qa/speech.wav LIKH_TRANSCRIBER=echo:hello target/release/likh.exe run --skip-onboarding
# Usage: powershell -File scripts/e2e-dictate.ps1 -Likh target/release/likh.exe -Seconds 3.5 [-Expected "hello"] [-PasteFile .agents/qa/pasted.jsonl]
# -LetSilenceStop never sends `stop`: likh must stop by itself (hands-free in
# "Tap to toggle" with silence stop on), within -Seconds.
param(
  [Parameter(Mandatory)] [string] $Likh,
  [Parameter(Mandatory)] [double] $Seconds,
  [string] $Expected = "",
  [string] $PasteFile = "",
  [switch] $LetSilenceStop
)
$ErrorActionPreference = "Stop"
$headless = $PasteFile -ne ""
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Fg {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[Fg]::SetProcessDPIAware() | Out-Null

function Pump([int] $ms) {
  $until = [DateTime]::Now.AddMilliseconds($ms)
  while ([DateTime]::Now -lt $until) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 15 }
}
$LikhPath = (Resolve-Path $Likh).Path
# likh.exe is a GUI-subsystem app (it runs from the tray), so PowerShell neither
# waits for `& likh.exe` nor keeps its output. Start it and read stdout directly.
function Ctl([string] $command) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo $LikhPath, "ctl $command"
  $psi.UseShellExecute = $false; $psi.RedirectStandardOutput = $true; $psi.CreateNoWindow = $true
  $process = [System.Diagnostics.Process]::Start($psi)
  $out = $process.StandardOutput.ReadToEnd()
  $process.WaitForExit()
  $out | ConvertFrom-Json
}
# Headless there is no box to lose, so "focused" is always true.
function Focused { $headless -or [Fg]::GetForegroundWindow() -eq $form.Handle }
function Quit([string] $why) {
  Ctl cancel | Out-Null
  if (-not $headless) { $form.Close() }
  throw $why
}

if ($headless) {
  $logged = if (Test-Path $PasteFile) { @(Get-Content $PasteFile).Count } else { 0 }
} else {
  $form = New-Object System.Windows.Forms.Form
  $form.Text = "likh e2e target"
  $form.Width = 640; $form.Height = 200; $form.StartPosition = "CenterScreen"; $form.TopMost = $true
  $box = New-Object System.Windows.Forms.TextBox
  $box.Multiline = $true; $box.Dock = "Fill"; $box.Font = New-Object System.Drawing.Font("Segoe UI", 12)
  $form.Controls.Add($box)
  $form.Show(); $form.Activate(); $box.Focus() | Out-Null
  Pump 400
  # Windows won't hand the foreground to a script, but a click always focuses what
  # it lands on, so click into our own box. Then refuse to go on unless it really
  # is in front: a paste must never land in someone's real app.
  $centre = $box.PointToScreen((New-Object System.Drawing.Point ([int]($box.Width / 2)), ([int]($box.Height / 2))))
  for ($try = 0; $try -lt 5 -and -not (Focused); $try++) {
    [Fg]::SetCursorPos($centre.X, $centre.Y) | Out-Null
    [Fg]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero); [Fg]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero)
    Pump 200
  }
  if (-not (Focused)) {
    $form.Close()
    throw "The test box never came to the front, so no dictation was started."
  }
}

$before = (Ctl status).dictation.dictations
$started = [DateTime]::Now
Ctl start | Out-Null
if ($LetSilenceStop) {
  # Watch for likh to stop by itself; it reports the transcribing phase once it has.
  # Anything else (no answer, the box losing focus, time running out) cancels.
  $deadline = [DateTime]::Now.AddSeconds($Seconds)
  do {
    Pump 50
    $phase = try { (Ctl status).dictation.phase } catch { $null }
    $focused = Focused
  } while ($phase -eq "hands_free" -and $focused -and [DateTime]::Now -lt $deadline)
  if (-not $focused -or $phase -notin @("transcribing", "idle")) {
    Quit "Stopped watching (phase '$phase', box focused: $focused), so the dictation was cancelled."
  }
} else {
  Pump ([int]($Seconds * 1000))
  # Someone clicked away mid-recording: cancel rather than paste into their app.
  if (-not (Focused)) { Quit "The test box lost focus while recording, so the dictation was cancelled." }
  Ctl stop | Out-Null
}
$released = [DateTime]::Now

# Wait for the paste: the dictation count goes up once the text is in.
$deadline = [DateTime]::Now.AddSeconds(30)
do { Pump 100; $status = Ctl status } while ($status.dictation.dictations -eq $before -and -not $status.dictation.last.error -and [DateTime]::Now -lt $deadline)
Pump 300
$presses = @()
if ($headless) {
  $new = @(Get-Content $PasteFile | Select-Object -Skip $logged | ForEach-Object { $_ | ConvertFrom-Json })
  $text = ($new | Where-Object { $_.paste -ne $null } | ForEach-Object { $_.paste }) -join ""
  $presses = @($new | Where-Object { $_.press -ne $null } | ForEach-Object { $_.press })
} else {
  $text = $box.Text
  $form.Close()
}

[pscustomobject]@{
  pasted      = $text
  pressed     = $presses
  expected    = $Expected
  matches     = ($Expected -eq "" -or $text.Trim() -eq $Expected.Trim())
  error       = $status.dictation.last.error
  latency_ms  = $status.dictation.last.latency_ms
  audio_s     = $status.dictation.last.audio_seconds
  release_to_seen_ms = [int]([DateTime]::Now - $released).TotalMilliseconds
  start_to_stop_ms = [int]($released - $started).TotalMilliseconds
} | ConvertTo-Json -Compress
