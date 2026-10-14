# Scores silence stop (silence/: the voice detector) on recordings: Windows'
# voice reading a few sentences, mixed with quiet rooms,
# fans, music, typing and another person talking. Makes the clips the first
# time, then runs the evaluation in a release build (real CPU numbers).
# Writes .agents/qa/silence/report.md and prints it.
# Usage: powershell -File scripts/silence-eval.ps1 [-Fresh]
param([switch] $Fresh)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot
Set-Location $root
$clips = ".agents/qa/silence/clips"
$lines = [ordered]@{
  "word.wav" = @("Yo.", "")
  "s1.wav" = @("Okay, so I want to test whether hands free mode stops when I finish talking.", "")
  "s2.wav" = @("Let me think about what else to say here, because I usually pause in the middle.", "")
  "s3.wav" = @("And that is the end of my thought.", "")
  # A second, different voice for "someone talking nearby".
  "babble.wav" = @("So then we went down to the market, and the weather was lovely, and everyone was out. We picked up some bread and cheese, and a few apples, and then we sat by the river for a while and talked about the trip next month. Nobody could decide where to go first, so we flipped a coin, and it landed on the mountains. After that we walked back the long way, past the old library and the little park with the fountain.", "Microsoft Zira Desktop")
}
foreach ($name in $lines.Keys) {
  $out = "$clips/$name"
  if ($Fresh -or -not (Test-Path $out)) {
    New-Item -ItemType Directory -Force $clips | Out-Null
    # An empty -Voice through `powershell -File` arrives as a missing argument, so pass it only when set.
    $voice = if ($lines[$name][1]) { @("-Voice", $lines[$name][1]) } else { @() }
    & powershell -NoProfile -File scripts/tts-wav.ps1 -Text $lines[$name][0] -Out $out @voice
  }
}
cargo test -p likh --release silence_eval -- --ignored --nocapture
if ($LASTEXITCODE -ne 0) { throw "the evaluation failed" }
