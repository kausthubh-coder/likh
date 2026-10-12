# Measures what a running murmur costs while nobody touches it: memory after it
# settles, and CPU time spent over a quiet window. Run it on a release build.
# Usage: powershell -File scripts/measure-idle.ps1 -ProcessId 1234 [-Settle 20] [-Window 30]
param(
  [Parameter(Mandatory)] [int] $ProcessId,
  [int] $Settle = 20,
  [int] $Window = 30
)
$ErrorActionPreference = "Stop"
Start-Sleep -Seconds $Settle
$before = Get-Process -Id $ProcessId
$cpuBefore = $before.TotalProcessorTime.TotalMilliseconds
Start-Sleep -Seconds $Window
$after = Get-Process -Id $ProcessId
[pscustomobject]@{
  pid           = $ProcessId
  private_mb    = [math]::Round($after.PrivateMemorySize64 / 1MB, 1)
  working_set_mb = [math]::Round($after.WorkingSet64 / 1MB, 1)
  cpu_ms        = [math]::Round($after.TotalProcessorTime.TotalMilliseconds - $cpuBefore)
  window_s      = $Window
  threads       = $after.Threads.Count
} | ConvertTo-Json -Compress
