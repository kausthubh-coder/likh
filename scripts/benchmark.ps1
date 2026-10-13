# What a running likh costs, measured from outside so measuring costs likh
# nothing: idle before a dictation, while recording, and idle again after
# (does memory come back?). Starts its own likh with its own LIKH_HOME, so the
# person's likh, settings and history are untouched, and stops it by PID.
#
# The dictation is hands-free with silence stop off, so it records for the
# whole window; the transcriber is faked (nothing leaves the computer) and the
# paste goes to a log. By default it records the real microphone (what users
# run; the audio is thrown away); -Audio <wav> replays a file instead.
#
# Each run appends one JSON line to .agents/qa/bench/results.jsonl and prints
# every run so far as a table, so runs can be compared over time.
#
# Usage: powershell -File scripts/benchmark.ps1 [-Label <text>]
#          [-Likh target/release/likh.exe] [-Audio <wav>] [-Settle 10] [-Idle 20] [-Record 30]
param(
  [string] $Likh = "target/release/likh.exe",
  [string] $Audio = "",
  [string] $Label = "",
  [int] $Settle = 10,
  [int] $Idle = 20,
  [int] $Record = 30
)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot
Set-Location $root
if (-not (Test-Path $Likh)) { throw "No build at $Likh. Run: cargo build -p likh --release" }
$LikhPath = (Resolve-Path $Likh).Path
$bench = Join-Path $root ".agents/qa/bench"
$home_ = Join-Path $bench "home"
New-Item -ItemType Directory -Force $home_ | Out-Null
# Its own shortcut, so it never fights the person's likh for Ctrl+Space.
@{
  shortcut = "Ctrl+Alt+Shift+F9"; mouse = "off"; mode = "tap"; engine = "codex"; silence_stop = "off"
  pause_media = $false; history = $false; sounds = $false; commands = $false
  start_with_windows = $false; auto_update = $false
} | ConvertTo-Json | ForEach-Object {
  # Windows PowerShell's utf8 adds a BOM, which settings.json can't start with.
  [IO.File]::WriteAllText((Join-Path $home_ "settings.json"), $_, (New-Object Text.UTF8Encoding $false))
}

# These reach only the likh this script starts (and its `likh ctl` calls).
$env:LIKH_HOME = $home_
$env:LIKH_TRANSCRIBER = "echo:benchmark"
$env:LIKH_PASTE = Join-Path $home_ "pasted.jsonl"
if ($Audio -ne "") { $env:LIKH_AUDIO = (Resolve-Path $Audio).Path } else { Remove-Item Env:LIKH_AUDIO -ErrorAction SilentlyContinue }

# likh.exe is a GUI-subsystem app, so read `likh ctl` output directly.
function Ctl([string] $command) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo $LikhPath, "ctl $command"
  $psi.UseShellExecute = $false; $psi.RedirectStandardOutput = $true; $psi.CreateNoWindow = $true
  $process = [System.Diagnostics.Process]::Start($psi)
  $out = $process.StandardOutput.ReadToEnd()
  $process.WaitForExit()
  if ($out) { $out | ConvertFrom-Json } else { $null }
}

# Samples one process over a window: CPU time spent, and memory at the end and at its highest.
function Sample-Likh([int] $processId, [int] $seconds) {
  $start = Get-Process -Id $processId
  $cpu0 = $start.TotalProcessorTime.TotalMilliseconds
  $clock = [Diagnostics.Stopwatch]::StartNew()
  $peakPrivate = 0; $peakWs = 0
  while ($clock.Elapsed.TotalSeconds -lt $seconds) {
    Start-Sleep -Milliseconds 500
    $p = Get-Process -Id $processId
    $peakPrivate = [math]::Max($peakPrivate, $p.PrivateMemorySize64)
    $peakWs = [math]::Max($peakWs, $p.WorkingSet64)
  }
  $end = Get-Process -Id $processId
  $cpu = $end.TotalProcessorTime.TotalMilliseconds - $cpu0
  $ms = $clock.Elapsed.TotalMilliseconds
  [ordered]@{
    seconds = [math]::Round($ms / 1000, 1)
    cpu_ms = [math]::Round($cpu)
    cpu_pct_of_one_core = [math]::Round(100 * $cpu / $ms, 2)
    private_mb = [math]::Round($end.PrivateMemorySize64 / 1MB, 1)
    peak_private_mb = [math]::Round($peakPrivate / 1MB, 1)
    working_set_mb = [math]::Round($end.WorkingSet64 / 1MB, 1)
    peak_working_set_mb = [math]::Round($peakWs / 1MB, 1)
    threads = $end.Threads.Count
  }
}

$process = Start-Process -FilePath $LikhPath -ArgumentList "run", "--skip-onboarding" -PassThru
$likhId = $process.Id
try {
  $deadline = [DateTime]::Now.AddSeconds(20)
  do { Start-Sleep -Milliseconds 300; $status = try { Ctl status } catch { $null } } while (-not $status.ok -and [DateTime]::Now -lt $deadline)
  if (-not $status.ok) { throw "likh (pid $likhId) never answered likh ctl status" }
  Start-Sleep -Seconds $Settle
  $before = Sample-Likh $likhId $Idle

  Ctl start | Out-Null
  # The phase is reported once the mic is open (300 to 600 ms).
  $deadline = [DateTime]::Now.AddSeconds(5)
  do { Start-Sleep -Milliseconds 100; $phase = (Ctl status).dictation.phase } while ($phase -eq "idle" -and [DateTime]::Now -lt $deadline)
  if ($phase -ne "hands_free") { throw "the dictation didn't start hands-free (phase '$phase')" }
  $recording = Sample-Likh $likhId $Record
  $stillRecording = (Ctl status).dictation.phase -eq "hands_free"
  Ctl stop | Out-Null
  $deadline = [DateTime]::Now.AddSeconds(15)
  do { Start-Sleep -Milliseconds 200; $status = Ctl status } while ($status.dictation.phase -ne "idle" -and [DateTime]::Now -lt $deadline)

  Start-Sleep -Seconds 3
  $after = Sample-Likh $likhId $Idle
} finally {
  try { Ctl quit | Out-Null } catch {}
  if (-not $process.WaitForExit(5000)) { Stop-Process -Id $likhId }
}

$result = [ordered]@{
  at = (Get-Date).ToString("s")
  label = $Label
  commit = (git rev-parse --short HEAD).Trim() + $(if (git status --porcelain -- app) { "+changes" } else { "" })
  source = $(if ($Audio -ne "") { Split-Path $Audio -Leaf } else { "microphone" })
  recorded_whole_window = $stillRecording
  idle = $before
  recording = $recording
  after = $after
}
$log = Join-Path $bench "results.jsonl"
($result | ConvertTo-Json -Compress -Depth 4) | Add-Content -Encoding utf8 $log

Get-Content $log | ForEach-Object { $_ | ConvertFrom-Json } | ForEach-Object {
  [pscustomobject]@{
    at = $_.at; label = $_.label; commit = $_.commit; source = $_.source
    "idle cpu %" = $_.idle.cpu_pct_of_one_core; "idle MB" = $_.idle.private_mb; "idle WS MB" = $_.idle.working_set_mb
    "rec cpu %" = $_.recording.cpu_pct_of_one_core; "rec peak MB" = $_.recording.peak_private_mb; "rec peak WS MB" = $_.recording.peak_working_set_mb
    "after cpu %" = $_.after.cpu_pct_of_one_core; "after MB" = $_.after.private_mb; "after WS MB" = $_.after.working_set_mb
  }
} | Format-Table -AutoSize | Out-String -Width 250
