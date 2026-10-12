# Speaks a sentence into a WAV with Windows' built-in voice, so dictation can be
# tested without anyone talking. 16 kHz mono 16-bit, what murmur sends anyway.
# Usage: powershell -File scripts/tts-wav.ps1 -Text "Remind me to send the deck tomorrow at nine." -Out .agents/qa/speech.wav
param(
  [Parameter(Mandatory)] [string] $Text,
  [Parameter(Mandatory)] [string] $Out
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Speech
$path = Join-Path (Get-Location) $Out
New-Item -ItemType Directory -Force (Split-Path $path) | Out-Null
$voice = New-Object System.Speech.Synthesis.SpeechSynthesizer
$format = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo 16000, ([System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen), ([System.Speech.AudioFormat.AudioChannel]::Mono)
$voice.SetOutputToWaveFile($path, $format)
$voice.Speak($Text)
$voice.Dispose()
$seconds = [math]::Round(((Get-Item $path).Length - 44) / 32000, 2)
"saved $Out ($seconds s)"
