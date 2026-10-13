# Speaks a sentence into a WAV with Windows' built-in voice, so dictation can be
# tested without anyone talking. 16 kHz mono 16-bit, what likh sends anyway.
# Usage: powershell -File scripts/tts-wav.ps1 -Text "Remind me to send the deck tomorrow at nine." -Out .agents/qa/speech.wav [-Voice "Microsoft Zira Desktop"]
param(
  [Parameter(Mandatory)] [string] $Text,
  [Parameter(Mandatory)] [string] $Out,
  [string] $Voice = ""
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Speech
$path = Join-Path (Get-Location) $Out
New-Item -ItemType Directory -Force (Split-Path $path) | Out-Null
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
if ($Voice -ne "") { $synth.SelectVoice($Voice) }
$format = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo 16000, ([System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen), ([System.Speech.AudioFormat.AudioChannel]::Mono)
$synth.SetOutputToWaveFile($path, $format)
$synth.Speak($Text)
$synth.Dispose()
$seconds = [math]::Round(((Get-Item $path).Length - 44) / 32000, 2)
"saved $Out ($seconds s)"
